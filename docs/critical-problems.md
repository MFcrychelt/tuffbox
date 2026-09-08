# Критические проблемы продукта

Дата аудита: 2026-09-08. Метод: воспроизведение сборок и проверок локально,
анализ CI (GitHub Actions), атрибуция всех ошибок `svelte-check`, ревизия
`BUG_REPORT.md`, сверка с роадмапом (Stage 13–15) и свежими аудитами от
2026-09-06 (`diagnostics-audit.md`, `diagnose-tab-audit.md`).

Легенда: **P0** — продукт нельзя поставить / ключевой экран падает;
**P1** — скорость разработки и качество; **P2** — гигиена.
Статусы обновлять по мере исправления.

---

## P0-1. Релизный билд фронтенда сломан — продукта нет

- **Симптом:** `npm run build` падает, `dist/` не собирается, Windows EXE
  в CI красный на шаге `Verify frontend production build`.
- **Причина:** `apps/tuffbox-desktop/src/components/BriefEditor.svelte:23-24`
  импортирует `./listing/MdToolbar.svelte` и `./listing/GalleryGrid.svelte`,
  но каталога `src/components/listing/` нет в репозитории (ни в одной ветке).
  Ошибка воспроизведена локально:
  `Could not resolve "./listing/MdToolbar.svelte" from "src/components/BriefEditor.svelte"`.
- **Влияние:** собрать и отдать пользователю нечего. Релизов в GitHub
  **ноль за всё время** (`gh release list` пуст). Критерии Stage 13 Alpha
  («5–10 реальных пользователей могут собрать тестовый Fabric modpack»)
  unverifiable. Заодно падает `lint:perf` (bundle-budget требует `dist/`).
- **Дополнительно:** `BriefEditor.svelte` — один из двух файлов с legacy
  `on:`-синтаксисом Svelte 4 (второй — `ConfigEditor.svelte`), компонент
  подключён в `IdeWorkspace.svelte:463`, т.е. это живой экран, а не мёртвый код.
- **Статус:** не исправлено.
- **План:** восстановить/написать `listing/MdToolbar.svelte` +
  `listing/GalleryGrid.svelte` (или вырезать зависимость и упростить
  BriefEditor), прогнать `npm run build`, убедиться, что Windows EXE
  проходит дальше сборки фронтенда.

## P0-2. Вкладка Diagnose → Advanced падает при открытии (ReferenceError)

- **Симптом:** открытие Advanced-режима во вкладке Diagnose бросает
  `ReferenceError` и не рендерит всю секцию (`Diagnostics.svelte:3166`,
  ветка `{:else}`).
- **Причина:** декомпозиция God-компонента (сентябрь 2026) вынесла разметку
  в `diagnostics/DiagnoseAdvanced.svelte`, но wiring остался от родителя:
  разметка ссылается на 11 имён родительской области видимости —
  `fixDisableMod` (:91), `toggleBisect` (:93), `runClassFinder` (:94),
  `runFindDependents` (:95), `fixMissingDependency` (:122),
  `fixDeduplicate` (:125), `keepOneDuplicateJar`, `disableWrongJar`,
  `removeWrongJar`, `applyFix` (:135), `saveAuthorCase` / `copyAuthorExport` /
  `openAuthorExportFolder` (:146–148) — вместо принимаемых `on*`-пропсов.
  Все 11 — ошибки `svelte-check` «Cannot find name». Пропсы компонента
  типизированы как `any`, поэтому дальше это никак не ловится.
- **Влияние:** мёртв весь Advanced surface: triage, group test, conflicts/jars
  фиксы, heuristic plan, author KB, performance phases. Это ровно те
  инструменты, ради которых существует Diagnose.
- **Статус:** не исправлено.
- **План:** переписать wiring на `on*`-пропсы (имена уже есть в деструктуризации
  `$props()`), типизировать пропсы вместо `any`, покрыть smoke-тестом
  «Advanced рендерится без исключений». Параллельно добить 6 type-only ошибок
  в `Diagnostics.svelte:3211,3229,3237,3239` (implicit `any`: `id`, `outcome`,
  `{modId, idx}`, `{modId, fileName}`) — рантайм не ломают, но держат
  `svelte-check` красным.

## P0-3. `master` красный: `cargo test` падает

- **Симптом:** workflow `Rust` на `master` (run 33952497628, 2 дня назад):
  шаг `Build` ✓, шаг `Run tests` ✗. Предыдущие запуски на `master` — тоже
  красные. В feature-ветке последний прогон зелёный, более ранние падали
  на том же шаге — возможен flaky тест.
- **Причина:** точный падающий тест не установлен — в песочнице нет Rust
  toolchain, а логи CI недоступны (обрыв соединения к results-receiver).
- **Влияние:** нет доверия к `master`, релизиться не с чего, регрессии не
  ловятся. 715 Rust-тестов есть, но их сигнал игнорируется.
- **Статус:** не исправлено (требуется прогон с toolchain).
- **План:** поставить Rust toolchain, прогнать `cargo test` на `master`,
  зафиксировать падающие тесты; flaky/network-зависимые — пометить `#[ignore]`
  с причиной (прецедент уже есть: `versions.rs`, `provider/modrinth.rs`)
  либо отвязать от сети. Заодно впервые скомпилировать новый
  `tuffbox-core/src/mod_version_req.rs` и тесты `changeModVersion`.

## P0-4. В CI нет ворот качества — всё вышеупомянутое мёржится молча

- **Факт:** ни один workflow не запускает `npm run check` (svelte-check),
  `npm test` (vitest), `cargo clippy`, `cargo fmt --check`. Проверяется
  только: `cargo build`, `cargo test`, `vite build`, perf-guards,
  bridge-parity. Поэтому P0-1 и P0-2 попали в ветки без единого сигнала.
- **Дополнительно:** workflows триггерятся только на `master` (+ legacy
  `feat/svelte-5-migration`) и PR в `master` — ветки вида `arena/*` и
  `feat/*` без PR вообще не проверяются.
- **Статус:** не исправлено.
- **План:** добавить job/stепы `svelte-check` + `vitest run` + `cargo clippy
  -- -D warnings` + `cargo fmt --check` (или отделить fmt в собственный
  быстрый job); расширить триггеры минимум на все PR. После P0-1/P0-2 —
  сделать эти job обязательными (required checks).

## P0-5. Дистрибуция нулевая: нет релизов, нет автообновления

- **Факт:** релизов нет (см. P0-1), в `src-tauri` нет updater-плагина,
  signed builds / installer / crash reporting opt-in (требования Stage 15)
  отсутствуют. Даже когда сборка починится, доставка = «скачай exe руками».
- **Статус:** не исправлено (зависит от P0-1, P0-3).
- **План:** после зелёного `master` — собрать первый pre-release artifact,
  затем по Stage 15: installer, auto-update, подпись, opt-in crash reporting.

---

## P1. Скорость разработки и структурный долг

### P1-1. God-компоненты и дубли источников истины (фронтенд)
- `Mods.svelte` — 7621 строка, `Graph.svelte` — 5170, `WorldMap.svelte` —
  3759, `Diagnostics.svelte` — 3696 (несмотря на частичную декомпозицию),
  `QuestEditor.svelte` — 3502. Детали и план декомпозиции — в свежем
  `docs/diagnose-tab-audit.md` (2026-09-06): единый snapshot, state machine,
  lazy-панели, отмена backend jobs (сейчас watchdog сбрасывает только
  UI-флаги, Ollama/сканы продолжают жечь ресурсы).
- P0-2 — прямое следствие: декомпозиция без типов и тестов ломает wiring.

### P1-2. Монолит бэкенда
- `apps/tuffbox-desktop/src-tauri/src/lib.rs` — **19 183 строки, 298
  tauri-команд**; всего зарегистрировано 466 команд, из них 77 без
  TS-вызывающих (мёртвая поверхность API). Bridge-parity при этом OK.
- Крупнейшие модули ядра: `crash.rs` (4789), `quest_plan.rs` (3846),
  `crash_assistant.rs` (3839). Детали — `docs/diagnostics-audit.md`.

### P1-3. Остатки BUG_REPORT.md (2026-07-20) — сверить и закрыть
- Bug 1 (OreGenVisualizer, infinite retry, HIGH): файл переписан на Svelte 5,
  `loadWorlds`/`lastWorldsPath` исчезли — **требует подтверждения**, что
  нового цикла нет.
- Bug 2 (`launching` сбрасывается мгновенно): переведено на стор
  `$isLaunching` + `launchHoldPath` — **вероятно исправлено**, нужен рантайм-чек.
- Bug 3 (flashTimer): **исправлено** (`onDestroy` чистит таймер).
- Bug 4 (SkinPreview3D race): переписано (`loadSkinFromCachedPath`), явной
  отмены/поколений не видно — **проверить гонку при быстрой смене скинов**.
- Bug 5 (theme desync): вынесено в `lib/themes.ts` — **вероятно исправлено**.
- Bug 6 (ExportBuilder до projectInfo): fallback `?? "modpack"` остался,
  но добавлен пересчёт при готовности info — **частично**, сверить UX.
- Bug 7 (double compute): **исправлено** (`$derived`).
- Bug 8 (unused import): **исправлено**.

### P1-4. Прочее из свежих аудитов (не дублировать, track-ссылки)
- Нет единого `DiagnosticsSnapshot`/fingerprint — UI может смешивать graph и
  findings от разных состояний проекта (`diagnostics-audit.md`, фазы 1–2).
- Resolver смешивает Finding и FixAction; строковые `code` вместо registry.
- Graph cache не учитывает fingerprint jar-файлов (stale после замены jar).
- Нет cancellation token для diagnose run (late AI-ответы после refresh).

## P2. Гигиена
- `docs/08-current-implementation.md` устарел («Начата Stage 1–3») при
  фактических Stage 9–14 в работе — обновить или удалить.
- 2 файла с legacy `on:`-синтаксисом Svelte 4 (`BriefEditor`, `ConfigEditor`)
  против требования AGENTS.md «только Svelte 5».
- 79 warnings `svelte-check` (unused CSS и пр.) — фон, чистить после P0.

---

## Проверено — проблем НЕТ (не тратить время)

- **XSS через `{@html}` (11 мест):** Modrinth-описания идут через DOMPurify
  (`lib/sanitizeHtml.ts`); лог-вьювер экранирует до подсветки; `mcFormat`
  (`packages/quest-lib`) экранирует `&<>"`; `markQuery` подставляет только
  уже экранированный текст. Self-XSS через поисковую строку невозможен.
- **CSP** настроен (`tauri.conf.json`), `dragDropEnabled: false`,
  `script-src 'self'`.
- **Секреты:** AI/GitHub/Supabase ключи — в OS keyring (`auth.rs`,
  `integrations.rs`), в webview не возвращаются; хардкода секретов в коде нет.
- **Зависимости:** `npm audit --omit=dev` — 0 уязвимостей.
- **Паники Rust:** вне тестов `unwrap/expect` — единицы, в основном
  обоснованные (`http.rs`: poisoned lock, инварианты циклов); `unsafe` —
  один легитимный (`CREATE_NO_WINDOW`); `panic!/todo!` в прод-коде нет.
- **Деструктивные операции:** перед удалением дубликатов jar — `auto_snapshot`;
  точечный `remove_file` без backup в пользовательском удалении модов не
  найден (проверено выборочно).
- **Сетевые тесты** помечаются `#[ignore]` с причиной (прецеденты есть).
- **Bridge parity:** 389 TS-команд против 466 Rust — расхождение объяснено
  (tests/CLI-only), проверка зелёная.

---

## Порядок исправления (предлагаемый)

1. P0-1 (сборка) + P0-2 (Advanced wiring) — разблокируют продукт и Diagnose.
2. P0-4 (CI-ворота) — чтобы P0-1/P0-2-класс багов больше не мёржился.
3. P0-3 (красный `cargo test` на master) — требует toolchain.
4. P0-5 (первый pre-release) — после зелёного master.
5. P1-3 (сверка BUG_REPORT), затем P1-1/P1-2 по планам аудитов.
