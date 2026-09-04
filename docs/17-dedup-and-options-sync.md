# 17. План: дедупликация файлов и синхронизация options.txt

Дата: 2026-09-04. Источник идеи: `deduplication-help.txt` (pnpm-подобный
Content-Addressable Storage на hardlinks).

---

## Часть 1. Дедупликация файлов сборок

### 1.1 Что уже есть (не строить заново)

`crates/tuffbox-core/src/mod_store.rs` — уже реализован pnpm-паттерн:

- Store root: `%LOCALAPPDATA%/TuffBox/modstore/objects/<xx>/<sha1>`.
- `try_hardlink(target, sha1)` — линкует объект в инстанс без скачивания.
- `record(file, sha1)` — атомарно (tmp + rename) складывает свежескачанный
  файл в store; перед записью проверяет реальный хеш.
- `gc()` — удаляет объекты, на которые нет ссылок (Unix: `nlink == 1`;
  Windows: консервативно по age > 30 дней).
- Путь вызова: `mod_files::materialize_mod_file_with_progress` — перед
  скачиванием пробует hardlink, после скачивания записывает в store.
- Fallback: любая ошибка store — не фатальна, идёт обычное скачивание
  (dedup — оптимизация, не требование).

Это покрывает **только моды** (`mods/*.jar`). Расширяем.

### 1.2 Что дедуплицировать (по приоритету)

| Категория | Папки | Выгода | Риск |
|-----------|-------|--------|------|
| Библиотеки Java / loader | `libraries/` из манифестов Mojang | Огромная: у 10 сборок на одну версию библиотеки идентичны | Нет — файлы неизменяемы |
| Assets Minecraft | `assets/` (звуки, текстуры ванилы) | Огромная: сотни МБ на версию | Нет |
| Ресурспаки | `resourcepacks/*.zip` | Высокая: игроки копируют паки между сборками | Низкий — zip обычно не редактируют |
| Шейдеры | `shaderpacks/*.zip` | Высокая — то же | Низкий |
| Моды | `mods/*.jar` | Уже сделано | — |
| Конфиги, saves, options.txt | `config/`, `saves/`, `options.txt` | — | **ЗАПРЕЩЕНО линковать** (мутируемые) |

Важно: не трогаем `config/`, `saves/`, `screenshots/`, `options.txt` —
hardlink сделает изменение в одной сборке видимым во всех (общий inode).

### 1.3 Архитектура: единый TuffStore вместо трёх

Переименование не обязательно; расширяем `mod_store` в общий модуль
`tuffbox-core/src/tuff_store.rs` (или переиспользуем mod_store с новыми
публичными функциями). Структура:

```
%LOCALAPPDATA%/TuffBox/store/
├── objects/<xx>/<sha1>          # все дедуплицируемые файлы
└── meta/<sha1>.json             # опционально: происхождение, readonly-флаг
```

Ключевое правило **одного тома**: store и инстансы должны быть на одном
диске. TuffBox разрешает произвольные папки проектов → проверяем том:
- Windows: сравнение префикса диска (`U:\` vs `C:\`) или `fs::hard_link`
  возвращает `CrossesDevices` → fallback на копирование (уже реализовано
  как «ошибка = нет хита»).
- Если проект на другом диске — store создаётся второй, локальный:
  `<project>/.tuffstore/objects/...` (тот же код, другой root).

### 1.4 Изменения по файлам

1. **`mod_files.rs`** — уже работает. Добавить материализацию
   resourcepacks/shaderpacks через тот же путь: у Modrinth-проектов типа
   `resourcepack`/`shader` тоже есть sha1 → `try_hardlink`/`record`
   работают без изменений кода. Нужно только пропустить их через
   `materialize_*` (сейчас, вероятно, отдельный путь установки).
2. **`mc_install.rs` (библиотеки)** — при скачивании библиотек из
   version manifest (есть `sha1` в манифесте Mojang) — обернуть тем же
   паттерном: `try_hardlink` → download → `record`. Наибольший выигрыш
   на одного игрока.
3. **`mc_install.rs` (assets)** — объекты < 1 МБ, их тысячи; hardlink на
   каждый файл ок, но выигрыш только между сборками на одной версии.
   Делать после библиотек.
4. **Ретроспективная дедупликация** — команда «Deduplicate existing packs»:
   - скан всех проектов TuffBox (список папок из реестра проектов);
   - для каждого `mods/*.jar`, `resourcepacks/*.zip`, `shaderpacks/*.zip`,
     `libraries/**` — посчитать sha1 (кэшировать в `mod_index_cache`);
   - если объект в store — заменить файл на hardlink
     (`remove_file` + `hard_link`, атомарно через tmp-name);
   - если нет — `record` его в store и линковать остальные копии.
   - UI: страница Maintenance/Diagnostics, прогресс + «освобождено N ГБ»
     (считать как `sum(size) - size_of(уникальных)`).
5. **GC** — уже есть; для Windows заменить age-эвристику: вести реестр
   ссылок (`meta/<sha1>.json` — список проектов) или расширить скан.
   Ближайший честный вариант: при ретро-скане/установке проверять
   `nlink` через `std::os::windows::fs::MetadataExt::number_of_links()`
   (стабильно с Rust 1.68+) → убрать age-эвристику целиком.
6. **Readonly-защита** — при `record()` ставить объекту read-only
   (`PermissionsExt` на Unix, `readonly(true)` на Windows). Игрок не
   испортит jar, который расшарен между 10 сборками.
7. **Проверка целостности** — `lookup()` уже перехеширует объект; для
   больших libraries это дорого → кэшировать результат проверки с TTL
   через `api_cache::get_or_insert_with_ttl` (паттерн уже в проекте).

### 1.5 Не делать (осознанно)

- Reflink/copy-on-write (APFS clonefile, ReFS) — вне охвата, NTFS-большинство.
- Дедупликация `saves/`, миров, `config/` — опасно и бесполезно.
- Symlink-режим — не нужен, hardlink + copy fallback достаточно.

---

## Часть 2. Синхронизация options.txt по группам версий MC

### 2.1 Требования пользователя

- Один общий `options.txt`, разделяемый между сборками.
- Группировка по **версиям Minecraft** (настройки несовместимы между
  мажорами: `1.21.x` vs `1.20.x` vs `1.12.2`).
- Возможность **переопределения** для конкретной сборки.
- **Никогда не перезаписывать** существующий options.txt инстанса
  (защита от потери настроек игрока).

### 2.2 Модель

```
%LOCALAPPDATA%/TuffBox/
└── options-profiles/
    ├── <group-id>/options.txt          # разделяемый шаблон группы
    └── registry.json                   # { "group-id": { "mc_versions": [...], "name": "..." } }
```

**Группы версий** — автоматические по data-epoch (уже есть
`ModpackEnvironment::data_epoch()` в `environment.rs` — переиспользуем!):

| Группа | Версии |
|--------|--------|
| `modern-1.21+` | 1.21, 1.21.x, 26.x (новые) |
| `1.20.x` | 1.20 – 1.20.6 |
| `1.19.x` | 1.19 – 1.19.4 |
| `1.18.x` | 1.18 – 1.18.2 |
| `1.17.x` | 1.17 – 1.17.1 |
| `1.16.x` | 1.16 – 1.16.5 |
| `legacy-1.12` | 1.13 и старше |

Правило: группа = по мажорной версии до первого несовместимого изменения
формата options.txt. При незнакомой версии — отдельная группа `mc-<version>`.

### 2.3 Алгоритм (строго без перезаписи)

Момент применения: `launcher::prepare_instance` (до запуска игры).

```
fn sync_options(instance_dir, mc_version):
    shared  = options_profiles/<group(mc_version)>/options.txt
    local   = instance_dir/options.txt
    stamp   = instance_dir/.tuffbox-options-managed   # маркер управления

    if local.exists():
        if !stamp.exists():
            return   # пользовательский файл — НИКОГДА не трогаем
        # файл под нашим управлением: сравнить содержимое
        if sha(local) == sha(shared): return
        # shared изменился → обновить локальный АТОМАРНО:
        #   1. бэкап: copy local -> instance_dir/options.txt.bak-<ts>
        #   2. copy shared -> tmp; rename tmp -> local
        #   (объединение ключей — см. 2.4)
    else:
        if shared.exists():
            copy shared -> local; create stamp
        else:
            return   # нечего синхронизировать; игра создаст свой
```

Когда **игрок меняет настройки в игре** — Minecraft перезаписывает
`local options.txt`. После выхода из игры (или при следующем запуске
TuffBox) — **записываем обратно**: copy local -> shared, если local
под управлением (stamp существует). Так общий файл «учится» у игрока.

### 2.4 Merge-стратегия при конфликте (local vs shared)

Ключ: `key=value` (парсер уже есть — `properties_parser.rs`).

Приоритет источника при обновлении shared → local:
- **Берём local** для всех ключей, которые игрок менял после последнего
  sync (хранить snapshot `last-synced.txt` рядом со stamp);
- **Берём shared** для ключей, которых нет в local (новые настройки
  пришли из другой сборки той же группы).

Реализация: три файла — shared, local, last-synced (значения на момент
прошлого sync). 3-way merge:
- local == last_synced → ключ не менялся игроком → взять shared;
- local != last_synced → игрок менял → оставить local.

### 2.5 Override для конкретной сборки

- `instance_dir/.tuffbox-options-managed` → отсутствие файла = сборка
  полностью независима (пользовательский режим).
- UI: в настройках сборки переключатель
  «Use shared options (group 1.21.x) / Independent options».
  - «Independent» = удалить stamp; локальный файл остаётся как есть.
  - Включение обратно = предложение импортировать текущий local в
    группу или принять групповой (диалог, не молча).
- Кнопка «Push current options to group» — скопировать local -> shared.

### 2.6 Удаление / миграции

- Удаление сборки никогда не удаляет shared-файл.
- При изменении версии MC сборки (обновление сборки на 1.22) — группа
  пересчитывается; stamp сохраняется, но первый sync новой группы
  делает merge (не overwrite) по правилам 2.4.
- `options.txt.bak-<ts>` — хранить последние 3, старше удалять.

### 2.7 Изменения по файлам

| Файл | Изменение |
|------|-----------|
| `crates/tuffbox-core/src/options_sync.rs` (новый) | группы версий, sync/merge/push, stamp-логика |
| `crates/tuffbox-core/src/launcher.rs::prepare_instance` | вызов `sync_options` перед запуском |
| `crates/tuffbox-core/src/launcher.rs` (после выхода процесса) | «write-back» local → shared |
| `apps/tuffbox-desktop/src-tauri/src/lib.rs` | Tauri-команды: get/set mode, push-to-group, list groups |
| `apps/tuffbox-desktop/src/components/...` | UI-переключатель режима + список групп |
| `crates/tuffbox-core/src/properties_parser.rs` | переиспользуется для 3-way merge |

### 2.8 Тесты

- `sync_creates_local_from_shared` — новая сборка получает групповой файл.
- `never_overwrites_user_file` — нет stamp → файл не тронут даже при
  разном содержимом.
- `backup_created_on_managed_update`.
- `three_way_merge_keeps_player_edits` — игрок менял `gamma`, shared
  менял `renderDistance` → в результате оба изменения.
- `override_mode_is_independent` — без stamp локальные правки не улетают
  в shared.
- `write_back_after_launch` — правки из игры попадают в shared.

---

## Часть 3. Порядок внедрения

1. **M1 — options.txt sync** (изолировано, высокий user-value, низкий риск):
   группы, stamp, 3-way merge, write-back, Tauri-команды, UI-переключатель.
2. **M2 — расширение dedup**: libraries → resourcepacks/shaderpacks →
   assets. Каждая категория — отдельный коммит с замером экономии.
3. **M3 — ретроспективная дедупликация + честный GC на Windows**
   (`number_of_links()`) + readonly-защита объектов.
4. **M4 — UI Maintenance-страница**: «освобождено X ГБ», статистика store,
   кнопка ретро-дедупликации, управление группами options.

Риски и границы:
- cross-device → fallback copy (уже в коде), локальный store на другой том.
- антивирусы/индексаторы Windows иногда держат файл открытым →
  hard_link может упасть ERROR_SHARING_VIOLATION → ретрай с задержкой,
  затем fallback copy.
- нелинковать ничего в `saves/`, `config/`, `screenshots/`.
