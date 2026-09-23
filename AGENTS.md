# Tool usage for token efficiency

## Code search — use codebase-memory-mcp FIRST

Проект проиндексирован в codebase-memory-mcp (граф знаний). Вместо grep/glob используй:

| Вместо | Используй |
|--------|-----------|
| `grep` / `glob` для поиска функций/классов | `search_graph` — ранжированный BM25 поиск |
| `grep` для поиска определений | `search_code` — объединяет grep + граф, дедуплицирует |
| `grep` для поиска кто вызывает функцию | `trace_path` — трейсинг вызовов |
| Чтение множества файлов для понимания структуры | `get_architecture` — архитектура проекта |

Форматы:
- `search_code(mode="compact")` — только сигнатуры (минимум токенов)
- `search_code(mode="full")` — с исходным кодом
- `search_code(mode="files")` — только пути файлов

## Графовые возможности codebase-memory-mcp

- **Семантический поиск**: `search_graph(semantic_query=["...", "..."])` — векторный поиск без API/ключа (nomic-embed-code встроен в бинарник). Находит функции по смыслу, а не по имени.
- **Архитектура**: `get_architecture` — языки, пакеты, точки входа, слои, границы, кластеры (Louvain), хотспоты. Всё в одном вызове.
- **ADR**: `manage_adr` — сохраняй архитектурные решения между сессиями.
- **Cross-service**: `trace_path(mode="cross_service")` — трейсинг через HTTP/gRPC/GraphQL/tRPC вызовы между сервисами.
- **Impact**: `detect_changes` — карта незакоммиченных изменений с классификацией риска.
- **Dead code**: `query_graph` с Cypher-подобными запросами — найди функции без вызывающих.
- **Cross-repo**: CROSS_* edges связывают узлы между разными репозиториями.

## Memory — engram (только важное)

`mem_save` — только для **важных архитектурных решений, исправлений багов, ключевых открытий**. Не логируй каждое действие. Критерии:
- архитектурное решение или компромисс
- причина и способ исправления бага
- неочевидное открытие (gotcha, граничный случай)
- новая конвенция или паттерн

Для поиска по сохранённому — `mem_search`.

## ECC — агентная ОС

ECC (плагин `ecc-universal`) добавляет автоматическую память сессий и обучение паттернам. Не требует явных вызовов — работает через хуки жизненного цикла:
- **session-start** — подгружает контекст предыдущей сессии
- **session-end** — сохраняет саммари сессии
- **Continuous learning v2** — наблюдает за tool-use, выявляет повторяющиеся паттерны, сохраняет как «инстинкты» (YAML с confidence score)

Дополнительно: `run-tests`, `check-coverage`, `security-audit`, `format-code`, `lint-check`, `git-summary`.

ECC и engram комплементарны: engram — для явных архитектурных решений, ECC — для автоматической сессионной непрерывности.

## Когда grep/glob всё же нужны

Только когда pattern не про индексацию кода (поиск по строкам, логам, тестовым данным, конфигам).

## Project architecture docs (read before coding)

| Topic | Doc |
|-------|-----|
| AI / ActionPlan / Crash KB | [`docs/06-ai-role.md`](docs/06-ai-role.md) |
| Federated network TuffSwarm (Knowledge Torrent + Creation Marketplace) | [`docs/13-tuffswarm-network.md`](docs/13-tuffswarm-network.md) |
| Roadmap Stage 16 | [`docs/07-development-roadmap.md`](docs/07-development-roadmap.md) |
| Product positioning | [`docs/01-product.md`](docs/01-product.md) |
| Layered architecture | [`docs/03-architecture.md`](docs/03-architecture.md) |
| Cosmetics / CSL / Me Appearance | [`docs/14-cosmetics.md`](docs/14-cosmetics.md) |

Перед работой по P2P / swarm / PoUW агент **MUST** прочитать `docs/13-tuffswarm-network.md` (agent checklist внутри).

## Desktop UI — border-radius tokens (CI-enforced)

В `apps/tuffbox-desktop` **не хардкодь** `border-radius` значениями из шкалы токенов. CI гоняет `scripts/check-border-radius.mjs`.

| Hardcoded (запрещено) | Используй |
|-----------------------|-----------|
| `border-radius: 8px` | `var(--border-radius-sm)` |
| `border-radius: 12px` | `var(--border-radius-md)` |
| `border-radius: 16px` | `var(--border-radius-lg)` |
| `border-radius: 24px` | `var(--border-radius-xl)` |

Почему: токены реагируют на toggle **Rounded corners** и на темы; литералы `8/12/16/24px` остаются круглыми, когда режим/тема требуют квадрат.

Допустимо: мелкий декор (`2px`/`3px`/`4px`) и промежуточные размеры вне шкалы (`10px`/`14px`/`18px`/`20px`/`999px`). Проверка: `node apps/tuffbox-desktop/scripts/check-border-radius.mjs`.

## Desktop UI — читаемость: контраст и минимальный кегль (CI-enforced)

В `apps/tuffbox-desktop` два guards на читаемость текста:

1. **Контраст тем (WCAG 2.1)** — `src/lib/theme-contrast.test.ts` (vitest). Парсит реальный CSS (`styles.css` + `styles/themes.css`), резолвит токены каждой из 18 тем (включая `var()`, `color-mix()` и альфа-композитинг) и падает, если любая пара (текст × поверхность, `--on-accent` × акцент, акцент-как-текст × поверхность, бейджи) ниже порога: 4.5:1 для текста и чернил на акценте, 3:1 для акцентов-как-текст. «Белый на белом» глазами не поймать — тест ловит по всем темам сразу. Меняя палитру темы, прогоняй `npm test` в `apps/tuffbox-desktop`.

2. **Минимальный кегль** — `scripts/check-font-sizes.mjs` (в `lint:perf`). Пол 11px для всего UI-текста (CSS `font-size:` и Tailwind `text-[Npx]`), включая кириллицу. Исключение — пиксельный шрифт (Press Start 2P / `--font-minecraft`): его глифы крупнее, допустимо 10px через allowlist в скрипте (с причиной). Боди-текст — от 12px.

Шрифты приложения: **Inter Variable** (UI) и **JetBrains Mono Variable** (код/логи/пути) — self-hosted через `@fontsource-variable/*`, объявления в `src/styles/fonts.css` (только latin/cyrillic сабсеты ради бюджета стартового CSS). Не хардкодь monospace-стеки — используй `var(--font-mono, …)`.

## Desktop UI — glass-поверхности и compositing (CI-enforced)

`scripts/check-glass-compositing.mjs` (в `lint:perf`) запрещает менять **compositing-состояние самой glass-поверхности** — элемента, рисующегося через `backdrop-filter` (карточки `.tb-card`/`.card`/`.panel` при «Glass transparency», rail/header/modal, `.glass-panel`, blur-правила в компонентах). `opacity < 1`, `will-change`, `filter`, `mix-blend-mode`, `mask`/`clip-path` на таком элементе заставляют WebView2 перегруппировывать backdrop-слой → **квадратный артефакт искажения фона** (баг 2026-09: вкл/выкл мода на Content переключал `.installed-card.disabled { opacity: 0.72 }` на blur-карточке). Правильно: приглушать/анимировать **дочерние** элементы, не саму поверхность. Исключения скрипта (задокументированы в его шапке): транзиентный feedback контролов (`:hover/:active/:disabled` на button/input/select/textarea), точный `opacity: 0` (скрытые элементы), entrance-анимации и `transform`. Точечное подавление — `/* glass-compositing: ignore(<причина>) */` внутри правила, причина обязательна.

## Desktop — Tauri-команды с тяжёлым I/O: только async + spawn_blocking

В `apps/tuffbox-desktop/src-tauri` **синхронные** `#[tauri::command]` выполняются на **главном потоке** приложения. Пока sync-команда ходит по диску, событийный цикл стоит: доставка ВСЕХ IPC-ответов зависает, и UI бесконечно крутит спиннеры (так Diagnose «вечно анализировал», хотя async-команды рядом уже имели таймауты/watchdog).

Правило: команда, которая читает/пишет файлы, сканирует jar, хеширует, строит граф или запускает процесс, — обязана быть `async fn` с телом `tokio::task::spawn_blocking(move || имя_impl(args)).await.map_err(|e| e.to_string())?`. См. `get_health_report` / `get_crash_diagnosis` в `lib.rs` (2026-09: конвертировано 23 команды Diagnose/AI-потока). Синхронными остаются только команды, читающие settings/кэш в памяти.

Связанное: любые запуски внешних процессов (`java -version`, `ollama --version` …) обязаны иметь дедлайн (см. `run_with_probe_timeout` в `crates/tuffbox-core/src/jre.rs`) — зависший бинарь иначе намертво блокирует prep-каскад диагностики.

## Svelte — только синтаксис Svelte 5

В `.svelte` / `.svelte.ts` / `.svelte.js` пиши **только Svelte 5**. Не используй синтаксис Svelte 4.

| Запрещено (Svelte 4) | Используй (Svelte 5) |
|----------------------|----------------------|
| `export let` | `let { ... } = $props()` |
| `let x` + реактивность через присваивание / `$:` | `$state`, `$derived`, `$effect` |
| `on:click` / `on:input` и т.п. | `onclick` / `oninput` (свойства) |
| `<slot>` / `$$slots` | `{#snippet}` / `{@render ...}` |
| `createEventDispatcher` | callback-пропсы (`onclick`, `onchange`, …) |

Новый и изменённый UI-код должен проходить через Svelte 5 runes / event props / snippets. Не смешивай оба стиля в одном файле.
