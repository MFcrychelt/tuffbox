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
- **Статус:** исправлено (2026-09-08, коммит `2352ea0`, CI-подтверждено).
- **Что сделано:** написаны оба недостающих компонента
  (`listing/MdToolbar.svelte` + `listing/GalleryGrid.svelte`) в Svelte 5
  runes-стиле под точный prop-контракт, который уже использует BriefEditor.
  `vite build` зелёный; Windows EXE workflow проходит шаг `Verify frontend
  production build` (run 34204338390, SUCCESS).

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
- **Статус:** исправлено (2026-09-08, коммит `2352ea0`).
- **Что сделано:** wiring переписан на прямой `on*`-passthrough (13 замен),
  4 implicit-`any` колбэка в `Diagnostics.svelte` аннотированы.
  `svelte-check`: 21 ошибка → **0 ошибок** (warnings 79 — baseline).
  Регресс-защита — P0-4 (svelte-check в CI ловит ровно этот класс багов
  «Cannot find name»). Полная типизация 60+ пропсов вместо `any` отложена
  в P1 (см. P1-1).

## P0-3. `master` красный: `cargo test` падает (master — устаревший снапшот)

- **Симптом:** workflow `Rust` на `master` (run 33952497628): шаг `Build` ✓,
  шаг `Run tests` ✗. Все запуски на `master` красные.
- **Разбор (2026-09-08):** `master` — один коммит без общей истории с рабочей
  линией (572 файла дельты, 289 тестов против 712 в ветке). Искать падающий
  тест в этом замёрзшем снапшоте нецелесообразно: его заменит мёрж.
  Актуальная линия (`feat/github-pack-transport`, PR #6) имеет зелёный
  прогон `cargo test` от 2026-09-06 (run 34036880876, SUCCESS) — но он не
  покрывает Rust-код, добавленный уже в arena-ветке (`mod_version_req.rs`,
  детектор `check_dep_version_mismatch`, `changeModVersion`), который нигде
  ни разу не компилировался (в песочнице нет toolchain: rustup/crates.io
  заблокированы на уровне TLS; логи CI из песочницы недоступны).
- **Влияние:** нет доверия к `master`, релизиться не с чего.
- **Статус:** исправлено (2026-09-08, CI-подтверждено на ветке и на master);
  PR #8 смёржен (`66e30bd`); PR #6 закрыт как superseded.
- **Что сделано:** новый Rust-код ветки впервые скомпилирован в CI и вылечен
  строго по фактам из логов: `e00d26c` (E0599 `next_back` на `Split<&str>` →
  `rsplit().next()`; closure→free fn в `trunc`), `4c131ab` + `a5984f1`
  (2 падавших теста `mod_version_req`). Rust workflow (run 34204338314):
  Build ✓, тесты **581 passed**; Windows EXE (run 34204338390): SUCCESS.
  Замёрзший `master` заменится мёржем PR #8; PR #6 после этого закрыть как
  superseded.

## P0-4. В CI нет ворот качества — всё вышеупомянутое мёржится молча

- **Факт:** ни один workflow не запускает `npm run check` (svelte-check),
  `npm test` (vitest), `cargo clippy`, `cargo fmt --check`. Проверяется
  только: `cargo build`, `cargo test`, `vite build`, perf-guards,
  bridge-parity. Поэтому P0-1 и P0-2 попали в ветки без единого сигнала.
- **Дополнительно:** workflows триггерятся только на `master` (+ legacy
  `feat/svelte-5-migration`) и PR в `master` — ветки вида `arena/*` и
  `feat/*` без PR вообще не проверяются.
- **Статус:** написано, НЕ запушено (2026-09-08, коммит `18ef112`,
  local-only): токену песочницы не хватает scope `workflows`, push `.github/`
  отклоняется GitHub. Требуется ручной push с машины с правами:
  `git push origin arena/01a07fae-tuffbox`.
- **Что сделано:** `quick-checks.yml` (svelte-check + vitest + fnmatch-guard)
  + шаги `cargo fmt --check` / `cargo clippy -- -D warnings` в `rust.yml` +
  триггеры на все PR. После мёржа сделать job обязательными (required
  checks).

## P0-5. Дистрибуция нулевая: нет релизов, нет автообновления

- **Факт:** релизов нет (см. P0-1), в `src-tauri` нет updater-плагина,
  signed builds / installer / crash reporting opt-in (требования Stage 15)
  отсутствуют. Даже когда сборка починится, доставка = «скачай exe руками».
- **Статус:** частично (2026-09-08): опубликован первый pre-release
  [v0.1.0-pre.1](https://github.com/MFcrychelt/tuffbox/releases/tag/v0.1.0-pre.1)
  (помечен pre-release, target — ветка `@ a5984f1`, до мёржа в master).
- **Ограничение:** бинарник EXE к релизу не приложен — песочница не может
  скачать CI-артефакты (egress к blob-хранилищу заблокирован на уровне TLS).
  EXE лежит в артефактах зелёного рана ([Build Windows EXE
  #8](https://github.com/MFcrychelt/tuffbox/actions/runs/34204338390#artifacts),
  нужен логин GitHub). С обычной машины: `gh run download 34204338390` +
  `gh release upload v0.1.0-pre.1 <exe>`.
- **Остаток по Stage 15:** installer, auto-update (updater-плагин + ключ
  подписи), подпись сборок, opt-in crash reporting.

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

### P1-3. BUG_REPORT.md (2026-07-20) — закрыт
- Все 8 багов исправлены пакетным коммитом `33b9f93` («fix(frontend):
  BUG_REPORT batch 1-8», 2026-08-25), принадлежность текущей ветке
  подтверждена через историю. Состояние кода сверено с текстом коммита
  (finally-guard, спиннер по `process-exited`, `onDestroy`-чистка,
  generation counter, theme store, пересчёт имён, `$derived`,
  удалённый импорт). Дополнительно `ea1b82f` закрыл тот же Bug-2 паттерн
  в IdeNextBar/Diagnostics. Остался только опциональный рантайм-чек Bug 2/4/6.

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
