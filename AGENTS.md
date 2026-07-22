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

Перед работой по P2P / swarm / PoUW агент **MUST** прочитать `docs/13-tuffswarm-network.md` (agent checklist внутри).
