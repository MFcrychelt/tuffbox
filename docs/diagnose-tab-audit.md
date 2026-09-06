# Diagnose tab: structural and performance audit

Дата: 2026-09-06

## Найденные плохие паттерны

### 1. God component

`Diagnostics.svelte` одновременно отвечает за:

- загрузку crash diagnosis;
- выбор источника лога;
- Crash Assistant;
- AI cascade;
- fix plan review;
- class search;
- duplicate/wrong-loader scans;
- group testing;
- author KB;
- ore/unify tools;
- event subscriptions;
- watchdog;
- отображение нескольких экранов.

Размер компонента превышает 3.6k строк. Это увеличивает стоимость реактивных обновлений, усложняет cancellation и приводит к тому, что изменение одной подсистемы затрагивает весь Diagnose surface.

### 2. Несколько независимых источников истины

Состояние одной и той же диагностики хранится в `diagnosis`, `crashFindings`, `aiAnalysis`, `plan`, `wrongLoaderJars`, `duplicateJarGroups`, `softVerifyStatus` и отдельных flags. После fix нужно вручную синхронизировать несколько состояний и повторно запускать разные IPC-команды. Это источник stale UI и лишних сканов.

### 3. Дублирование IPC pipeline

При открытии вкладки путь может одновременно вызывать `load(true)` из `$effect`, `onMount` и `tuffbox:open-diagnostics`. Дополнительно PackHealthBadge отдельно вызывает `get_pack_health`. До этого не было общего in-flight guard, поэтому одинаковое чтение логов и graph могло стартовать параллельно.

### 4. Нет настоящей отмены backend jobs

Watchdog сбрасывает UI-флаги через 180 секунд, но Ollama/crash scan продолжают работать. Generation guard защищает UI от late result, однако ресурсы старого запуска всё равно расходуются.

### 5. Тяжёлая работа выполняется каскадом

`load` → `runCrashAssistant` → `runAiExplain` последовательно ждут друг друга. При этом rule-based scans и чтение независимых источников можно выполнять параллельно. AI должен запускаться только после готового компактного context, а не блокировать базовую диагностику.

### 6. Кэширование по разным правилам

Crash diagnosis, graph cache и Pack Health имеют разные cache keys и TTL. Нет единого snapshot/fingerprint, поэтому UI может одновременно показать graph от одной версии проекта и crash findings от другой.

## Реализованная оптимизация

Добавлен `activeLoadPath` guard в Diagnose:

- `$effect`, `onMount` и open event больше не запускают одинаковый `get_crash_diagnosis` параллельно;
- повторный Refresh во время активной загрузки не создаёт второй pipeline;
- path фиксируется для конкретного IPC-запроса;
- завершение старого запроса не сбрасывает состояние нового проекта.
- Pack Health получил 10-секундный project-scoped cache по manifest/mods/history mtimes, поэтому badge и Diagnose не запускают одинаковые тяжёлые scans одновременно.
- rule-based Crash Assistant report получил 30-секундный cache по тому же input fingerprint: базовая diagnosis и расширенная панель больше не прогоняют одинаковые crash patterns дважды.
- class-finder cache теперь учитывает fingerprint JAR, а результаты дедуплицируются по `modId:className`; замена модов не оставляет старое ownership-решение.
- Pack Health Badge получил отдельный in-flight path guard: первая загрузка больше не дублируется до того, как `lastPath` успеет обновиться после ответа.
- загрузка состояния Group Test отложена до открытия Advanced tab; базовое открытие Diagnose больше не делает лишний IPC-запрос для инструмента, которым пользователь не воспользовался.
- graph diagnostics кэшируются единым 5-секундным project fingerprint для `get_diagnostics`, counts и Pack Health; повторные consumers не перестраивают один и тот же graph.
- cache miss защищён single-flight guard с повторной проверкой после ожидания: одновременные Badge/counts/Pack Health consumers не выполняют один cold graph build несколько раз.
- Pack Health scan также получил single-flight guard внутри blocking pool: одновременные запросы не дублируют export, wrong-loader, duplicate и quest scans.

## Выполненная структурная декомпозиция

- Группа Advanced toolbar вынесена в `diagnostics/DiagnoseAdvancedToolbar.svelte`.
- Triage, folders и scanner actions больше не живут в основном God-компоненте.
- Родитель передаёт только state и callbacks, а child отвечает за rendering и button-level disabled/loading states.
- Group Test panel также вынесен в `diagnostics/DiagnoseGroupTestPanel.svelte`; его controls и presentation больше не смешаны с AI/log/fix UI.
- Author KB form вынесен в `diagnostics/DiagnoseAuthorPanel.svelte`; поля формы и presentation отделены от сохранения/экспорта cases.
- Это создаёт границы для дальнейшего выделения Advanced tools и уменьшает связанность основной Diagnose view.

## Целевая реконструкция

1. Вынести state machine в `diagnostics/store.ts`:
   `idle → loading_base → ready → enriching → applying → verifying`.
2. Вынести панели в lazy components: `BaseReport`, `Evidence`, `AiAnalysis`, `Fixes`, `AdvancedTools`.
3. Ввести один `DiagnosticsSnapshot`:
   `runId`, `fingerprint`, `base`, `enrichments`, `findings`, `fixes`, `timings`.
4. Один backend command должен возвращать base snapshot, а отдельные enrichment jobs — добавлять AI и optional scans.
5. Использовать Abort/cancellation token на backend и generation check на frontend.
6. Объединить Pack Health Badge и Diagnose через короткоживущий project-scoped cache.
7. Запускать параллельно только независимые операции:
   - base graph;
   - filesystem health;
   - history lookup;
   - wrong-loader/duplicate scan.
8. AI и network lookups — только по явному запросу или после idle delay.
9. Перерисовывать тяжёлые панели через `{#if activeTab}` и не держать все Advanced tools активными одновременно.
10. Добавить performance marks: `base_ms`, `jar_scan_ms`, `crash_rules_ms`, `ai_ms`, `total_ms`, `cache_hit`.

## Целевые метрики

- повторное открытие без изменений: базовый результат ≤100 ms;
- повторный вызов при активной загрузке: 0 дополнительных backend jobs;
- UI base findings доступны до старта AI;
- смена источника лога не запускает повторный graph/jar scan;
- старый run не изменяет новый snapshot;
- fix после изменения проекта всегда отклоняется по fingerprint.
