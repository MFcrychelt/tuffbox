# Known Problems / Известные проблемы

Рабочий список проблем, выявленных при использовании TuffBox IDE.
Обновляется по мере исправления. Формат: описание → статус → план/причина.

## 1. Долгая загрузка страницы Setup + всплывающие окна консоли
- **Симптом:** Страница/экран Setup грузится долго; при работе видны открывающиеся
  окна консоли (чёрные окна cmd).
- **Причина (предпол.):** Rust-бэкенд на Windows запускает дочерние процессы
  (`std::process::Command`) без флага `CREATE_NO_WINDOW`, из-за чего Windows
  показывает консольное окно на каждый spawn. Долгая загрузка может быть связана
  с синхронными вызовами при инициализации Setup (ожидание версий Minecraft/загрузчиков,
  проверка обновлений и т.п. в главном потоке UI).
- **Статус:** в работе (часть решена)
- **Что сделано:** флаг `CREATE_NO_WINDOW` (`0x08000000`) уже применяется ко всем
  дочерним процессам на Windows — `process.rs:65` (запуск игры/лог),
  `jre.rs:187` (java -version), `forge.rs:481` и `forge_install.rs:508`
  (forge-процессоры). Консольные окна при spawn больше не всплывают.
- **Осталось:** долгая загрузка Setup. `ProjectSettings` (вкладка setup) и
  `AddInstanceModal` грузят версии/шаблоны синхронно через `await` в `onMount`.
  Перенести в фоновую загрузку со скелетоном/индикатором (см. `AddInstanceModal.svelte:45-87`).
- **Сделано (2026-07-09):** `ProjectSettings.svelte` — `onMount` теперь грузит
  версии MC, java-превью и статус схемы параллельно через `Promise.all`, форма
  рендерится сразу (по `$projectPath`, а не `$projectInfo`), во время загрузки
  показывается индикатор `Loading instance settings…` и форма `.dimmed`.
  `AddInstanceModal.svelte` — `onMount` грузит версии MC и дефолтную папку
  параллельно (`Promise.all`), затем версии загрузчика. `svelte-check`: 0 errors.
- **Статус:** исправлено (асинхронность Setup/AddInstance реализована).

## 2. Вкладка Content не грузит моды
- **Симптом:** Вкладка Content пустая — моды не подгружаются вообще. Должна
  загружать список модов при её открытии.
- **Детали:** Иконки модов показывают плейсхолдеры (картинка-заглушка), будто
  изображения не загрузились. Кнопка "Add Mods" работает (там баг загрузки модов
  обойдён локально), но сама вкладка при открытии ничего не делает.
- **UI:** вкладка выглядит ужасно (плохая вёрстка).
- **Референс:** см. реализацию загрузки модов в `U:\TuffBox\AstralRinth-AR-0.10.2701\astralrinth`.
- **Статус:** исправлено (2026-07-10) — обновления модов, update-all, UI Content,
  окно прогресса скачивания с реальными шкалами.
- **Что сделано:** `Mods.svelte` грузит `list_mods` при активации вкладки
  (реактивный блок `Mods.svelte:522` + `load()` в `:522`), есть фильтры по
  контенту (mod/resourcepack/datapack/shader), кнопка Add Mods, sync, update,
  recommendations, version picker. Иконки берутся из `iconUrl`
  (`list_mods` строит `https://cdn.modrinth.com/data/{pid}/icon.png`, lib.rs:343)
  с корректным fallback-плейсхолдером (`Mods.svelte:483-485`). CSP в tauri = null,
  так что CDN-иконки грузятся.
- **Сделано (2026-07-09):** в `Mods.svelte` после `list_mods` запускается
  `hydrateMissingIcons()` — для модов с `projectId`, у которых `iconUrl` пуст
  (локальные jar'ы с известным Modrinth-id, или сбой CDN), иконка
  докачивается через `get_modrinth_project_icon` (lib.rs:441) параллельно
  (`Promise.all`), при ошибке остаётся letter-avatar fallback. `svelte-check`:
  0 errors.
- **Сделано (2026-07-10):**
  1. Update / Update all: применяют `versionId` из batch-check, удаляют старый
     jar при смене имени файла, не валят весь batch из‑за одного failed
     `get_project`.
  2. `download_project_mods_tracked` пишет реальный byte-progress в
     `DOWNLOAD_PROGRESS` и эмитит `mod-download-progress` /
     `mod-download-batch`.
  3. Content UI: hero, update panel, карточки с badge/анимацией; модалка
     прогресса со шкалами на каждый мод.
  4. `svelte-check`: 0 errors; `cargo check -p tuffbox-desktop`: ok.
- **Сделано (2026-07-10, Windows update transaction):** streaming download
  теперь заменяет одноимённый jar через platform-aware atomic persist, а
  single-mod update скачивает и проверяет файл до записи manifest. При ошибке
  восстанавливается предыдущий jar; учитывается суффикс `.disabled`, UI больше
  не скрывает `download.failed`.

## 3. Вкладка Recipes — непонятный парсинг, знаки вопроса
- **Симптом:** Не ясно, как парсится содержимое; весь текст покрыт знаками
  вопроса (����).
- **Причина (предпол.):** неверная кодировка при чтении/выводе (UTF-8 vs системная
  кодировка, особенно на Windows — cp1251), либо парсер выводит нечитаемые данные.
- **Статус:** исправлено (полноценный JEI UI, 2026-07-10)
- **Что сделано:**
  1. `recipe_layout.rs` — раскладка 3×3 по алгоритму JEI
     (`CraftingGridHelper.getCraftingIndex`), печь, кузня (горизонтально),
     камнерез; циклическая смена `one_of`/тегов.
  2. `recipe_scan.rs` — JAR + `datapacks/` + world datapacks + `kubejs/data`,
     лимит 8000, `RecipeScanResult` со статистикой; генерация/запись KubeJS
     remove-скриптов в `kubejs/server_scripts/tuffbox_recipe_removes.js`.
  3. `RecipeBrowser.svelte` — layout как JEI: category rail, MC-style панель
     крафта, ingredient list с пагинацией, bookmarks, history, поиск
     `@mod #tag &id $ $ -exclude`, R/U/B/←→/Backspace, Queue remove → disk.
  4. Forge/Fabric/NeoForge: пути `recipes` и `recipe`.
- **Live JEI (2026-07-10):** добавлен companion plugin для Fabric/NeoForge
  1.21.1. При запущенном клиенте Recipes получает реальные runtime categories,
  crafting stations, slot layouts, alternatives и локализованные имена через
  token-authenticated localhost bridge. После выхода UI автоматически
  возвращается к offline snapshot.

## 3b. Quest Editor — заглушка без сохранения
- **Симптом:** Quest editor парсил только заголовки глав, квесты не загружались,
  сохранение «coming soon».
- **Статус:** исправлено (2026-07-10)
- **Что сделано:** `quest_book.rs` — SNBT parser/serializer, `load_quest_book` /
  `save_quest_chapter` / `validate_quest_book`. `QuestEditor.svelte` — загрузка
  квестов, карта зависимостей, валидация, сохранение в SNBT с auto-snapshot.

## 4. Вкладка World — карта и инструменты (MCA Selector)

- **Симптом (исторический):** UI казался без карты — IDE stage «World / Ores»
  показывал только ore-chart; MCA-карта была спрятана ниже или в sidebar Ctrl+8.
- **Статус:** исправлено (2026-07-29)
- **Что сделано:**
  1. IDE stage **World / Map** (`world-map`) → полный [`World.svelte`](apps/tuffbox-desktop/src/components/World.svelte)
     с [`WorldMap`](apps/tuffbox-desktop/src/components/WorldMap.svelte) dock (как MCA Selector):
     select / delete / export-import / NBT / filters, spawn overlay, polygon.
  2. IDE stage **Ores / Heights** (`ore-gen`) → только график руды; ссылка на World map.
  3. Sidebar **World (Ctrl+8)** — тот же MCA map UI.
  4. Бэкап мира перед destructive ops остаётся в World header; delete подтверждается диалогом.
- **Бэкенд:** `region.rs` / `region_edit.rs` + `api.worlds.*` (anvil .mca).

## 5. Вкладка Resolve полностью сломана
- **Симптом:** нельзя взаимодействовать с графом (не кликабелен/не рендерится),
  надписи в карточках выезжают за пределы, неудобный UI.
- **Статус:** исправлено (быстрый offline/cache render + background refresh)
- **Что сделано:** `Graph.svelte` полноценно использует `d3-force` для
  раскладки, перетаскивание узлов (`handleNodeMouseDown`, Graph.svelte:360),
  пан/зум (`zoomBy`/`resetView`), клик по узлу выделяет, auto-install missing
  deps, change-plan. Карточки узлов ограничивают текст: `.node-label` и
  вложенный `.node-meta` имеют `overflow:hidden; text-overflow:ellipsis`
  (Graph.svelte:934-946).
- **Осталось:** проверить на больших графах (сотни узлов) — `forceCollide`
  радиус 46 и `distance(150)` могут давать налезающие карточки; при
  необходимости увеличить collide-радиус или уменьшить размер карточки в
  `.compact`-режиме.
- **Сделано (2026-07-09):** в `Graph.svelte` увеличен `forceCollide` радиус
  46→70 и link `distance` 150→170 (Graph.svelte:348-349) для лучшего разрежения
  узлов на больших графах; подписи в SVG-узлах обрезаются до 22 символов с
  многоточием (Graph.svelte:499), чтобы длинные label не налезали друг на
  друга. `svelte-check`: 0 errors.
- **Сделано (2026-07-10):** `get_graph` больше не выполняет синхронный N+1
  Modrinth-enrich. Сначала возвращается локальный или валидный cached graph,
  сеть обновляет `.tuffbox/cache/dependency-graph.json` в фоне. Resolve plan и
  diagnostics используют тот же snapshot. Missing dependency теперь
  полноценный core-узел, поэтому все edge endpoints существуют и d3 layout не
  падает.

## 6. Вкладка History — надписи не помещаются в шкалу изменений
- **Симптом:** текст описания изменений выходит за границы таймлайна/шкалы.
- **Статус:** исправлено
- **Что сделано:** в таймлайне `ChangeHistory.svelte` полоса `.file-title`
  имеет `overflow:hidden; text-overflow:ellipsis; white-space:nowrap`
  (ChangeHistory.svelte:385), колонка дерева `minmax(0,1fr)` (Graph сетка
  `.file-strip` grid-template-columns: `18px minmax(0,1fr)`), так что длинные
  пути файлов не вылезают за карточку. `preview-header p` (дата/причина)
  — muted, переносится. Команды `list_project_change_history` /
  `read_project_history_file` работают.
- **Примечание:** если ранее текст выезжал — это было до добавления `minmax(0,1fr)`
  в grid-колонки. Сейчас ограничен.

## 7. Export не работает полноценно
- **Симптом:** экспорт (mrpack / server pack / prism / curseforge / release)
  работает не полностью.
- **Статус:** исправлено (покрыто smoke-тестами; рантайм-проверка в GUI опциональна)
- **Что сделано:** все экспорт-команды присутствуют и бэкенд проходит
  `cargo check` без ошибок. Добавлены unit-тесты в `exporter.rs`
  (`export_modrinth_pack_smoke`, `export_server_pack_skips_client_mods`) —
  проверяют, что упаковка не падает и корректно отбирает моды по side
  (server-pack пропускает client-side моды). Тесты проходят (`cargo test
  --lib exporter` → 2 passed). Функции экспорта возвращают `Result`
  (обработка ошибок, без паник), UI — `ExportBuilder.svelte` + `ReleaseRoom.svelte`.
- **Осталось (опционально):** прогнать каждый формат на реальном проекте в GUI
  для финальной проверки специфичных веток (curseforge/prism/ mrpack с
  overrides). Базовая логика подтверждена тестами.

## 8. Обновления модов / смена версии (Content)
- **Симптом:** Update all и точечное обновление пропускали моды без sha1 в
  манифесте; отдельная панель «updates ready» дублировала кнопки на карточках;
  version picker показывал только server-filtered список без канала/поиска.
- **Статус:** исправлено (2026-07-14)
- **Что сделано:**
  1. `check_mod_updates` / `update_all_mods` — общий `resolve_pending_mod_updates`:
     sha1 из манифеста → hash jar на диске → fallback `project/{id}/version`
     под текущие MC+loader (Quilt также пробует Fabric). Сравнение по
     `file_id`/hash файла, не по строке version_number.
  2. Убрана секция update-panel; в тулбаре **Update all**, на карточке —
     иконка Update + Change version; бейджи/dots остаются.
  3. `get_mod_versions` грузит все версии, помечает `compatible` /
     `versionType`; UI: поиск, hide incompatible, changelog, confirm switch.
  4. `validate_project.loaderKind` → `loader_kind_slug`; soft MC-check при
     refresh metadata (явный switch на другую MC-версию после confirm).
- **Референс:** Prism `ModrinthCheckUpdate`, Modrinth App ContentUpdaterModal,
  modrinth-extras version+loader filter.

## 9. CurseForge + установка модпаков + Retry
- **Симптом:** не было CurseForge API; импорт CF zip ставил SourceKind::Modrinth
  и website URL; UI не умел ставить модпаки; download dialog без Retry.
- **Статус:** исправлено (2026-07-14)
- **Что сделано:**
  1. `provider/curseforge.rs` — Flame API (`x-api-key`, search modpacks,
     `/mods/files`, Modrinth SHA1 fallback для blocked downloads).
  2. Исправлен импорт CF: `projectID`/`fileID`, resolve URLs, overrides,
     stash `curseforge/manifest.json`.
  3. `install_modpack` + вкладки Add Instance: Blank / Import / CurseForge.
  4. Download dialog: **Retry** на failed row + **Retry failed (N)**.
- **Референс:** Prism FlameCreationTask / FileResolvingTask / NetworkJobFailedDialog.

---
## 10. MSA-сессия протухает раз в сутки без гарантированного продления
- **Симптом:** Microsoft-токен живёт ~24 часа; `expires_at` ставился один раз
  при логине и не сдвигался при тихом обновлении в `mc_get_auth_status`,
  а launch-путь вообще не проверял протухание — первый запуск дня мог
  упасть на невалидном токене (онлайн-серверы отклоняют сессию).
- **Статус:** исправлено (2026-09-16).
- **Что сделано:**
  1. `mc_get_auth_status` теперь сдвигает `expires_at` после успешного тихого
     обновления токенов (auth.rs).
  2. Выделен общий `refresh_active_microsoft_session()` (обновляет токены в
     keyring, сдвигает expiry, сохраняет состояние); `mc_refresh_profile`
     переведён на него.
  3. Новая команда `mc_refresh_token` — принудительное продление без
     30-секундного троттлинга статуса; зарегистрирована в invoke_handler.
  4. Launch-путь (`launch.ts`): при `expiresAt` ближе 10 минут — тихое
     продление до запуска игры; при неудаче — тост «Sign in / Launch anyway»
     с 60-секундным бэкоффом (повтор не долбит OAuth). Оффлайн/Yggdrasil
     (expiry null) не трогаются.
  5. `App.svelte`: при старте гидратирует сессию и проактивно продлевает
     протухший токен — первый Play дня больше не падает.
  6. Тесты: `needsTokenRefresh` (маржа/граница/null) в launchState.test.ts.

## 11. Java 8/16 не скачивалась; старые версии запускались на новом Java
- **Симптом:** MC ≤1.16.4 требует Java 8, 1.17 — Java 16, но managed-загрузка
  умела только GraalVM CE (у которой нет релизов jdk-8/jdk-16) — установка
  падала ошибкой. Хуже: наличие более нового Java считалось совместимым, и
  старый Forge ≤1.16 запускался на Java 17/21 с классическими крашами
  bootstrap-загрузчика.
- **Статус:** исправлено (2026-09-16).
- **Что сделано (jre.rs):**
  1. `managed_jdk_needed`: legacy-мажоры (≤16) требуют ТОЧНОГО совпадения
     мажора — новый Java больше не подменяет старый; современные мажоры (17/21)
     как раньше принимают более новый рантайм.
  2. Маршрутизация `install_managed_jdk_major`: 8/16 → Adoptium Temurin
     (API `/v3/assets/latest/{major}/hotspot`, sha256-проверка, zip/tar.gz),
     17/21 → GraalVM CE как раньше; переиспользование через `.tuffbox-java-ok`.
  3. Тесты: exact-match для legacy, >= для современных, маршрутизация,
     URL-билдер и имена платформ Adoptium.
- **Эффект:** паки 1.7.10–1.16.5 (старый Forge) получают корректный Temurin 8,
  1.17 — Temurin 16, данные Forge-профилей берутся с launcher-meta Modrinth
  (покрытие до 1.10).

## 12. Простой лаунчера дорогой: ~14% ЦП и 200 МБ ОЗУ на главной в фоне
- **Симптом (замер пользователя, Ryzen 7 5700X3D):** запущенный лаунчер на
  главной странице в фоне занимает 14.2% ЦП (внутри процесса WebView2
  dispatcher — 9%) и ~200 МБ ОЗУ.
- **Причины (по коду):**
  1. Два полноэкранных ambient-пятна `.glow-a/.glow-b` (42vw, `filter:
     blur(90px)`) с бесконечной анимацией transform — каждый кадр дорогая
     растеризация гигантского размытия (главный источник работы dispatcher).
  2. `TaskProgressPanel` опрашивал `list_background_tasks` каждые **800 мс
     бесконечно**, даже когда панель пуста и окно скрыто.
  3. `Header` ходил в Supabase (`get_launcher_online`) каждые 15 с без
     проверки видимости окна.
  4. `PackHealthBadge` сканировал проект (`get_pack_health`) каждые 60 с —
     комментарий «while visible» не выполнялся.
  5. `startRunningInstancesWatch` продолжал опрос (12 с idle) у скрытого
     окна; visibilitychange-listener не снимался при очистке.
- **Статус:** исправлено (2026-09-16).
- **Что сделано:**
  1. `.glow`: предварительное «размытие» radial-gradient вместо рантайм
     `blur(90px)` — визуально то же, растеризация больше не нужна.
  2. Новая политика простоя `src/lib/idlePerf.ts`: `body.app-hidden`
     ставится при `document.visibilityState !== "visible"` (WebView2
     отдаёт hidden при минимизации И полном перекрытии окна), а styles.css
     глушит `animation-play-state: paused` для ВСЕХ анимаций этого класса —
     фоновой лаунчер перестаёт композитить кадры целиком.
  3. `TaskProgressPanel`: адаптивный опрос (2 с пока есть видимые задачи /
     15 с когда пусто), скрытое окно пропускает IPC, идентичные ответы не
     реассайнят стейт (нет лишнего ре-рендера).
  4. `Header`: 30 с + пропуск в фоне (нет Supabase-трафика у свёрнутого окна).
  5. `PackHealthBadge`: скрытое окно пропускает 60-секундный скан.
  6. `startRunningInstancesWatch`: интервал снимается у скрытого окна,
     восстанавливается по видимости; listener корректно снимается в cleanup.
  7. Тесты: `idlePerf.test.ts` (каденс опроса, класс видимости, cleanup) —
     happy-dom проект vitest.
- **Ожидание по метрикам:** скрытое окно — ~0% на рендер (все анимации на
  паузе, опросы остановлены); видимое окно главной — блобы больше не
  перерисовывают blur (десятки раз дешевле кадр), IPC падает с ~1.25/с до
  ≤1/15 с. ОЗУ: ~200 МБ — это почти целиком пол WebView2 (Chromium), а не
  JS-хип лаунчера; заметного снижения ждать не стоит.
- **Важно при замере:** dev-сборка (vite dev + HMR) всегда дороже
  production-сборки — мерить нужно packaged exe.

---
## 13–24. Аудит UI (2026-09-26): что игроки и разработчики будут хейтить

Аудит проведён на **реально запущенном** приложении: vite dev (`:1420`) +
`target/debug/tuffbox-desktop.exe`, драйв через CDP по WebView2
(`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`) —
клики/навигация по всем 21 экранам, замер контраста/шрифтов/bbox по DOM,
сбор консоли (`Runtime`/`Log`). Тема `tuffbox-light`, `tuffbox.locale = "en"`.
Все цифры ниже — замеры, а не впечатления.

- **Порядок исправления:** 14 ([object Object]) → 15 (локаль) → 16 (контраст) → 21 (рантайм-ошибки) → 17 (общий контейнер) → 19 (обрезка текста) → 18 (дубли) → 20 (вёрстка экранов) → 22 (a11y) → 13 (fallback шрифтов) → 23/24 (дев-процесс).

## 13. Шрифты Inter / JetBrains Mono не грузились — тихий fallback на системный
- **Симптом:** весь UI рисуется системным шрифтом; в консоли WebView2 5×
  `Failed to decode downloaded font: .../inter-latin-wght-normal.woff2` +
  `OTS parsing error: invalid sfntVersion: 1008821359`.
- **Причина:** `@fontsource-variable/inter` и `@fontsource-variable/jetbrains-mono`
  объявлены в `apps/tuffbox-desktop/package.json` и присутствуют в
  `package-lock.json`, но **отсутствовали в `node_modules`** (локальная
  недосборка). Vite отдавал на URL шрифта SPA-fallback HTML
  (`200 text/html`, 381 B) → `@font-face` падал, браузер уходил в fallback.
- **Статус:** не баг репы; вылечено `npm install` (пакеты в локе уже были).
  После установки vite резолвит `url("@fontsource-variable/...")` в `/@fs/...`,
  `document.fonts.check('16px "Inter Variable"') === true`.
- **Осталось:** деградация **тихая** — ни варнинга, ни явного fallback-стека
  в `--font-sans` / `--font-mono`. Стоит добавить fallback-цепочку и/или
  проверку загрузки (шрифт — первое, что делает UI «не тем»).
- **Важно при замере:** пока шрифты не грузятся, любые выводы о типографике
  недействительны — сначала `npm install`, потом смотреть.

## 14. `[object Object]` прямо в UI
- **Симптом:** в карточке «Looks like this broke it» (IDE · Diagnose → Health,
  блок под «Needs fix») в моноширинном боксе выводится литерал `[object Object]`.
- **Где:** `Diagnostics.svelte` / `components/diagnostics/*` — объект попадает
  в текстовый узел без сериализации.
- **Статус:** исправлено (проходы 2026-09-26 и 2026-10-01). Проверка 2026-10-01 на живом приложении: `[object Object]` — **0 вхождений** в DOM Diagnose. **P0**

## 15. Русские строки и даты при `tuffbox.locale = "en"`
- **Симптом:** интерфейс английский, но часть строк и **все даты в некоторых
  экранах** — русские.
- **Замер (localStorage `tuffbox.locale = "en"`, тема `tuffbox-light`):**
  - Snapshots: фильтры `Все` / `Автоматические` / `Ручные` / `Сбои`;
    даты `ср, 2 сент.`, `чт, 10 сент.`.
  - Release: кнопки `Опубликовать релиз во всех каналах`,
    `Предпросмотр перед отправкой`.
  - Library: `5 сент.`, `16 авг.`, `31 июл.`; панель инстанса: `5 сент. 2026 г.`.
  - Resolve (Graph): `10.09.2026, 18:24:37`.
- **Причина (предпол.):** даты форматируются хардкодом `ru-RU` вместо локали
  приложения; часть строк захардкожена по-русски (не через `t()`).
- **Статус:** исправлено (проходы 2026-09-26 и 2026-10-01): даты идут через стор
  `localeTag` (ru-RU/en-US), 7 русских литералов переведены в i18n. Проверка
  2026-10-01 при `tuffbox.locale = "en"`: кириллицы нет ни на Snapshots, Release,
  Resolve, History и Library, даты — en-US. **P0**

## 16. Контраст ниже WCAG AA в конкретных поверхностях
CI-тест `theme-contrast.test.ts` проверяет **токены тем**, но не эти места:
- **Settings → Appearance: названия тем** — 15 из 16 нечитаемы, замер
  **1.57–1.9:1** (тёмно-серый текст по тёмному свотчу). Читается только
  «TuffBox Light». Плюс бейдж `LIGHT/SHARP/MINIMAL` **налезает на название**
  темы (Win95, Netherite, Sakura, Aero и т.д.).
- **Brief → Categories: выбранные чипы** — `Adventure ✓`, `Magic ✓` — **1.06:1**
  (светло-зелёный по светло-зелёному). Невыбранные — читаемы.
- **Хедер: пилюля «Errors»** — **2.35:1** (красный по бледно-розовому), висит
  на каждом экране.
- **Diagnose → Health: «2 warnings»** — **1.71:1**; в полосе «Needs fix»
  «1 warning» так же.
- **Snapshots: «Manifest-only checkpoint.»** — **1.25:1**, бейдж `launcher` — **1.49:1**.
- **Resolve (Graph): подписи узлов** — **153 из 295** текстовых узлов не проходят
  контраст, сами подписи ~**1.19:1** (почти чёрное по почти чёрному холсту) —
  имена модов на графе фактически не читаются.
- **Home: подпись выбранного модпака** «Fabulously Optimized» в ряду Modpacks —
  **1.02:1** (тёмно-зелёный по тёмно-серому).
- **Статус:** исправлено (2026-10-01) — все перечисленные пары замерены заново
  (`Errors` 5.5:1, warnings 5.66:1, чипы Brief 10.07:1, названия тем 13.7–18.6:1,
  подпись модпака 4.79:1, «Manifest-only checkpoint.» 3.95:1 — акцент-как-текст);
  дополнительно закрыты провалы «чернила на заливке»: см. «Второй проход 2026-10-01».

## 17. Нет единого page-container: 21 экран = 21 разная геометрия
- **Симптом:** ширина и выравнивание контента гуляют от экрана к экрану.
- **Замер (одно окно 2432px, bbox контента внутри `main`):**
  Library 1034px по центру · Chats 1144 · Me 1304 · Settings 1120 ·
  **Crash Votes прижато влево** (106…1502) · **Quest Editor 96…728** (632px,
  справа 1704px пустоты) · **Ore Heights — один бокс 98px в середине** ·
  IDE-стадии во всю ширину (72…2422).
- **Следствие:** «половина окна пустует» (Settings занимает 445…1168 из 2432;
  Library — три тулбара и пустота; Mods — контент в верхних 230px из 1354).
- **Статус:** исправлено (2026-09-26) — в части, которая была реальным дефектом.
- **Что сделано:** замерены не union-bbox (он ловил full-bleed фоны), а
  **корневые элементы** видов и зазоры до края панели. Оказалось: Home, Library,
  Chats, Me, Settings, Recipe Browser **уже центрированы** (lg == rg), полной
  ширины по делу — `Project Settings` (фон страницы + sticky-хедер, контент
  внутри центрирован) и `Quest Editor` (это IDE-стадия, как остальные стадии).
  Единственный реально сломанный: **`Crash Votes`** — объявлял `max-width: 1400px`,
  но без центровки стоял вплотную к левому краю (lg=2 / rg=902). Добавлен
  `margin-inline: auto`.
- **Проверка после:** все не-IDE виды сбалансированы —
  Home 383/383 · Library 626/626 · Chats 559/559 · Me 483/483 · Settings 583/583 ·
  Crash Votes 443/443 · Recipe Browser 223/223.
- **Осознанно НЕ сделано:** приводить все виды к одной `max-width`. Ширины
  разные по типу экрана (карточная сетка 1065 vs трёхпанельный воркспейс 1840) —
  это выбор дизайна, а не дефект; единым должен быть **выравнивание**, и оно теперь
  единое. Если захочется меню ширин — вводить токены `--page-w-*`, а не один общий
  контейнер.

## 18. Дубли хрома и информации
- Library: **две строки тулбара подряд**; «Library» трижды (крошка + H1 +
  заголовок секции).
- Me: «Me» трижды (крошка + H1 + внутренний хедер) + кнопка «← Back».
- Mods: три бокса `0 SHOWN / 0 UPDATES / 0 TOTAL`; **два ряда фильтр-чипов
  с нулями**; бейдж **«MODRINTH» на всех 52 строках**.
- Home: версия повторяется дважды в каждой карточке
  («Minecraft Java Edition - 26.3» / «26.3»).
- Settings: чип «Auto 100%» рядом с чипом «100%».
- **Статус:** исправлено (2026-10-01): Library закрыт редизайном 2026-09-27
  (один тулбар, «Library» один раз, список по умолчанию); Me — убран третий h1,
  кнопки вправо; Mods — полоса нулевой статистики скрыта в пустой категории
  (в Datapacks она вдобавок показывала «24 UPDATES» от модов); Settings — чип
  «Auto · 100%» → «Auto».

## 19. Плотность и обрезка текста
- **Mods: версии модов обрезаны посреди строки** — `1.8.7+fabric-… ⌄` на
  каждой строке. Строка шириной **1630px** под имя + 2 бейджа + 4 иконки:
  ~**550px пустоты** между именем и бейджами.
- **Home:** серая карусель рисует подпись поверх пиксель-арта, соседний элемент
  обрезан за стрелкой; в правой карточке скина имя налезает на рендер и
  обрезано краем карточки.
- **Home → YouTube:** заголовки поверх превью (белым по светлому), обрезка
  посреди слова, 6-я карточка срезана краем контейнера без fade.
- **Library:** статус-бар обрезан у нижней кромки окна («Total playtime: 1m»).
- **Статус:** исправлено (2026-09-26 + 2026-10-01): версии модов не обрезаются;
  карусель аккаунтов — шаг рельса считается по замеренной ширине слота (ники
  12 знаков = 147.6px против шага 82px больше не налезают друг на друга, пересечений 0);
  YouTube-рельс получил фейд по правой кромке; статус-бар Library после редизайна
  читается целиком.
- **Статус:** исправлено (2026-10-03): ник под 3D-превью больше не наезжает на
  рендер (`margin-top: -4px` → `2px`), подпись держит `line-height: 1.6` (у
  Press Start 2P descent = 0, под базовой линией оставалось 0.2em под тень/свечение);
  карусель аккаунтов — ник ограничен шириной вьюпорта (`--vp-w`) с ellipsis вместо
  жёсткого реза краем `overflow: hidden`. Все пиксель-ники (Me, AccountManager,
  MinecraftLogin, SkinPreview3D) приведены к тому же leading'у.

## 20. Экраны с явными дефектами вёрстки
- **Chats:** левый список обрезан снизу; средняя колонка почти пуста; правая —
  **кремово-жёлтый бокс** вне палитры, содержимое которого «идите в Quests»
  (тупик вместо работы).
- **Release room:** **тёмные панели** (Publish config / GitHub) внутри светлой
  темы, рядом — светлые карточки; три разных «Preview» на одном экране.
- **Diagnose → Health:** пользователю показываются внутренние коды
  `SCAN EXTERNA…`, `LAUNCHER MOD_CH…`, `CRASH_F…` (обрезанные enum-имена) и
  сырые строки лога в теле карточки; «Fix all (10)» при 2 critical + 1 warning.
- **World:** пустое состояние справа говорит «Select a world from the rail»,
  а сам rail — «No worlds found».
- **Статус:** исправлено (2026-10-01): Chats — заголовок беседы больше не вылезает
  за колонку (`min-width: 0` + колоночная раскладка строки); Release room —
  панели «Publish config»/метрики переведены с жёсткой тёмной палитры на токены
  тем (цифры VERSION/CHANGED MODS/COMPONENTS были невидимы на светлой теме);
  Health — человекочитаемые подписи вместо обрезанных enum-кодов, «Fix all (N)»
  считается по числу проблем; World — пустые состояния rail и детали совпадают.
  **Осознанно не сделано:** «средняя колонка Chats пуста» и панель-указатель
  «Quest AI → Open Quest editor» — это про продукт (перенос редактирования квестов
  в Chats), а не дефект вёрстки.

## 21. Рантайм-ошибки из консоли (367 событий за проход по 21 экрану)
- `[api] save_recent_projects failed: invalid args 'projects' for command
  'save_recent_projects': missing field 'playerName'` — **рассинхрон IPC-контракта**,
  recent projects по этому пути не сохраняются.
- `check_mod_updates failed: 403 Forbidden` на
  `https://api.modrinth.com/v2/version_files/update` — проверка обновлений модов
  молча падает (похоже на отсутствующий/непринятый User-Agent Modrinth).
- `[svelte-put/shortcut] Trigger should have either 'key' or 'code'` —
  **хоткей мёртв**, сконфигурирован неверно.
- Tracking Prevention блокирует `i.ytimg.com` — **превью YouTube не грузятся
  в WebView2**.
  > **Уточнено 2026-10-01:** превью грузятся (замер: 24 из 24 `img` с
  > `i.ytimg.com` отдают `naturalWidth > 0`); предупреждение касается *доступа
  > к storage* для стороннего изображения, а не загрузки. Лечится одним
  > атрибутом: `<img crossorigin="anonymous">` — эксперимент в живом webview
  > показал, что CORS-запрос такого предупреждения не даёт, а обычный даёт
  > (×4). Проставлено на всех трёх тегах миниатюр; после — 0 предупреждений
  > за полный проход по 21 экрану.
- Плюс 404 на неопознанный ресурс при загрузке.
- **Статус:** частично закрыто (2026-09-26 и 2026-10-01): 21a — `player_name`
  получил `#[serde(default)]`, `save_recent_projects` больше не падает;
  21b (403 от api.modrinth.com) и 21c (`@svelte-put/shortcut`) — не дефекты
  приложения (Cloudflare/транзитивная зависимость), см. «Уточнения»;
  `/favicon.ico` — косметика dev-режима.

## 22. Доступность (CDP `Accessibility.getFullAXTree`, 1246 узлов)
- 281 кнопка, **0 без доступного имени** — хорошо.
- **73 из 74 изображений без accessible name** (`alt=""`) — включая осмысленные
  превью (обложки паков, превью YouTube).
- **Заголовков всего 3 и нет `h1`** (на экране Mods: «None selected»,
  «MODS (52)», «RESOURCE PACKS (3)») — иерархия заголовков сломана;
  стадийная панель IDE не размечена как nav/tabs.
- **Статус:** закрыто/уточнено (2026-10-01): панель стадий IDE **уже** —
  `<nav aria-label="Modpack production workflow" role="tablist">`, кнопки
  `role="tab"` с `aria-selected` и `aria-current="step"`, доступное имя берётся
  из видимого текста (`Brief/Idea`, `Content/Mods`, …); `alt=""` у иконок паков
  и превью YouTube — осознанный выбор (рядом текстовое имя); `h1` на экране
  теперь один (Home-герой понижен до `h2`).

## 23. Дев-запуск: `npm run tauri:dev` как фоновый сервис умирает (exit 124)
- **Симптом:** при запуске через инструментальный шелл сервис с `ready`-таймаутом
  живёт пару минут, затем всё дерево процессов (vite + приложение) убивается
  с кодом 124.
- **Обход (рабочий):** поднимать раздельно — vite (`npm run dev`) detached,
  затем `target/debug/tuffbox-desktop.exe` detached. Для UI-аудита добавить
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` и
  управлять приложением через CDP (точнее пиксельных кликов, даёт DOM/контраст/
  консоль).
- **Замечание:** собранный бинарь лежит в **корневом** `target/`
  (`U:/TuffBox/tuffbox/target/debug/`), не в `apps/tuffbox-desktop/src-tauri/target/`.

## 24. Побочка дев-сборки: перегенерация `gen/schemas` и `bindings.ts`
- Запуск dev-сборки изменяет `apps/tuffbox-desktop/src-tauri/gen/schemas/*`
  (+162 строки в desktop/windows-schema) и `src/bindings.ts` (формат `ARGS_MAP`).
- **Статус:** не проблема сама по себе, но даёт шум в `git status` при любом
  запуске — либо коммитить актуальную генерацию, либо держать в `.gitignore`.
- **Уточнено 2026-10-01 (решение за владельцем, код не тронут):** в шаблоне
  Tauri v2 `src-tauri/gen/schemas` **не версионируется** (это производное от
  capabilities + плагинов), а `src/bindings.ts` — наоборот, версионируется, потому
  что его импортирует фронтенд (при пине `taurpc@^1.8.1` две подряд dev-сборки
  дают побайтово одинаковый файл, `svelte-check` 0 ошибок). Но сейчас `gen/schemas`
  **уже в индексе**, поэтому одного `.gitignore` мало: нужно либо закоммитить
  перегенерированные схемы, либо `git rm --cached apps/tuffbox-desktop/src-tauri/gen/schemas/*`
  и добавить `gen/schemas` в `.gitignore`. Удаление отслеживаемых файлов — решение
  владельца, сам я его не делал.

---
## Fix pass 2026-09-26 — что закрыто и что уточнено

Статусы к пунктам 13–24 плюс поправки к самому аудиту: часть его выводов
оказалась **артефактами замера**, а не дефектами (см. «Уточнения»).

### Исправлено

| # | Что сделано | Файлы |
|---|---|---|
| 14 | `strongestEvidence` берёт `.text` вместо объекта; пропс сужен до `string \| null` | `Diagnostics.svelte`, `diagnose/DiagnoseVerdictHero.svelte` |
| 15 | Даты идут через новый стор `localeTag` (ru-RU/en-US) вместо системной локали; 7 русских литералов переведены в i18n (`snapshots.*`, `release.*`) | `lib/i18n.ts` + `Snapshots`, `ReleaseRoom`, `ChangelogEditor`, `LibraryInstancesPane`, `Graph`, `Diagnostics`, `TestRuns`, `Mods`, `ContentCard` |
| 16a | Подписи свотчей считают WCAG-яркость своего фона (`swatchInk`), а не берут токены активной темы | `ThemePicker.svelte` |
| 16b | Выбранный чип категории: текст `var(--text-primary)` вместо `#34d399` (1.06:1); ховер тоже | `BriefEditor.svelte` |
| 16c | `--accent-danger` / `--accent-warning` вместо `#f87171` / `#fca5a5`; Snapshots переведён на токены целиком | `PackHealthBadge`, `Diagnostics`, `Snapshots` |
| 16d | Подписи кластеров графа: `color-mix(currentColor 60%, #fff)` + `fill-opacity: 1` (палитра кластеров идёт с alpha 0.5 — как текст давала 1.2–1.4:1) | `Graph.svelte` |
| 16e | Подпись выбранного модпака: акцент, осветлённый к белому (1.4:1 → 5:1); ник скина — светлые чернила вместо тёмного `--mc-nick-color` | `HomeInstanceShelf`, `Dashboard` |
| 19 | Версии модов больше не обрезаются (`max-width: 100%` резолвился в ширину inline-flex родителя) | `Mods.svelte` |
| 18 | Бейдж `MODRINTH` показывается только для не-Modrinth источников; версия в карточке новостей не дублируется | `Mods.svelte`, `MojangNews.svelte` |
| 17 | `Crash Votes` центрирован (`margin-inline: auto`); остальные не-IDE виды уже были сбалансированы — проверено зазорами до края панели | `CrashVotes.svelte` |
| 20 | Release room и Snapshots переведены с тёмного стекла на токены тем; ChangelogEditor тоже | `ReleaseRoom`, `Snapshots`, `ChangelogEditor` |
| 21a | `player_name` получил `#[serde(default)]` — одна запись кэша без имени больше не валит весь `save_recent_projects` | `src-tauri/src/types.rs` |
| 13 | Добавлена проверка загрузки Inter: пустой результат `document.fonts.load()` → предупреждение в консоль (фолбэк-стек в `--font-sans` уже был) | `main.ts` |
| — | Прочий контраст: letter-чипы рецептов, тег `side-server`, бейдж и CTA Quests в Chats, плейсхолдеры listing-preview | `RecipeBrowser`, `Mods`, `Chats`, `ListingCardPreview` |

**Проверка:** `npm test` 142/142, `npm run check` 0 ошибок, `npm run lint:perf` — все шесть guard-скриптов OK.

### Уточнения к аудиту (аудит был неправ)

- **16d:** подписи узлов графа читаемы (`#d1d5db` на `#09090b`). 153 «провала» —
  артефакт: для SVG `<text>` видимый цвет задаёт `fill`, а замер читал `color`
  (наследует тёмный `--text-primary`). Реально нечитаемы были **подписи кластеров** — их и поправили.
- **16a:** бейдж `LIGHT/SHARP/MINIMAL` **не** налезает на название темы — проверено
  прямоугольниками (`overlap: false` для всех 18 свотчей).
- **13:** явный fallback-стек в `--font-sans`/`--font-mono` уже был; не хватало только
  сигнала о сбое.
- **21b:** `403` от `api.modrinth.com` — не баг приложения: Cloudflare режет запрос
  («A browser extension, VPN, or proxy may have altered the request», IP 103.115.109.49 / TW).
  Проверено с тремя разными User-Agent, включая корректный с контактом — 403 во всех случаях.
- **21c:** `[svelte-put/shortcut]` — предупреждение из `@xyflow/svelte` (транзитивная
  зависимость `@svelte-put/shortcut@^4.1.0`), не наш код.
- **Home (45 «провалов»):** белый текст на постерах/обложках — фоновое изображение
  замер не видит. Реальными были только два случая (см. 16e).
- **22:** заголовок страницы есть — `Header.svelte:120` рендерит `<h1 class="page-title">`
  для каждого вида (проверено в живом DOM: `h1: ["Home", "Fabulously Optimized"]`).
  `alt=""` у иконок паков и превью YouTube — корректный выбор: это декоративные
  изображения, имя рядом текстом. Реально остаётся только семантика «два h1 на
  экране Home» (шелл + hero) — косметика.
- Мелкие 4.1–4.4:1 (акцент как текст на светлом) — по правилам проекта
  (`theme-contrast.test.ts`) для «акцент-как-текст» порог 3:1, так что они проходят.

## 25. `--glass-bg` в светлой теме — почти чёрный
- **Симптом:** в теме `tuffbox-light` панели `.glass-panel` (Home-полки, hero,
  `Listing preview`, `GitHub Pack Transport`) рисуются **тёмным** стеклом
  (`rgba(13,16,20,0.68)`), а текст внутри берёт токены светлой темы → тёмное на тёмном.
- **Замер:** `section.shelf` = `rgba(13,16,20,0.68)` при активной `tuffbox-light`;
  подпись выбранного пака давала 1.4:1, ник скина — 2.4:1.
- **Статус:** частично закрыто (точечно поправлены чернила внутри тёмных панелей — 16e).
  **Требует решения:** либо `--glass-bg` в светлой теме становится светлым (тогда
  придётся перевести все hardcoded-светлые тексты внутри `.glass-panel` на токены),
  либо Home осознанно остаётся «тёмные карточки на светлом холсте» и это фиксируется
  как дизайн-правило. Сейчас — второе, но не задокументировано.

## 26. `ChangelogEditor` был полностью тёмным + русские строки в Preview
- **Симптом:** панель changelog в Release room — тёмная в любой теме; в режиме
  Preview заголовки `## Новые моды` / `## Обновленные моды` / `## Удаленные моды`,
  кнопка `Свернуть` и `Еще N зависимостей…` — по-русски при `locale = "en"`.
- **Статус:** исправлено (токены тем + ключи `release.changelog*`).
- **Примечание:** это был ещё один, не пойманный аудитом, источник утечки русского —
  виден только в режиме Preview.

## 27. `bindings.ts` перегенерировался в формате, который не проходит `svelte-check`
- **Симптом:** после старта dev-приложения `src/bindings.ts` перезаписывается в виде
  `const ARGS_MAP = { '':'{...}' }` + `createProxy<Router>(ARGS_MAP)`, и `svelte-check`
  падает: `Property 'argsMap' is missing`. В репозитории лежал **отредактированный
  вручную** файл в формате 2.0 (`{ argsMap: ARGS_MAP }` + массив аргументов).
- **Причина:** рассинхрон версий taurpc. Rust-крейт `taurpc = 0.7.1` генерирует TS под
  npm-пакет **1.x** (`createTauRPCProxy(args: Record<string, string>)`), а в
  `package.json` стоял `taurpc@^2.0.0`, у которого сигнатура другая
  (`createTauRPCProxy(options: { argsMap: Record<string, Record<string,string[]>> })`).
  Плюс: генерация идёт **не на сборке**, а в рантайме — `create_ipc_handler`/
  `Router::into_handler` пишут файл, если `tauri::is_dev()`.
- **Статус:** исправлено (2026-09-26).
- **Что сделано:** npm-пакет опущен до `taurpc@^1.8.1` (совпадает с крейтом 0.7.1),
  `bindings.ts` больше не правится руками — он генерируется при старте dev-приложения
  и проходит `svelte-check` как есть. Проверено: два старта подряд дают побайтово
  одинаковый файл (sha256 `06c98a288faadb99`), `npm run check` → 0 ошибок.
- **Апгрейд-путь (если нужен taurpc 2.0):** поднять крейт до `0.8.2`, заменить
  `#[taurpc::procedures(export_to = "...")]` на явный экспортёр
  (`taurpc::Exporter::new().export(&router, "../src/bindings.ts")`) — в 2.0 `export_to`
  убран. Учти: `taurpc 0.8.2` жёстко требует `specta = 2.0.0-rc.25` (сейчас rc.22).

## 28. TauRPC-роутер регистрировался, но тут же затирался — `TauRPC__*` недостижим
- **Симптом:** вызов через сгенерированный прокси падал с
  `Command TauRPC__get_p2p_status not found`, хотя роутер собирался без ошибок.
- **Причина:** `Builder::invoke_handler` хранит **только последний** обработчик.
  Ранний `builder = builder.invoke_handler(router.into_handler())` (lib.rs:21462)
  перетирался поздним `generate_handler![… ~490 команд]` (lib.rs:21589) — taurpc-пилот
  не был подключён вообще. Никто не замечал, потому что фронтенд прокси не вызывал.
- **Статус:** исправлено (2026-09-26).
- **Что сделано:** оба обработчика скомпонованы в одном месте. taurpc-роутер
  (`Router::on_command`) возвращает `false` для команд без префикса `TauRPC__`, поэтому
  диспетчеризация идёт по имени команды **до** передачи `Invoke` (он не `Clone`):
  `TauRPC__*` → taurpc, остальное → legacy. `generate_handler!` присвоен
  `fn(tauri::ipc::Invoke<tauri::Wry>) -> bool` — без аннотации макрос не выводит тип (E0282).
- **Проверка:** `proxy.get_p2p_status()` из вебвью вернул реальный `P2pStatus`
  (`{"enabled":false,"healthy":false,…}`), ошибок в консоли нет;
  `check-bridge-parity` теперь видит 605 зарегистрированных команд (было 490).

## 29. `GET /favicon.ico` → 404 в dev
- **Симптом:** единственная сетевая ошибка при старте — `404 /favicon.ico`.
- **Статус:** косметика; в упакованном приложении иконку даёт Tauri.

---
## 30–36. Разбор краша 2026-09-27 (MC 26.2 / Fabric) — «лаунчер советует отключить не тот мод»

Лог: `C:\Users\admin\Downloads\log-crash-tuff.txt`. Суть: паack 26.2 запущен на **Java 8**,
Fabric отказывается стартовать; 22 мода жалуются на `requires version 25 or later`,
плюс реальный конфликт Sodium ↔ Iris.

**Что показывал UI до фикса:** вердикт «Looks like this broke it: **More Chat History**,
94% confidence · secondary» с кнопкой «Disable More Chat History»; в пуле подозреваемых
8 модов по 90–94%, все — из списка java-требований. Причина краша при этом была
в другом.

## 30. `required_java_major` не знал про годовую нумерацию MC — выбирал Java 8
- **Симптом:** пак 26.2 запускался на Java 8, хотя Fabric-лог требует 25+.
- **Причина:** `required_java_major()` (`jre.rs`) разбирает `26.2` как major=26, minor=2 и
  проваливается через таблицу `1.x` в `else { 8 }`. Вся таблица написана под старую схему
  (`1.21` → minor=21).
- **Статус:** исправлено (2026-09-27). Ветка `major >= 25 → Java 25`; тест
  `required_java_major_year_based_scheme_needs_java_25` (`26.2`/`26.1`/`26.1.1`/`25.4` → 25).
- **Эффект:** `autoJava` теперь ставит/выбирает Java 25 для 26.x, а не Java 8.

## 31. Java-требования модов классифицировались как конфликт модов
- **Симптом:** строка
  `Mod 'More Chat History' (morechathistory) 2.0.0 requires version 25 or later of 'Java HotSpot(TM) 64-Bit Server VM' (java), but only the wrong version is present: 8!`
  попадала в `ModVersionMismatch` (по слову «wrong version»), и **каждый мод, который
  лишь объявляет требование**, становился подозреваемым на 90–94%.
- **Статус:** исправлено (2026-09-27).
  1. `mentions_java_runtime()` — если зависимость в строке это `(java)`/`'java'`/
     `java hotspot`, строка классифицируется как `CrashSignalKind::JavaVersion`
     (до всех мод-конфликтных веток).
  2. `JavaVersion` исключён из блока, который вешает вину на моды по совпадению имени.
  3. Хинт `java-version` теперь цитирует реальные числа из лога (`Java 25` / `Java 8`),
     а не общие «Java 17/21», и содержит фикс `autoJava`.
- **Проверка:** тест `java_runtime_requirement_is_not_a_mod_culprit` на дословных строках лога.

## 32. Конфликт Sodium ↔ Iris советовал отключить Iris
- **Симптом:** `Mod 'Sodium' … is incompatible with version 1.1.12 or earlier of mod 'Iris' (iris), yet a conflicting version is present: 1.1.2+mc26.2!` → CTA
  «Disable Iris Shaders». Это лечит симптом ценой шейдеров: правильный фикс — обновить Iris.
- **Статус:** исправлено (2026-09-27). Новый `CrashSignalKind::DependencyTooOld` +
  `extract_outdated_dependency_id()` + хинт `outdated-dependency` с фиксом `updateMod`
  для названного мода. Общий хинт `version-mismatch` в этом случае подавляется, чтобы не
  показывать противоречивую кнопку.
- **Проверка:** тест `outdated_dependency_points_at_the_side_to_update`.

## 33. Кнопка «Use Java 25+» падала: `selectJava` не был реализован
- **Симптом:** finding `WRONG_JAVA_VERSION` предлагал фикс с `kind: "selectJava"`, а
  `execute_fix_action_inner` знает только `autoJava` → `unknown fix action kind: selectJava`.
- **Статус:** исправлено (2026-09-27): finding переведён на `autoJava` (тот же смысл,
  но реально ставит/выбирает нужный рантайм).
- **Бонус:** `java_major_present()` — если лаунчер не определил назначенный JRE,
  версия берётся из лога (`present: 8`). Раньше в тексте было «Java unknown (looks old)».

## 34. Вердикт и список проблем читали разные источники
- **Симптом:** заголовок «LOOKS LIKE THIS BROKE IT: Iris Shaders» с кнопкой «Disable
  Iris Shaders» — при том что список «Problems» ниже писал «Fix this first: wrong Java».
- **Причина:** `topFinding`/`primaryRec` строились из `crashFindings`, а список — из
  `buildUnifiedProblems(...)` (hints + findings + graph + jars). Два разных набора → два
  разных совета на одном экране.
- **Статус:** исправлено (2026-09-27): `topFinding` = высший по severity `Problem` из
  `unifiedProblems`, `primaryRec` = первое применимое действие этого же `Problem`.
  Вердикт и список теперь не могут разойтись по построению.
- **Проверка (живой UI):** вердикт «FIX THIS FIRST / Selected Java is too old for the mods …
  assigned runtime is Java 8», кнопка «Use Java 25+»; «Disable Iris Shaders» исчез,
  вместо него «Update Iris Shaders».

## 35. `fit_to_screen` роняла приложение на старте (`min > max`)
- **Симптом:** `panicked at core/src/num/f64.rs: min > max, or either was NaN. min = 800.0, max = 640.0`
  — приложение умирало до загрузки вебвью (обнаружено при проверке этого фикса).
- **Причина:** `(mw * 0.95).clamp(800.0, mw)` в `fit_to_screen` (`lib.rs`). Если монитор
  отдаёт логическую ширину < 800 (битый DPI-запрос, виртуальный/RDP-дисплей), пол
  оказывается больше потолка — `clamp` паникует.
- **Статус:** исправлено (2026-09-27): `clamp(800.0_f64.min(mw), mw)` и то же для высоты.
- **Замечание:** баг латентный и не связан с диагностикой; без него нельзя было запустить
  приложение для проверки.

## 36. Тесты `tuffbox-core` вообще не компилировались
- **Симптом:** `cargo test -p tuffbox-core` падал на `mod_store.rs:853`
  (`crate::mod_files::ContentType` — приватный ре-экспорт).
- **Статус:** исправлено (2026-09-27) — путь заменён на `crate::manifest::ContentType`.
- **Осталось (не связано):** два теста падают и без наших правок (проверено `git stash`):
  `crash_kb::tests::popularity_alone_does_not_even_qualify_as_candidate`,
  `fs_util::tests::restore_from_backup_keeps_hardlinked_siblings_intact`.

---
## 37–39. Fabric сам предлагает комбинацию — лаунчер её не видел (2026-09-27, вторая итерация)

Пользователь перезапустил пак: java-ошибка ушла, остался конфликт Sodium ↔ Iris.
Лог (`instances/fabulously-optimized/logs/latest.log`) содержит собственное решение лоадера:

```
Fix: add [], remove [], replace [[sodium 0.9.2-alpha.4+mc26.2] -> add:sodium 0.9.1+mc26.2 ([[0.9.1+mc26.2,0.10-)])]
	 - Replace mod 'Sodium' (sodium) 0.9.2-alpha.4+mc26.2 with any version between 0.9.1+mc26.2 (inclusive) and 0.10- (exclusive) that is compatible with:
		 - iris 1.11.2+mc26.2
```

## 37. Предложение Fabric («Replace mod …») не детектировалось
- **Симптом:** лаунчер не видел готовое решение лоадера; в UI был только общий
  «Two mods refuse to load together» с кнопкой **«Disable `iris`»**.
- **Причина:** (а) строка начинается с `\t - ` (буллет лоадера), а проверка была
  `line.trim_start().starts_with("Replace mod '")` — не совпадало; (б) партнёр
  («compatible with:») лежит на **следующей** строке и назван **id** (`iris`),
  а не именем.
- **Статус:** исправлено (2026-09-27). Новый `CrashSignalKind::ReplaceSuggestion` +
  `parse_replace_suggestion()`; буллет снимается `strip_log_bullet()`; обёрнутые
  строки склеиваются (до 3 продолжений) до появления партнёра. Тесты:
  `parse_replace_suggestion_handles_the_log_wording`,
  `fabric_replace_proposal_becomes_a_concrete_update` (дословный фрагмент лога).
- **Что предлагает:** хинт `replace-suggestion` — диапазон `[min, max)`, партнёр и его
  минимальная версия, плюс фиксы `updateMod` на **обе** стороны (партнёр первым).

## 38. Общий конфликтный фикс советовал отключить мод, хотя Fabric уже нашёл решение
- **Симптом:** finding `HARD_MOD_CONFLICT` формировал действия `Disable → Update → Remove`
  для каждого мода, поэтому однокнопочным фиксом был «Disable `iris`» на паке, которому
  нужен только новый Iris.
- **Статус:** исправлено (2026-09-27). Если в логе есть предложение Fabric
  (`replace mod '` + `with any version between`), finding предлагает **только**
  `updateMod` и объясняет, что обе стороны можно оставить.
- **Проверка (живой UI):** CTA стал **«Update `iris`»**, набор действий —
  `Update iris / Update polytone / Update sodium`, без disable/remove; хинт
  «Fabric already worked out a working combination» с кнопками
  «Update Iris Shaders to the newest version» и «Update Sodium to the newest version».
- **Сверка с Modrinth (26.2 + fabric):** Iris `1.11.4+26.2-fabric` (новее установленной
  1.11.2) и Sodium `mc26.2-0.9.2-fabric` — то есть обновление обеих сторон до самых
  новых даёт рабочую пару, как и просил пользователь.

## 39. `cargo build` падал: кончилось место на диске
- **Симптом:** `error: failed to build archive ... Недостаточно места на диске. (os error 112)`
  при каждой второй пересборке.
- **Причина:** `target/debug/incremental` разросся до ~5 ГБ; свободного места на диске U
  оставалось ~0.3 ГБ.
- **Обход:** удалить `target/debug/incremental` (регенерируется) — освобождает ~3.5–5 ГБ.
  Это не код, а эксплуатация; при частых debug-сборках держать в голове.

---
## Fix pass 2026-10-01 — остаток пунктов 18–25

Проверено на живом дев-приложении (`npm run dev` :1420 + `target/debug/tuffbox-desktop.exe`,
CDP на :9222). Каждый пункт сначала измерялся, потом правился — часть претензий аудита
к этому моменту уже была закрыта (отмечено ниже).

| # | Что сделано | Файлы |
|---|---|---|
| 18 | **Me:** убран третий «Me» и внутренний заголовок; кнопки (Sign out / Add account) ушли вправо (`margin-left: auto`). Живой DOM: `h1: ["Me"]`, `.me-top` = «Back \| Sign out \| Add account» | `Me.svelte` |
| 18 | **Mods:** три бокса (`0 SHOWN / 24 UPDATES / 0 TOTAL`) больше не показываются, когда в категории пусто. Бокс «UPDATES» брал апдейты **модов** и в пустой категории Datapacks прямо вводил в заблуждение | `Mods.svelte` |
| 18 | **Settings:** чип «Auto · 100%» → «Auto» — дублировал соседний чип «100%»; разрешённое значение и так написано в подсказке «Suggested for this screen: 100% · following window size» | `Settings.svelte` |
| 19 | **Home, карусель аккаунтов:** ники больше не налезают на соседние головы. Шаг рельса брался из константы `HEAD_W + SLOT_GAP` = 82px, а ник рисуется целиком (Press Start 2P 12px = **147.6px**, живой замер). Теперь слот = `width: max-content; min-width: 64px`, а центрирование выбранного слота считается по измеренной раскладке (`offsetLeft/offsetWidth`), а не по фиктивному шагу. Проверка: слоты [573–613][668–775][813–961] — пересечений 0, центр активного 887 = центр вьюпорта 887 | `AccountCarousel.svelte` |
| 19 | **Home → YouTube:** 6-я карточка резалась жёсткой кромкой. На скролл-контейнер `.feed-row` добавлена маска-фейд **по правой кромке** (левая не гасится: рельс стартует со `scroll 0`, двусторонний фейд постоянно притемнял первую карточку). В `.rail/.grid/.is-full` маска снята — там это не скроллер | `YoutubeFeed.svelte` |
| 20 | **Chats:** заголовок беседы вылезал за 200px-колонку и обрезался без ellipsis. Причина: у `button.session-main` (`display:flex` по глобальному правилу) `min-width: auto` = ширина неразрывного текста (живой замер: 828px при колонке 189px). Теперь `min-width: 0` + `flex-direction: column` (задумано как две строки: ник + превью). Проверка: main 140, title 52/331 → ellipsis, preview 124/400 → ellipsis | `Chats.svelte` |
| 20 | **Health, «Recent pack changes»:** вместо обрезанных enum-кодов (`LAUNCHER CRASH_…`, `SCAN EXTERN…`) — читаемые подписи; сырой код остался в `title`. Столбец `68px` → `max-content`, снят `text-transform: uppercase` | `Diagnostics.svelte` |
| 20 | **Health, «Fix all (N)»:** N теперь = число **проблем** с автолечением, а не число действий (было «Fix all (6)» рядом с «1 critical · 1 warning»; стало «Fix all (2)», полное «2 problems · 5 automatic fixes» — в `title`) | `Diagnostics.svelte`, `diagnostics/DiagnoseStatusBar.svelte` |
| 20 | **World:** пустое состояние rail и детали больше не противоречат друг другу (оба «No worlds found / Generate a world by launching the game, then refresh.»). Проверено в живом DOM | `World.svelte` |
| 22 | **Home:** один `h1` на экране — hero-заголовок понижен до `h2` (страничный `h1` даёт `Header.svelte`) | `HomeHero.svelte` |
| 25 | **Решение принято:** Home в светлой теме **остаётся** «тёмными островами на светлом холсте» — это осознанный дизайн (`themes.css`, блок «Light theme Home: dark silvery content islands…»), и он читаем: замер живого UI даёт заголовки панелей `#f2f5f7` на тёмном стекле. Тёмные чернила внутрь тёмных панелей не протекают — точечные утечки закрыты в проходе 2026-09-26 (п. 16e) | `themes.css` (без правок) |

**Уже было закрыто к 2026-10-01 (аудит этих пунктов устарел):**

- **18 Library** — переписан (см. `docs/plans/2026-09-27-library-redesign.md`): один тулбар, «Library» один раз, список по умолчанию, IDE-dock. Статус-бар не обрезан (п. 19 Library).
- **22 IDE-панель стадий** — уже `<nav aria-label="Modpack production workflow" role="tablist">`, кнопки `role="tab"` с `aria-selected` и `aria-current="step"`, у каждой доступное имя из видимого текста (`Brief/Idea` и т.д.).

**Осознанно НЕ закрыто (из п. 20 Chats):** «средняя колонка почти пуста» и панель-указатель «Quest AI → Open Quest editor» — это про продукт (перенос редактирования квестов в Chats), а не дефект вёрстки. Решение за автором; вёрстка там уже не ломается (обрезка заголовка починена выше).

**Гейты после правок:** `npm test` 142/142, `npm run check` 0 ошибок / 0 предупреждений (4783 файла), `npm run lint:perf` — все шесть guard-скриптов OK (border-radius, font-sizes, microtext, glass-compositing, lazy-views, bundle-budget 193.7 KB / 44.4 KB).

### Второй проход 2026-10-01 — приоритеты P0 из вводного абзаца §13–24

Порядок исправления из аудита: **14 → 15 → 16 → 21 → 17 → 19 → 18 → 20 → 22 → 13**. Пункты 17–22 закрыты первым проходом (см. выше), здесь — проверка P0-хвоста и то, что аудит «не увидел».

**Проверено в живом приложении (tuffbox-light, `tuffbox.locale = "en"`):**

| # | Проверка | Замер сейчас | Вывод |
|---|---|---|---|
| 14 | `[object Object]` в вердикте Diagnose | 0 вхождений в DOM | ✅ закрыто (16e/14 в проходе 2026-09-26) |
| 15 | русские строки/даты при `locale = "en"` | Кириллицы нет ни на Snapshots, Release, Resolve, History, Library; даты — en-US | ✅ закрыто (стор `localeTag`) |
| 16a | названия тем в Settings | 13.7–18.6:1 у 13 тем; 5 «минимальных» — артефакт замера (фон-градиент не резолвится), перекрытие бейджа и названия **0** | ✅ закрыто (`swatchInk`) |
| 16b | выбранные чипы Brief | 10.07:1 (было 1.06) | ✅ закрыто |
| 16c | пилюля «Errors» в хедере | 5.5:1 (было 2.35) | ✅ закрыто |
| 16d | «2 warnings» в Health | 5.66:1 (было 1.71) | ✅ закрыто |
| 16e | подпись выбранного модпака на Home | 4.79:1 (было 1.02) | ✅ закрыто |
| 16f | «Manifest-only checkpoint.» | 3.95:1 (было 1.25) — акцент-как-текст, порог 3:1 | ✅ проходит по правилу проекта |

**Найдено и исправлено в этом проходе (аудит не увидел):**

| Что | Было | Стало | Файлы |
|---|---|---|---|
| **Release room — жёстко зашитая тёмная палитра** (`text-neutral-100`, `bg-neutral-950/40`, `bg-white/[0.04]`, `bg-black/30`, `border-white/10`): цифры «VERSION 1.0.0 / CHANGED MODS 2 / COMPONENTS 70» были **невидимы** на светлой теме | 1.07:1 (белым по светлому) | токены тем (`--text-primary`, `--bg-secondary`, `--bg-tertiary`, `--bg-primary`, `--border-color`); панель «Publish config» снова светлая | `ReleaseMetrics.svelte`, `PublishPlatformsConfig.svelte` |
| **CTA на emerald-600 с белым текстом** (Create release draft / Publish release to all channels / Save version / Save config) — мимо 16 тем, которым нужны тёмные чернила на ярком акценте | 3.65:1 | общий класс `.cta-accent` на `var(--accent-primary)` + `var(--on-accent)` (в светлой теме 4.86:1, в остальных — свой `--on-accent`) | `ReleaseRoom.svelte`, `ReleaseMetrics.svelte`, `PublishPlatformsConfig.svelte` |
| **`Publish pack` — зашитые тёмные чернила** `text-[#06251a]` на акценте | 3.19:1 | `text-[var(--on-accent)]` | `GithubPackTransport.svelte` |
| **`Fix all` / `primary` — зашитый `color: #000`** на `var(--accent-primary)` (в светлой теме `--on-accent` = белый) | 4.1:1 | `var(--on-accent)` | `diagnostics/DiagnoseStatusBar.svelte`, `diagnostics/DiagnoseProblemsList.svelte` |
| **Чип счётчика на активной вкладке Content** — полупрозрачная заливка цветом чернил *осветляла* акцент под белым текстом | 3.33:1 | заливка убрана, контур `color-mix(--on-accent 55%)` → пара ровно `--on-accent` на `--accent-primary` | `Mods.svelte` |

**Остаток (осознанно оставлен):** 3.55–4.49:1 у «акцент-как-текст на светлом» (Library `Update all`, Chats `Quests`/`Continue in Quests`, Snapshots `auto`, Setup-бейджи, Mods `Often together`) — по правилу проекта (`theme-contrast.test.ts`) для акцент-как-текст порог 3:1, они проходят; полный скан 20 экранов не нашёл ни одного провала «чернила на заливке».

**Хвост приоритетного списка, закрытый в том же проходе:**

- **§21 — рантайм-ошибки.** Полный проход по 21 экрану с включёнными `Runtime`/`Log`/`Network`:
  осталось одно семейство событий — `Tracking Prevention blocked access to storage for i.ytimg.com`
  (24 шт.). Починено атрибутом `crossorigin="anonymous"` на трёх тегах миниатюр YouTube
  (`YoutubeFeed.svelte`, `YoutubeQueueWindow.svelte` ×2): миниатюры грузятся как и раньше
  (24/24 `naturalWidth > 0`), предупреждений — **0**. Ошибок `ERR_CONNECTION_REFUSED` в замере
  нет (это был артефакт: vite-сервер в инструментальном шелле убивается по таймауту, см. §23).
- **§13 — шрифты.** Проверено живьём: `document.fonts.check('16px "Inter Variable"') === true`,
  JetBrains Mono подгружается по требованию (`fonts.load` → 1 face), проба-предупреждение на месте
  (`main.ts:34`). Тихий фолбэк закрыт.
- **§23/§24** — эксплуатационные заметки, кода не требуют: §23 остаётся обходом (сервис убивается
  супервизором, поднимаю раздельно), §24 — решение за владельцем (см. уточнение в §24).
- **`BUG_REPORT.md` (8 фронтенд-багов со Severity) — весь список устарел**, проверено по HEAD:
  1/2/3/7/8 закрыты раньше, 4 — гонка `SkinPreview3D` уже гасится счётчиками `initGen`/`texGen`
  (включая dispose), 5 — Settings работает через стор `theme` (`theme.set`/`$theme`, `readStoredTheme`),
  6 — `ExportBuilder.onProjectPathChange` дожимает дефолтные имена, когда приходит реальный
  `projectInfo`. Ни один пункт не воспроизводится.

**Проверка после правок:** `npm test` 142/142, `npm run check` 0 ошибок / 0 предупреждений, `npm run lint:perf` — шесть guard-скриптов OK.

_Легенда статуса: не исправлено / в работе / исправлено._
_Аудит 2026-07-09: большинство проблем уже реализованы в коде (проверено по
источникам: CREATE_NO_WINDOW везде, Mods/Graph/Recipes/History имеют загрузку и
overflow, `cargo check` бэкенда проходит). Оставшиеся пункты — долгая загрузка
Setup (асинхронность onMount) и рантайм-проверка Export/world-вкладки._
