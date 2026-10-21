# План 2: перенос из UClient / Undefined Client (по отчёту docs/19-studioberry-client-reference.md)

Дата: 2026-10-02. Источник отчёта: `docs/19-studioberry-client-reference.md`.
Соседний план (референс 1, Millida): [`2026-10-02-millida-port.md`](2026-10-02-millida-port.md) — трек A там **реализован**.
Целевой репо: `U:/TuffBox/tuffbox` (MIT OR Apache-2.0).
Статус: **треки E, F, G реализованы**; **H — уже был реализован** (кода не потребовалось); **I реализован отдельным планом** ([`2026-10-02-ai-tool-loop.md`](2026-10-02-ai-tool-loop.md)); J — политика зафиксирована ниже. Осталась только сверка треков B/D плана 1.

---

## Статус трека H (шаринг инстанса) — уже реализовано, кода не потребовалось

Отчёт утверждал «фичи нет». Проверка по коду: **обе половины есть и подключены к UI.**

| Что | Где |
|---|---|
| Публикация пака в GitHub-репозиторий | `github_pack_publish` (`src-tauri/src/github_pack_commands.rs:682`), UI — `GithubPackTransport.svelte` |
| Ссылка для шаринга, включая `tuffbox://install?repo=…` | `GithubPackTransport.svelte:284–294` собирает `shareMessage` (https-ссылка + deep-link одной строкой), кнопки «Copy share message» и «Open share link» |
| Импорт по такой ссылке | глагол `install` в `deep_link.rs` → Library → диалог подтверждения |
| Экспорт пака файлом (не по ссылке) | `ExportBuilder.svelte` + `ExportFormatSelector.svelte`, команды `export_modrinth_pack` / `export_curseforge_pack` / `export_prism_instance` / `export_server_pack` / `export_packwiz_pack` |
| Точка входа | Release Room → `PublishPlatformsConfig.svelte` → `GithubPackTransport.svelte` |

**Остаточное ограничение (не работа, а факт):** шаринг по ссылке требует GitHub-репозитория с правом записи и авторизации (`github_auth.rs`). UClient хостил пакеты у себя — это продуктовое решение о своём хостинге, а не недоделка; в плане 1 это уже отнесено к треку B/TuffSwarm.

---

## Статус трека I (AI-инструменты) — реализован как отдельная фича

Отчёт даёт ~100 инструментов UClient (`ai-tools.ts` + `ai-tools-extended.ts`) как «готовое ТЗ». Проверка по коду TuffBox показала: **цикла вызова инструментов (OpenAI `tool_calls`) в AI-слое не было** — ни в `ai_explanation.rs`, ни в `action_plan.rs`, ни в `quest_chat.rs`. AI-слой был построен как «промпт → структурированный `ActionPlan`».

По решению владельца механизм введён целиком — отдельным планом **[`2026-10-02-ai-tool-loop.md`](2026-10-02-ai-tool-loop.md)** (цикл, транспорт, диспетчер, UI), чтобы это не растворялось в переносе. Итог: 23 read-only инструмента, запись по-прежнему только через `ActionPlan` с подтверждением. Почему не 100/40 «по числу имён» — там же, §5.

---

## Прочие пункты отчёта 19 — сверены, покрыты

| Пункт отчёта | Факт в TuffBox |
|---|---|
| item 6 «ZIP64 + traversal + CF-каталог-скрейп + forgecdn fallback» | traversal закрыт в треке A плана 1 (`safepath`); ZIP64 даёт крейт `zip = 2`; CurseForge покрыт **глубже** скрейпа: официальный API с ключом, реконструкция CDN-ссылок `mediafilez`/`edge`/`media` при withheld `downloadUrl`, Modrinth-SHA1 fallback, правило «ключ только для `edge`/`api`» — `provider/curseforge.rs:341,637,664,467`. |
| item 12 «фрагменты launcher.ts: Java-детект, enrichment» | Java-детект TuffBox читает **реестр Windows** (`SOFTWARE\JavaSoft\…`) + PATH + управляемый JDK, со слоями Fast/Full (`jre.rs:249–270`); покрытие то же, что у UClient. |
| items 8, 9, 10 (батчи, stall-лимиты, классификация логов) | уже были — см. §0. |
| item 11 LAN-relay через VPS | требует свой endpoint — продуктовое решение, не перенос. |
| item 13 дельта `skinviewengine` | источник MIT (`vendor/mine3d`) зафиксирован в треке J; брать по потребности. |

Итог: **всё, что в плане 2 было кодом, сделано; остальные пункты либо уже существовали, либо упираются в продуктовое решение, а не в реализацию.**

---

## Статус трека J (лицензионный чек-пойнт) — зафиксировано

Политика переноса для обоих отчётов:

- Всё перенесённое — **независимый рерайт** по идеям/алгоритмам; ни строки из GPL-деревьев Millida (`milida-new/launcher-main`) и UClient (`studioberry-client/client-main`).
- **Запрещено к переносу** до выяснения лицензий: `emotes/*` (SPEmotes/Emotecraft) и `skin-outer-voxel.ts` (порт 3D Skin Layers) из UClient.
- Дельта движка скина берётся из `milida-new/launcher-main/src/vendor/mine3d/` (MIT, «Copyright (c) 2026 Undefined Studio»), а не из GPL-дерева UClient.
- Реализованные треки A (Millida) и E/F/G (UClient) написаны с нуля: `safepath.rs`, `launch_args.rs`, `world_preview.rs`, `mesher.ts`, `chunkStore.ts`, `blockColors.ts`, `aiUndo.ts` — новых зависимостей, кроме MIT `three`, не добавлено.

---

## Статус трека G (действия deep-link) — реализовано

Один канал, два глагола, строгая валидация — вместо отдельной ветки на каждое действие:

| Что | Файлы |
|---|---|
| `PendingDeepLink { Install \| Launch \| Unknown }` с под-вердиктами; `parse_deep_link` (чужой scheme → `None`, незнакомый глагол → `Unknown`, который показывается тостом, а не игнорируется). `launch` принимает только **абсолютный** путь, который резолвится в манифест `.tuffbox.json` и **парсится** как проект; вердикт несёт путь и имя пака. | `apps/tuffbox-desktop/src-tauri/src/deep_link.rs` (+8 тестов) |
| Чистый переход: `handle_install_url` → `handle_deep_link_url`, команда `take_pending_install_repo` → `take_pending_deep_link`, событие `tuffbox:install-link` → `tuffbox:deep-link`. Обновлены оба вызова в `lib.rs` и фронтенд. | `src-tauri/src/lib.rs`, `src/lib/api.ts` |
| Фронтенд: диспетчер по `kind`. `install` — прежний путь (Library + диалог подтверждения, `onInstallLink` не тронут); `launch` — окно события `tuffbox:launch-link`; `unknown`/битый launch — тост. | `src/lib/api.ts` |
| Launch **никогда не запускается сам**: тост с именем пака и кнопкой `Open & launch` (внешняя ссылка не должна ничего стартовать без действия пользователя). Открытие проекта вынесено в общий `openProjectFromPath`, который теперь использует и `--launch` из ярлыка. | `src/App.svelte` |

**Не реализовано (осознанно, а не «потом»):**
- `import-instance` (произвольный архив по URL) — нет цели: `install_modpack` принимает локальный путь или GitHub-источник, а загрузки произвольного архива по ссылке в продукте нет; делать её ради глагола — это новая фича, а не перенос.
- `join-group` — в TuffBox нет такого понятия: у UClient это группы их мессенджера (чужой бэкенд), в TuffSwarm «групп» нет.

Проверки: `cargo test -p tuffbox-desktop --lib deep_link` — 8/8; `svelte-check` — 0 ошибок / 0 предупреждений; `lint:perf` — пять гайдов зелёные; `vitest` — 22 файла / 187 тестов; `npm run build` — ок.

---

## Статус трека F (инлайн-undo для действий ИИ) — реализовано

| Что | Файлы |
|---|---|
| Журнал undo: `pushAiUndo` / `peekAiUndo` / `dropAiUndo` / `clearAiUndo` / `aiUndoFor`, глубина 20, привязка к проекту (снапшоты проектные — два открытых пака не должны откатывать друг друга). `peekAiUndo` вынесен чистой функцией, поэтому журнал проверяется без стора. | `src/lib/aiUndo.ts` (+7 тестов) |
| После успешного `applyActionPlan` — запись `{snapshotId, title, detail, actor, projectPath}` в журнал. | `src/components/tune/TuneAiSidebar.svelte` |
| Полоса над композером: `<actor> applied <N patches>` + **Undo** (вызывает `api.snapshots.rollback`, снимает запись из журнала, пишет системное сообщение в чат и дёргает `onapplied` для перезагрузки проекта) + **Keep** (скрыть, не откатывая). | там же |

**Отклонение от исходной формулировки** («чип в сообщении»): сделана постоянная полоса над композером вместо чипа в конкретном сообщении. Модель сообщений персистится в чатах — расширять её ради чипа дороже, чем держать один видимый элемент; полоса не теряется при прокрутке и переключении чата. Тот же журнал позже переиспользует Quest AI.

Проверки: `vitest` — 22 файла / 187 тестов passed; `lint:perf` — пять гайдов зелёные; `npm run build` — ок, стартовый бандл не изменился.

**Зависимости (побочный эффект работы, решено с владельцем):** `npm install three` понизил `taurpc` 2.0.0 → 1.8.1 в `package.json`; при возврате 2.0.0 всплыло, что `src/bindings.ts` (Specta, «DO NOT EDIT») сгенерирован под сигнатуру taurpc 1.x — с 2.0.0 `npm run check` падает (`CreateTauRPCProxyOptions.argsMap`). По решению владельца объявление приведено к факту: `"taurpc": "^1.8.1"`. Лоck и `node_modules` теперь согласованы, `npm run check` — 0 ошибок. Миграция на taurpc 2 с перегенерацией bindings — отдельная задача.

---

## Статус трека E (3D-просмотр мира) — реализовано

| # | Что сделано | Файлы |
|---|---|---|
| E1 | Гейтинг версий: `resolvePreviewStrategy` → `live\|export\|unsupported`, границы `MIN_READ_DATA_VERSION=1519`, `READ_MAX=4903`, `MESHER_MAX=4435`, бейдж и подпись версии (имя берётся из самого мира, таблица «DataVersion → релиз» **не** выдумывается). | `apps/tuffbox-desktop/src/lib/worldPreviewMatrix.ts` (+13 тестов) |
| E2 | Чтение чанков: обе раскладки секций (1.13–1.17 `Level.Sections` с `Palette`/`BlockStates`; 1.18+ корневой `sections` с `block_states`/`biomes`), **распаковка палитровых индексов** (непрерывный битовый поток до 1.16 / выравнивание по long после; схема определяется по длине массива — там, где схемы неразличимы, они дают одинаковый результат), base64 `u16` на выходе. | `crates/tuffbox-core/src/world_preview.rs` (+10 тестов) |
| E3 | IPC: `read_world_column(path, worldName, dimension, chunkX, chunkZ)` — `async` + `spawn_blocking`, `None` = «чанка нет». | `apps/tuffbox-desktop/src-tauri/src/lib.rs`, `src/lib/api.ts` (`worlds.readColumn`) |
| E4 | Блок-текстуры из client.jar — **нового кода не потребовалось**: существующий `get_item_icons_batch` уже извлекает `assets/minecraft/textures/block/*.png` из client.jar модов и ванильного jar с дисковым кешем. | `apps/tuffbox-desktop/src-tauri/src/lib.rs:9959` |
| E5 | Мешер (грани видимых блоков, отсечение по соседям, неизвестный сосед = непрозрачный), кеш колонок с окном готовности меша и LRU, палитра блоков (средний цвет текстуры + детерминированный fallback по хешу имени), вьюер на `three` с орбитой/зумом/полётом WASD-QE, ленивая загрузка из `World.svelte`. | `src/lib/world/{mesher,chunkStore,blockColors}.ts` (+32 теста), `src/components/WorldPreview3D.svelte`, `World.svelte` |

**Отклонения от предложенного в docs/19 (осознанные):**
1. **Свой мешер вместо `minecraft-renderer`.** Отчёт сам отмечает хрупкость этой цепочки (несуществующий `main` в package.json, изолированная копия three, патчинг minified `mesherWasm.js`). Минимальный собственный мешер — ~200 строк, без WASM и без патчей node_modules; невидимые грани отсекаются по соседям.
2. **Индексы распаковываются в Rust, а не в JS.** `i64` с упакованными индексами превышает 2^53 и при `JSON.parse` молча портится; кроме того схема упаковки зависит от версии. Rust знает `DataVersion` и отдаёт готовые `u16` (base64).
3. **Нет серверного LRU-кэша сессий.** Кеш колонок живёт на фронте (рендереру он всё равно нужен), бэкенд делает одно чтение файла + inflate; слой `region.rs` уже кеширует метаданные регионов.
4. **Свет (sky/block light) не передаётся** — вьюер использует затенение по нормалям; поля, которые никто не читает, не тащим.

Проверки: `cargo test -p tuffbox-core --lib` — 630 passed / 0 failed (два предсуществующих падения `crash_kb`/`fs_util` отфильтрованы); `cargo check -p tuffbox-desktop --all-targets` — без ошибок; `vitest` — 21 файл / 180 тестов passed (из них 38 новых: мешер, кеш, палитра, гейтинг версий); `svelte-check` — 0 ошибок / 0 предупреждений; `npm run build` — ок; `npm run lint:perf` — все пять гайдов зелёные, стартовый бандл **не изменился** (174.3 KB JS / 25.4 KB CSS gzip): `three` уехал в отдельный ленивый чанк (113 KB gzip), который грузится только при открытии 3D-режима.

Новая зависимость: `three@0.156.1` + `@types/three@0.156.0` (та же версия, что уже приходила транзитивно через `skinview3d` — второй копии в дереве не появилось).

**Не проверено визуально:** рендер в живом приложении (нет открытого проекта с миром в этой среде). Проверены все слои до `three`: данные, распаковка, мешер (включая геометрию и намотку граней), кеш, палитра; сам вьюер — тонкая обёртка над ними.

---

## 0. Сверка отчёта 19 с кодом TuffBox (половина «топов» уже есть)

Проверено по коду в этом репо, а не по описанию:

| Заявлено в docs/19 | Факт в TuffBox | Доказательство |
|---|---|---|
| «Паттерна подтверждение + undo-стек + build-diff нет вообще» (§3.2, ★★★ #1) | **Неверно.** Есть полный цикл: `apply_action_plan` — только после подтверждения пользователя, создаёт снапшот и возвращает `snapshotId`; откат — `rollback_snapshot`; восстановление одного файла из снапшота — `restore_tracked_file`; диффы — `diff_snapshots` / `diff_snapshot_vs_current` / `snapshot_file_diff_text`; история изменений проекта с диффами, актором и `plan_source`. UI: `TuneAiSidebar.svelte` — чекбокс на каждое действие, `beforeExcerpt`/`afterExcerpt`, risk, validation, `needsUserReview`-подтверждение; `QuestAiSidebar` + `QuestPlanReview`. | `apps/tuffbox-desktop/src-tauri/src/lib.rs:8482` (`apply_action_plan`), `:14266` (`rollback_snapshot`), `:14173` (`restore_tracked_file`), `:14287`, `:14256`; `src/components/tune/TuneAiSidebar.svelte:272–340` |
| item 9 «Stall-timeout/лимиты скачивания» (★★) | **Уже есть.** `DEFAULT_STALL = 120s`, `.tuffbox.part`-резюм через Range, sha1/md5/sha256 перед rename, bounded concurrency (по умолчанию 8, настраивается 1–64), circuit breaker по хостам. | `crates/tuffbox-core/src/download_engine.rs:19, 135–160, 294, 347` |
| item 8 «Батчи `/v2/projects?ids=` по 80» (★★) | **Уже есть**, с запасом: `CHUNK = 256` для `version_files/update`, `CHUNK = 100` для `projects?ids=`. | `crates/tuffbox-core/src/provider/modrinth.rs:131, 175` |
| item 10 «Классификация строк лога» (★★) | **Уже есть и глубже**: `classify_signal_line` с ~30 видами `CrashSignalKind`, разбор Fabric-предложений «Replace mod…», защита от ложных срабатываний (`missing model` ≠ missing mod). | `crates/tuffbox-core/src/crash.rs:2531+`, тесты `:4138–4600` |
| 3D-просмотр мира (★★★) | **Подтверждено: нет.** `three` в зависимостях отсутствует, есть только `skinview3d`; карта мира — 2D (`WorldMap.svelte`, canvas 2D). | `apps/tuffbox-desktop/package.json` |
| Шаринг инстансов ссылкой (★★ «фичи нет») | **Частично есть**: GitHub Pack Transport (публикация/импорт пака по ссылке на репо, `tuffbox://install?repo=`) уже в ядре. Отсутствует «отдать произвольный инстанс по ссылке из UI». | `crates/tuffbox-core/src/github_pack/` (`publish.rs`, `apply.rs`), `docs/15-github-pack-transport.md`, `src-tauri/src/deep_link.rs` |
| Валидация deep-link (★★) | **Есть, но одно действие**: `tuffbox://install?repo=…` с вердиктами и тестами. Доп. действий (`import-instance`, `launch`, `join-group`) нет. | `apps/tuffbox-desktop/src-tauri/src/deep_link.rs` |
| ZIP64 + path-traversal в импортёре (★★) | zip-slip закрыт (сведён на `safepath` в треке A плана 1); ZIP64 — **проверить**, но крейт `zip = "2"` поддерживает его из коробки. | `crates/tuffbox-core/src/importer.rs`, `crates/tuffbox-core/src/safepath.rs` |
| Запуск MC, мессенджер, Electron, Python-инсталлер, темы, локали | Не берём (у TuffBox своё, лицензия GPL) | — |

Вывод: единственный крупный реально отсутствующий блок — **3D-просмотр мира**. Всё остальное из «топов» отчёта 19 либо уже реализовано в TuffBox (иногда глубже), либо является узкой UX-дельтой.

---

## 1. Лицензия

UClient — GPL-3.0-only + доп. условия; TuffBox — MIT OR Apache-2.0. Как и в плане 1: **только независимый рерайт** по идеям/алгоритмам, ни строки GPL.

- `emotes/*` (SPEmotes/Emotecraft) и `skin-outer-voxel.ts` (порт 3D Skin Layers) — **под запрет** до выяснения лицензий.
- Легальный источник дельты движка скина: `U:\TuffBox\milida-new\launcher-main\src\vendor\mine3d\` — MIT («Copyright (c) 2026 Undefined Studio»), а не GPL-дерево UClient.
- `minecraft-renderer@0.1.96` (zardoy) — MIT; `prismarine-*`/`mc-assets` — публичные. GPL покрывает только обвязку UClient (`world.ts`, `worldlib/*`, `anvil.ts`, `worldViewer.ts`) — её рерайтим.

---

## 2. Треки

### Трек E — 3D-просмотр мира (решение требуется)

Единственная новая крупная фича. Разбивка:

| # | Что | Куда | LOC (impl+тесты) |
|---|---|---|---|
| E1 | Матрица версий: `MIN_READ_DATA_VERSION=1519 (1.13)`, `READ_MAX=4903 (26.2)`, `MESHER_MAX=4435 (1.21.6)`, `resolvePreviewStrategy` → `live\|export\|unsupported`, бейдж. Чистая логика, без DOM. | `apps/tuffbox-desktop/src/lib/worldPreviewMatrix.ts` + vitest | ~150 + 120 |
| E2 | Парсер Anvil на Rust: WireColumn/WireSection, **два layout'а битмассива (spanning до 1.17 / no-span 1.18+)**, палитра, свет, `.mcc`/LZ4. | `crates/tuffbox-core/src/anvil.rs` (new) | ~900–1200 |
| E3 | Слой сессий: `describeWorld`, LRU (8 регионов / 4096 колонок), батч-ингест, IPC-команды. | `crates/tuffbox-core/src/world_viewer.rs` (new) + `src-tauri` | ~400 |
| E4 | Блок-текстуры из `client.jar` (+ кэш). | `crates/tuffbox-core/src/` | ~150 |
| E5 | Рендер: `minecraft-renderer` (MIT) отдельным entry (изолированный three 0.184 / alias в Vite), fly-controls (`rendererYaw = π − mcYaw`), маркер игрока, таблицы блоков/биомов реестра 1.21.6. | `apps/tuffbox-desktop/src/lib/world/*` | ~800–1200 |

**Итого ~2 400–3 100 LOC, 1–2 недели.** Ограничения переносить осознанно: миры 1.12 и старше не открываются; миры новее 1.21.6 — стратегия `export`/`unsupported`.

Альтернатива: **не строить**. Тогда E1 тоже не нужен в отрыве (бейджи без вьюера бесполезны), и трек закрывается полностью.

Возможный компромисс: только **E2** (Rust-парсер Anvil) — он полезен сам по себе: точные цвета блоков для существующей 2D-карты, поиск спавна/структур, экспорт. E3–E5 откладываются.

### Трек F — инлайн-undo в AI-чате (узкая дельта, ★)

Реально отсутствует только «отменить последнее действие ИИ» из ленты чата: `pushAiUndo`/чип, 20 шагов.
Основа не нужна — есть `apply_action_plan` → `snapshotId` и `rollback_snapshot`.
Работа: держать в сайдбаре стек `{snapshotId, title, detail}` (20), кнопка «Отменить» → `rollback_snapshot`, чип в сообщении. ~150–200 LOC Svelte, 0 Rust.

### Трек G — deep-link: остальные действия (~200 LOC Rust + тесты)

Сейчас только `install?repo=`. Добавить разбор и валидацию `import-instance` (локальный путь — только по явному подтверждению в UI), `launch` (открыть проект и запустить), `join-group` (по образцу UClient `deepLink.ts`, но по своим правилам). Строгая валидация payload обязательна — ссылка приходит извне.

### Трек H — «поделиться инстансом» из UI (~400 LOC)

Не бэкенд, а точка входа: экспорт текущего проекта в пак (уже есть `exporter.rs`) → публикация через `github_pack::publish` → ссылка `tuffbox://install?repo=…`. Новый код — в основном UI + один склеивающий команду вызов.

### Трек I — расширение AI-инструментов по схемам UClient

`ai-tools.ts` (47+) + `ai-tools-extended.ts` (~50) — готовое ТЗ имён/схем. Сначала **сверка со своим AI-слоем** (сколько из ~100 уже есть), потом маппинг недостающих на существующие команды ядра. Оценка объёма — после сверки, сейчас не оцениваю.

### Трек J — лицензионный чек-пойнт

Зафиксировать в репо: всё переносимое — независимый рерайт; `emotes/*` и `skin-outer-voxel.ts` — запрещены; дельта скина — из `vendor/mine3d` (MIT).

---

## 3. Что не берём

Запуск MC (`eml-lib`), messenger (`messenger-api.ts` + `ui.ts` 6 223 — чужой бэкенд), Electron-специфика (`main.ts`, `app://`-схема, ANGLE-свитчи, WebContentsView), `installer/*.py` (заменяется Tauri-bundler/updater — см. трек B плана 1), `app.ts` (14 666 строк DOM-монолит), свои темы/локали, «парковка» DOM-карточек (`parkAiConfirmsFromRoot` — в Svelte это обычное реактивное состояние).

---

## 4. Порядок и решение

1. **Решение по треку E** (строить 3D-просмотр / только парсер E2 / отказ) — от него зависит 1–2 недели работы.
2. **F** и **G** — дешёвые, можно делать независимо от решения по E.
3. **H** — после F/G (склеивает существующие `exporter.rs` + `github_pack`).
4. **I** — после сверки со своим AI-слоем.
5. **J** — процедурно, в любой момент.

Первый шаг по умолчанию (если E отложен): трек F — инлайн-undo в AI-сайдбаре.
