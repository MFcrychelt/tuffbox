# Дизайн: цикл вызова инструментов (AI tool-loop) в TuffBox

Дата: 2026-10-02. Источник требований: `docs/19-studioberry-client-reference.md` §3.2 (UClient `ai-tools.ts` + `ai-tools-extended.ts`).
Связанные планы: [`2026-10-02-uclient-port.md`](2026-10-02-uclient-port.md) (трек I), [`2026-10-02-millida-port.md`](2026-10-02-millida-port.md).
Статус: слайсы 1–4 реализованы (ядро, транспорт, диспетчер, UI), слайс 5 впереди.

---

## 1. Что уже есть (проверено по коду — на этом всё и строится)

| Слой | Факт |
|---|---|
| Транспорт | `integrations.rs::call_ai_messages_with_usage(settings, system, messages, json_mode, json_schema)` — принимает **произвольный массив сообщений** в формате OpenAI, поддерживает Ollama-native, OpenAI-совместимые и Gemini-compat, структурный вывод (JSON Schema для Ollama), учёт токенов, общий таймаут 300 с, стриминг отдельной функцией. |
| Промпты/схемы | `action_plan::{ACTION_PLAN_SYSTEM_PROMPT, ACTION_PLAN_JSON_SCHEMA_HINT, KNOWN_OPS, validate_action_plan_with_inventory}` — AI уже выдаёт валидируемый `ActionPlan`. |
| Применение и откат | `apply_action_plan` (после подтверждения, со снапшотом) + `rollback_snapshot` + журнал undo (трек F). |
| Поверхности | `TuneAiSidebar`, `QuestAiSidebar`, Diagnose-AI, `AiSettingsPanel` (провайдер/модель/ключ/endpoint). |
| Мок-LLM для тестов | `integrations.rs` — `spawn_mock_llm` (throwaway TCP-сервер) уже гоняет реальный код `call_ai_messages`. |

**Чего нет:** цикла `tool_calls`. Транспорт возвращает только извлечённый контент, а не полное сообщение ассистента, и не передаёт `tools` в запрос.

## 2. Инвентаризация требований UClient

- `src/main/ai-tools.ts` — **39** имён (`list_builds … web_search`, `fetch_url`).
- `src/main/ai-tools-extended.ts` — **48** имён (`diagnose_build … set_options_value`, `import_modpack`, `backup_build`, `create_instance_share`, `launch_build`, …).
- Итого **87** имён. Берём их как ТЗ (имена и набор операций не охраняются); описания и схемы пишем свои.

## 3. Ключевое проектное решение: инструмент = возможность, а не имя UClient

UClient разводит одну возможность на несколько инструментов (`read_config` + `get_options_value` + `read_options`; `write_config` + `set_options_value` + `write_options`). Для модели это шум и рассинхрон схем.

В TuffBox инструмент — это **одна возможность**, оборачивающая уже существующую функцию ядра:
- `read_config(path)` вместо трёх читателей;
- `set_config_value(path, key, value)` вместо трёх писателей;
- `list_worlds()`, `diagnose_build()`, `find_mod_conflicts()`, `check_updates()`, `launch_build()` — по одному.

Оценка: **~40 инструментов** вместо 87 при том же покрытии. Меньше схем — меньше галлюцинаций и меньше дрейфа.

## 4. Инварианты цикла

1. **Нет орфанных `tool`-сообщений.** Каждое `assistant`-сообщение с `tool_calls` кладётся в историю *до* своих `tool`-ответов; результат всегда несёт `tool_call_id` из вызова. Это гарантия по построению, а не пост-обработка (урок из `main.ts:901–935` UClient: там историю режут `slice(-48)` и это ломает API).
2. **Ошибка инструмента — не ошибка хода.** Неизвестный/упавший инструмент возвращает `{"error": …}` как результат, модель продолжает.
3. **Потолок раундов.** `max_rounds` (по умолчанию 5, как у UClient) — цикл завершается ответом последнего хода с пометкой «остановлено по лимиту».
4. **Запись только через `ActionPlan`.** Инструменты чтения выполняются сразу; всё, что меняет проект, доступно модели **только** как предложение (`ActionPlan`), которое пользователь подтверждает — тот же путь, что и сейчас. Никаких «тихих» write-инструментов.
5. **Деградация без `tools`.** Если провайдер/модель не принимает `tools`, транспорт делает ровно одну повторную попытку без них и сообщает «инструменты недоступны» — фича не ломает существующие сценарии.

## 5. Слайсы

| # | Содержание | Где | Статус |
|---|---|---|---|
| 1 | Ядро: `ToolSpec`/`ToolCall`/`AgentTurn`, реестр read-only инструментов, цикл с инвариантами §4, тесты на фейковой модели | `crates/tuffbox-core/src/ai_tools.rs`, `ai_agent.rs` | **реализовано** — 10 инструментов, 11 тестов |
| 2 | Транспорт: `call_ai_messages_raw` (полное сообщение ассистента: `content` + `tool_calls` + usage), `tools` в payload, fallback без tools; тесты через `spawn_mock_llm` | `src-tauri/src/integrations.rs` | **реализовано** — 4 теста |
| 3 | Диспетчер: имя инструмента → существующая команда/функция ядра (read-only), реестр в одном месте | `src-tauri/src/ai_tools_dispatch.rs` | **реализовано** — 8 тестов |
| 4 | UI: трейс инструментов в чате, кнопка инспекции, тумблер | `TuneAiSidebar.svelte`, `api.ts`, команда `ai_inspect_project` | **реализовано** |
| 5 | Расширение каталога: 10 → **23** инструмента (сводка проекта, контент-паки, options/properties, поиск файлов, отчёты о крашах, снапшоты, история запусков, серверы, измерения мира, диск, метаданные jar, проект Modrinth). Инвариант «каталог ↔ диспетчер» закреплён тестом. | `ai_tools.rs` + dispatcher | **реализовано** — 23 инструмента, 19 тестов диспетчера |

**Почему 23, а не 40 «по числу имён UClient».** Число инструментов — следствие гранулярности «одна возможность = один инструмент», а не цель. Дальше добавлять имеет смысл те, под которые есть реальный вопрос пользователя и существующая функция (версии Minecraft/лоадеров, каталог предметов и иконки, скриншоты, книга квестов, KubeJS, web-lookup). Пустые обёртки «чтобы было 40» — это схемы, которые модель будет выбирать неверно.

**Слайс 1 (детали).** `ai_tools::tool_catalogue()` — 10 read-only инструментов: `pack_health`, `diagnose_build`, `read_log_tail`, `find_mod_conflicts`, `list_mods`, `check_mod_updates`, `search_content`, `list_worlds`, `read_config_file`, `java_status`; `to_openai_tools()` даёт готовый payload. `ai_agent::run_tool_loop()` — цикл с проверяемыми инвариантами: `tool`-сообщение всегда идёт после своего `assistant.tool_calls` (тест разбирает транскрипт и падает на орфане), неизвестный инструмент возвращается модели как `{"error":…}`, потолок раундов сообщается флагом `capped`. Трейты `AgentModel`/`ToolDispatcher` позволяют тестировать цикл без модели и без Tauri (в dev-dependencies ядра добавлен только `tokio`).

**Слайс 2 (детали).** `integrations::call_ai_messages_raw(settings, system, messages, tools)` возвращает `RawCompletion { turn, tools_available }`: `parse_openai_message` сохраняет `tool_calls` (и подставляет id, когда Ollama их не отдаёт), `null` content не считается ошибкой, `tools` добавляются вместе с `tool_choice:"auto"`. Если эндпоинт отвергает `tools` (эвристика по тексту ошибки), делается ровно одна повторная попытка без них и `tools_available=false` — нативный Gemini в инструменты не умеет и сообщает об этом честно. Существующая функция `call_ai_messages_with_usage` не тронута.

**Слайс 3 (детали).** `ProjectTools` — тонкий роутер: каждый инструмент вызывает ту же функцию, что и UI (`list_mods` → манифест, `read_config_file` → `helpers::safe_project_file` + чтение, `list_worlds` → `level_dat::read_world_info`, `java_status` → `jre::find_all_runtimes` + `required_java_major`, `read_log_tail` → хвост `logs/latest.log` без загрузки всего файла, `find_mod_conflicts` → диагностика резолвера **плюс** разбор строк лога тем же парсером, что у UI, `diagnose_build` → `crash::build_crash_diagnosis`, `pack_health` → `get_health_report_impl` (открыт как `pub(crate)`), `check_mod_updates` → `updater::check_manifest_updates`, `search_content` → `ModrinthProvider::search`). Всё выполняется в `spawn_blocking` (файлы, jar, сеть), вывод обрезается по 120 КБ; ошибка инструмента приходит модели как `{"error": …}`.

**Слайс 4 (детали).** Команда `ai_inspect_project(path, question, maxRounds)` собирает `TransportModel` (реализация `AgentModel` над `call_ai_messages_raw`; запоминает, принял ли провайдер `tools`) и `ProjectTools`, крутит цикл и возвращает `{ text, rounds, capped, toolsAvailable, toolRuns }`. В `TuneAiSidebar` — отдельная кнопка инспекции рядом с отправкой (иконка поиска, а не «Send»: это другой режим — ответ без плана и подтверждения), трейс в свёрнутом блоке под ответом; если провайдер отверг инструменты, ответ прямо говорит, что данных проекта в нём нет. Заодно вынес сохранение чата в `persistChat()`, чтобы инспекция не терялась при переключении сессий (раньше блок был инлайном в apply).

## 6. Что НЕ берём из UClient

`ai-action-bridge.ts` (Electron-специфика), `open_auto_cave`-подобные сервисные инструменты их бэкенда, инструменты мессенджера/релея, `open_launcher_data_folder` (у нас свои пути), «парковка» DOM-карточек.

---

*Правится по мере слайсов; после слайса 5 обновить таблицу статусов и свести итог с треком I плана 2.*
