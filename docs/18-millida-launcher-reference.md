# Millida Launcher — референс-отчёт: что взять в TuffBox

**Дата:** 2026-10-02
**Источник:** `U:\TuffBox\milida-new\launcher-main` (Millida Launcher, GPL-3.0-only, Tauri 2 + React 19)
**Цель:** `U:\TuffBox\tuffbox` (TuffBox, MIT OR Apache-2.0, Tauri 2 + Svelte 5)
**Назначение файла:** быстрый справочный источник для ИИ-агентов. Все утверждения ниже проверены по коду обеих репозиториев, а не по README. Пути даны от корней репозиториев.

---

## TL;DR для агента

1. **Главный gap — автообновление TuffBox.** В TuffBox нет ни `minisign`, ни `selfupdate` (проверено grep: 0 совпадений), есть только проверка GitHub Releases без установки (`apps/tuffbox-desktop/src-tauri/src/integrations.rs:916`). Это невыполненный Stage 15 roadmap (`docs/07-development-roadmap.md:611`). Переносить: Millida `src-tauri/src/engine/core/selfupdate.rs` (673 стр.) + `installer/` (1729 стр.).
2. **Три реальные дыры запуска в TuffBox:** (а) нет Log4Shell-митигации, (б) пользовательские JVM-флаги идут без чёрного списка, (в) нет единой валидации недоверенных путей (есть только zip-slip для архивов).
3. **Нет `@argfile`** для длинного classpath на Windows (лимит 32767) — на больших Forge-сборках spawn молча упадёт.
4. **Не брать:** краш-анализ и граф-резолвер — в TuffBox они сильнее Millida. Всё, привязанное к `api.millida.net` (~20 кусков), переносить некуда.
5. **⚠️ Лицензия:** копирование GPL-кода делает бинарь TuffBox GPL-3.0-only. До копирования строк — выбрать стратегию (см. раздел «Лицензия»).

---

## 1. Профиль Millida (что за репо)

| Параметр | Значение |
|---|---|
| Стек | Rust (Tauri 2) + React 19, без роутера (переключение экранов классом `on`) |
| Объём Rust `src-tauri/src` | ≈ 44–45 тыс. строк (измерено 41 609 + мелкие файлы) |
| Объём фронта `src/` | ~90 сторов zustand, ~84 компонента, 17 экранов, ~145 тестов |
| IPC-команды | **244** в `generate_handler!` (`src-tauri/src/lib.rs:152–395`); в ARCHITECTURE.md заявлено ~135 — док устарел |
| Тесты | 83 модуля `#[cfg(test)]` в Rust; инвариантные тесты безопасности в `lib.rs:402–688` |
| CI | `.github/workflows/ci.yml`: typecheck/test/build фронта, `clippy -D warnings` + test (Linux/Windows), cargo-deny, cargo audit |

### Объёмы модулей Millida (точный замер строк)

| Модуль | Строк | Файлов |
|---|---:|---:|
| `src-tauri/src/engine/game/` | 14 281 | 24 |
| `src-tauri/src/engine/content/` | 9 369 | 16 |
| `src-tauri/src/engine/core/` | 4 840 | 16 |
| `src-tauri/src/engine/profiles/` | 4 080 | 12 |
| `src-tauri/src/engine/skins/` | 2 362 | 7 |
| `src-tauri/src/engine/accounts/` | 1 083 | 3 |
| `src-tauri/src/commands/` | 1 791 | 8 |
| корень (`lib.rs`, `secrets.rs`, `discord.rs`, …) | 2 897 | 7 |
| `installer/` | 1 729 | 6 |

Крупнейшие файлы: `game/launch.rs` 2765, `game/install.rs` 1899, `game/java.rs` 1482, `ipc/commands.ts` 1447 (TS), `content/deps.rs` 1203, `profiles/imports.rs` 1179, `game/crashcause.rs` 1151.

---

## 2. Текущее состояние лаунчерной части TuffBox (чтобы не брать лишнее)

Полный gap-чек-лист по TuffBox, проверено по коду:

| # | Возможность | Статус | Доказательство (путь:строка) |
|---|---|---|---|
| 1 | Microsoft-аккаунты (XBL/XSTS) | **Есть** | `apps/tuffbox-desktop/src-tauri/src/auth.rs`, `mc_list_accounts` `:2483` |
| 2 | Офлайн-режим | **Есть** | `auth.rs:1903` `mc_offline_login`; `crates/tuffbox-core/src/launcher.rs:93` `offline_uuid` |
| 3 | Скины/кейпы | **Есть** | `auth.rs:2826` `mc_apply_skin`; `cosmetics_local.rs`, `SkinPreview3D.svelte` |
| 4 | CurseForge | **Есть** | `crates/tuffbox-core/src/provider/curseforge.rs` + `murmur2.rs`; `lib.rs:16603` |
| 5 | Импорт из лаунчеров | **Частично** | `importer.rs:1225` (Prism/MultiMC), `:279` (CurseForge-паки). **Нет:** ATLauncher, GDLauncher, Modrinth App, `.minecraft` |
| 6 | Автообновление лаунчера | **НЕТ** | только `integrations.rs:916` `check_for_app_update` (проверка, без скачивания/установки). Grep `minisign|selfupdate` — 0 |
| 7 | Бэкапы миров | **Есть** | `src-tauri/src/worlds.rs:242,258,364,406,131` |
| 8 | Пинг серверов | **Есть** | `crates/tuffbox-core/src/servers_dat.rs:111`, `lib.rs:10597` |
| 9 | Изоляция инстансов | **Частично** | `create_instance` `lib.rs:19337` есть, **но** `launcher.rs:258` `prepare_instance` — `#[allow(dead_code)]`; запуск идёт из каталога проекта |
| 10 | Установка Java | **Есть** | `jre.rs:67` `required_java_major`, `:342` `ensure_java_for_minecraft`; `run_with_probe_timeout` `:393` |
| 11 | Deep-link/шаринг логов | **Есть** | `deep_link.rs:1` (`tuffbox://install?repo=…`), `lib.rs:19029` `share_log_mclogs` |
| 12 | Discord Rich Presence | **Есть** | `lib.rs:16443/16448`, `presence.rs` |

Launch-пайплайн рабочий целиком: `mc_install.rs` (~1470 стр., SHA-1 всех файлов, Fabric/Quilt meta-API), `forge_install.rs` (processor-джары Forge/NeoForge), `download_engine.rs` (конкурентные загрузки, resume через `.part`), `process.rs:864` (`spawn_and_track_with_cleanup` — piped-логи, orphan-watcher, kill), запуск через `spawn_blocking` (`lib.rs:15430` → `:16323`), события `launch-phase`/`launch-progress`/`launch-crashed`.

**Вывод:** гэпы TuffBox — не в «умении запускать», а в (1) автообновлении, (2) защите краевых случаев, (3) изоляции инстансов.

---

## 3. ТОП-приоритеты переноса (проверенные gaps)

### 3.1 Автообновление + инсталлер (закрывает Stage 15)

**Где в Millida:**
- `src-tauri/src/engine/core/selfupdate.rs` — **673 стр.**: свой парсер манифеста + проверка **minisign-подписи** (`minisign-verify` в `src-tauri/Cargo.toml:57`) тем же публичным ключом, что и плагин updater (`tauri.conf.json:228`); beta-канал; NSIS/AppImage/App bundle; fallback-эндпоинты `selfupdate.rs:17, 537–544` (`launcher-storage.millida.net`).
- `installer/` — **1729 стр.**, отдельный Rust-инсталлер: `install.rs` 604 (докачка 4 частями, ≤5 редиректов, манифест ≤64 КБ, payload ≤1 ГБ, single_instance), `place.rs` 414 (Unix), `ui.rs` 308 (Windows GUI), `manifest.rs` 198, `main.rs` 136. Верификация payload minisign (`installer/src/install.rs:9, :497`).
- `src-tauri/src/engine/core/selfheal.rs` (142) — «вебвью не отчитался о готовности → установка битая»; гейт сборки `scripts/check-frontend-gate.mjs` (в бандле обязан остаться вызов `frontend_ready`).

**Самодостаточность:** архитектура полностью отвязана; заменить константы URL и ключ.
**Трудоёмкость:** средняя (нужен свой minisign-ключ + статический хост манифеста, подойдут GitHub Releases).

### 3.2 Безопасность запуска — три дыры

| Gap | Реализация в Millida | Состояние TuffBox (проверено) |
|---|---|---|
| **Log4Shell** | `-Dlog4j2.formatMsgNoLookups=true` первым аргументом всегда (`game/launch.rs:1428–1429`); для 1.7–1.11.2 — заменяющий log4j-конфиг (`needs_log4j_config`/`ensure_log4j_config`, `game/install.rs:1264–1286`); тесты `launch.rs:1911–1956` | ❌ Нет ни флага, ни заменяющего конфига. Grep `formatMsgNoLookups` — 0. `crates/tuffbox-core/src/launcher.rs:452` ставит `-Dlog4j.configurationFile` — это Mojang-конфиг из version json, **не митигация** |
| **Опасные JVM-флаги** | Чёрный/белый список: отсекает `-Xbootclasspath`, `-classpath`, подмену лог4j-конфига (`launch.rs:814–850, 870–896, 998`), тесты `:1911–1956` | ❌ `options.jvm_args` идут в `cmd` как есть (`launcher.rs:429`). Угроза: манифест из `tuffbox://install?repo=…` может подсунуть `-javaagent`/`-classpath` |
| **Пути из внешних данных** | `engine/core/safepath.rs` (**250 стр.**): `safe_join`/`safe_child`/`safe_file_name` — отброс absolute (база молча заменяется `PathBuf::join`), `..`, управляющие символы, зарезервированные имена Windows, trailing dot/space; проверка по **канонизированному** пути | ⚠️ Частично: zip-slip-guard для архивов есть (`importer.rs:562, 583`, тесты `:1334`), для путей из mrpack/манифестов единой точки нет |

Все три — мелкие самодостаточные файлы с тестами. **Лучшее соотношение ценность/трудоёмкость в списке.**

### 3.3 Аргументный файл Windows (`@argfile`)

- Millida: `game/pack_launch.rs:234–268` (`write_argfile`, ANSI-кодировка, тест `:773`), экранирование `core/ids.rs:92 argfile_line`; фолбэк при нехватке длины — `launch.rs:1510` («classpath не влезает в командную строку Windows»).
- TuffBox: grep `argfile|32767|Command line was too long` — **0 совпадений**. `launcher.rs:469` собирает `Command::arg` → на Windows Rust склеивает в одну строку с лимитом 32767 символов. На Forge-сборках с сотнями модов classpath переваливает → spawn падает без внятной ошибки.

### 3.4 JVM-тюнинг / автоподбор памяти

- Millida `game/tuning.rs` (**502 стр.**): `GC_FLAGS` (общий источник для авто-режима и «Буста FPS»), `fit_jvm_args` (`:295`), `safe_ceiling_mb` (`:115`), дедупликация GC-опций, порядок «раньше `UnlockExperimentalVMOptions`»; тесты `:457–497`.
- `game/fpsboost.rs` (714) — GC-профиль `GC_FLAGS + AlwaysPreTouch` (`:10`).
- TuffBox: есть `resource_mode.rs`/`cpu_affinity.rs`, GC-профиля для игры нет.

### 3.5 Оживить изоляцию инстансов

- Проблема в TuffBox: `crates/tuffbox-core/src/launcher.rs:258` `prepare_instance` — `#[allow(dead_code)]`, запуск идёт из каталога проекта (работает только `prepare_server_instance` `:173`).
- Модель Millida для переноса: `engine/profiles/store.rs` (618 стр., `profiles.json`) + per-profile `millida-settings.json` (JVM-аргументы, размер окна, путь к Java); **настройки читаются до установки** — своя Java отменяет скачивание JRE, а не отменяет его задним числом (`ARCHITECTURE.md` + `launch.rs` `install_and_launch_in`).

### 3.6 Импорт остальных лаунчеров

- Millida `engine/profiles/imports.rs` — **1179 стр.**: Prism, MultiMC, ATLauncher, GDLauncher, CurseForge, Modrinth App, общие `.minecraft` (включая TLauncher). Плюс `packfile.rs` 235 (`.mrpack`, архивы CurseForge), `export.rs` 126 (`.mrpack`), `migrate.rs` 317.
- TuffBox: `importer.rs:1225` (Prism/MultiMC), `:279` (CurseForge), `importer/packwiz.rs`. Не хватает ATLauncher/GDLauncher/Modrinth App/`.minecraft`.

### 3.7 Паттерн «записал — откатил ровно своё»

- `game/fpsboost.rs`: всё изменённое пишется в `millida-settings.json` (`fpsBoostMods`, `fpsBoostVideo`) → выключение снимает **ровно** свои моды и возвращает **ровно** прежние значения `options.txt`.
- Релевантно TuffBox напрямую: ложится в систему снапшотов/change-планов (`snapshot.rs`, `change_plan.rs`).

### 3.8 Дешёвые победы

| Что | Где в Millida | Зачем TuffBox |
|---|---|---|
| Пиннинг-тесты безопасности | `src-tauri/src/lib.rs:402–688`: тест фиксирует `capabilities/default.json` (`:503`), запрет `core:default` (`:541`), «команда не трогает сейф» (`:554, 585`), `withGlobalTauri==false` (`:610`), CSP `script-src==['self']` (`:621`), пустой asset-scope (`:654`) | Прямой кандидат в CI-гайды (у уже есть border-radius/font-size/glass-compositing/contrast) |
| Валидация jar после fractureiser + блоклист | `engine/content/safety.rs` (**514 стр.**, блоклист `:189` — адрес свой, можно отдавать статикой с GitHub Releases) | Для IDE, открывающей сторонние проекты |
| Реестр идущих установок | `engine/core/jobs.rs` (**228 стр.**): ключ → отмена, прогресс, антидубль | Проверить, есть ли эквивалент |
| Зеркала заблокированных CDN | `engine/core/mirror.rs` (226): схема `/launcher/mr/*`, `/launcher/dl?url=` через свой API | Только схема, адреса свои; важно для РФ |
| Единый HTTP-клиент | `engine/core/http.rs` (1157): один `reqwest::Client` с системными корнями TLS + webpki-fallback, ретраи, resume, `download_checked` (sha1/256/512 + размер), JSON-кэши | TuffBox имеет `download_engine.rs` + `download_cache.rs` — сравнить глубину (ретраи, resume, DEEP_VERIFY) |
| Один вызов = ровно одно действие | `game/crashfix.rs` (888): `CrashAction`/`apply_crash_fix` | Аналогично вашему `create_fix_plan`, но с гарантией атомарности |
| Рефакторинг-паттерн гигантского файла | Millida разнесла логику по `commands/*` + `engine/*`; у TuffBox `src-tauri/src/lib.rs` = **22 345 строк** (~490 команд) | Пример целевой структуры при выносе launch/модов/крашей из `lib.rs` |

---

## 4. Брать НЕ стоит

### 4.1 В TuffBox уже сильнее

- **Краш-анализ/диагностика:** Millida `game/crashcause.rs` 1151 + `crashfix.rs` 888 против TuffBox `crash.rs` 5324 + AI-планы (`action_plan.rs`, `change_plan.rs`, `diagnostics.rs`).
- **Граф/резолвер:** Millida `content/deps.rs` 1203 (BFS, `MAX_NODES=80`, `dep_plan`/`install_required`/`audit_deps`) проще TuffBox `resolver.rs` + `graph.rs` + `graph_service.rs` + `resolve.rs`. Единственное к сравнению — `content/localmeta.rs` (1103): чтение `fabric.mod.json`/`mods.toml`/`mods.toml`, связи `mod_id`/`provides`/`requires`/`breaks` **из jar, включая файлы не из каталогов** — сверить с `mod_scan.rs` (507).
- **Темы/UI-гайды:** у TuffBox 18 тем + WCAG-контраст-тест; у Millida `lib/accent.ts` + `density.ts` — взять можно только генерацию CSS-переменных из hex.

### 4.2 Привязано к бэкенду Millida (`api.millida.net` и др.) — переносить некуда

| Кусок | Путь Millida |
|---|---|
| Прокси API ядра | `engine/accounts/millida.rs:4` (`MILLIDA_API`) |
| Прокси CurseForge (ключ на сервере) | `engine/content/curseforge.rs:9` (`/v2/launcher/cf`) |
| Зеркала Modrinth/чужих maven | `engine/core/mirror.rs:65, 72, 149–182` |
| Каталоги модов/паков | `content/{millida, millida_plan, catalog_file, catalog_pack, millida_mod}.rs` (`garage.millida.net`, `cdn.millida.net`) |
| Блоклист | `content/safety.rs:189` (адрес — свой) |
| Шаринг сборок по коду | `profiles/share.rs:36, 193` (`millida.net/p/<code>`) |
| Облачный синк | `engine/cloud.rs` |
| Выгрузка логов/скриншотов | `profiles/{screenshots, logs}.rs`, `sharelog.rs` |
| Хостинг серверов | `commands/hosting.rs` |
| Головы-аватарки, Yggdrasil, CSL | `skins/heads.rs:8`, `skins/diag.rs:554`, `accounts/session.rs:246` |
| Друзья, звонки WebRTC, Discord-ссылки | `discord.rs:28–325`, фронт `src/lib/call/*` (~ крупный модуль) |
| Самообновление (адреса) | `selfupdate.rs:17, 537–544`, `tauri.conf.json:226, 228` |
| Инсталлер (брендинг) | `installer/src/main.rs:16` |
| Каталог серверов | отдельный модуль + бэкенд |

### 4.3 React-фронтенд

Стек другой — переносимы только паттерны: ленивые экраны без роутера (`src/screens/registry.ts`, `preloadScreen`, `requestIdleCallback`), форма IPC-слоя (`src/ipc/tauri.ts` — импорт из `@tauri-apps/*`, не `window.__TAURI__`; `commands.ts` 1447; `events.ts` — подписки возвращают `null` вне Tauri), two-phase модалки (`src/state/ui.ts:147–167`).

---

## 5. Лицензия — решение ДО копирования кода

- Millida: **GPL-3.0-only** (+ `TRADEMARK.md`: бренд, ключи и сервисы не передаются). README честно говорит: код выложен «ради проверяемости, а не как заготовка для чужих лаунчеров» — GPL это не запрещает, но решение за нами.
- TuffBox: **MIT OR Apache-2.0** (`LICENSE.md`).

Копирование GPL-строк в TuffBox делает **собираемый бинарник GPL-3.0-only** — совместить с текущей лицензией репо нельзя. Стратегии:

- **(а) Чистый рерайт** идей/алгоритмов без копирования строк — легально свободно, основной вариант для этого отчёта.
- **(б) Явный перенос** с переключением репо/бинаря на GPL-3.0-only.
- **(в) Отдельный GPL-крейт** — не спасает бинарь (линкуется).

Рекомендация: **(а)** — весь ТОП из раздела 3 это небольшие, понятные по описанию алгоритмы (митигация, чёрный список, argfile, манифест обновления), рерайт дешевле, чем лицензионный разворот.

---

## 6. Правила интеграции (из `AGENTS.md` TuffBox)

1. Любая переносимая Tauri-команда с I/O — `async fn` + `tokio::task::spawn_blocking` (sync-команды блокируют главный поток и зависшие IPC). Любой запуск внешнего процесса — с дедлайном (`run_with_probe_timeout`, `crates/tuffbox-core/src/jre.rs:393`).
2. После переноса кода — `graphify update .` (иначе граф навигации врёт новым файлам).
3. UI — только Svelte 5 (`$props`/`$state`/`onclick`/snippets), токены `border-radius`, контраст тем (`npm test`), мин. кегль 11px, не менять compositing glass-поверхностей.
4. Новый код — тесты рядом (`#[cfg(test)]`), `cargo clippy --all-targets -- -D warnings`.

---

## 7. Сводная таблица приоритетов

| # | Что берём | Millida-путь | Объём | Трудоёмкость | Ценность |
|---|---|---|---|---|---|
| 1 | Самообновление (minisign) | `engine/core/selfupdate.rs` | 673 | средняя | ★★★ (Stage 15) |
| 2 | Инсталлер | `installer/` | 1729 | средняя | ★★★ |
| 3 | Log4Shell-митигация | `game/launch.rs` + `install.rs` | ~100 + тесты | низкая | ★★★ (безопасность) |
| 4 | Чёрный список JVM-флагов | `game/launch.rs:814–998` | ~200 + тесты | низкая | ★★★ |
| 5 | `safepath` для путей | `engine/core/safepath.rs` | 250 | низкая | ★★★ |
| 6 | `@argfile Windows` | `game/pack_launch.rs:234–268`, `core/ids.rs:92` | ~150 | низкая | ★★★ |
| 7 | GC/память-тюнинг | `game/tuning.rs` | 502 | низкая–средняя | ★★ |
| 8 | Изоляция инстансов (модель профилей) | `engine/profiles/store.rs` + настройки до установки | 618+ | средняя | ★★ |
| 9 | Импорт ATLauncher/GDLauncher/Modrinth App/`.minecraft` | `engine/profiles/imports.rs` | 1179 | средняя | ★★ |
| 10 | Паттерн точного отката изменений | `game/fpsboost.rs` (settings-файл) | идея | низкая | ★★ |
| 11 | Пиннинг-тесты capabilities/CSP | `src-tauri/src/lib.rs:402–688` | ~280 | низкая | ★★ |
| 12 | Проверка jar (fractureiser) | `engine/content/safety.rs` | 514 | низкая | ★★ |
| 13 | Реестр задач установок | `engine/core/jobs.rs` | 228 | низкая | ★ (проверить наличие) |
| 14 | Схема зеркал CDN | `engine/core/mirror.rs` | 226 | низкая | ★ (адреса свои) |
| 15 | Сравнить: `localmeta.rs` vs `mod_scan.rs` | `engine/content/localmeta.rs` | 1103 | — | справочно |

---

*Отчёт собран прямым чтением кода обоих репозиториев (2 агента-исследователя + точечная верификация grep'ом в TuffBox по строкам: `log4j`, `argfile`, `safe_join`/zip-slip`, `minisign`). Обновлять при изменении Millida или закрытии гэпов TuffBox.*
