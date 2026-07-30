# 09. Author Workflow: TuffBox как DaVinci Resolve для модпаков

## Идея

TuffBox должен ощущаться не как список instances, а как **производственная линия модпака**. Лаунчер остаётся быстрым домашним экраном для выбора сборки и запуска Minecraft. Полная IDE открывается отдельной боковой кнопкой **Open IDE**.

Аналогия с DaVinci Resolve:

```text
Media → Cut → Edit → Fusion → Color → Fairlight → Deliver
```

Для Minecraft-сборок:

```text
Brief → Setup → Content → Resolve → Tune → History → Test → Diagnose → Snapshots → Export → Release
```

Каждая вкладка отвечает за один этап разработки, имеет понятный результат и не смешивает задачи.

## Главный путь автора

### 0. Launcher / Home

Цель: быстро выбрать сборку или создать новую.

Действия:

- создать instance;
- открыть существующий `.tuffbox.json`, `.mrpack`, Prism zip или папку Minecraft;
- нажать Play;
- открыть папку/логи/настройки;
- нажать боковую кнопку **Open IDE** для режима разработки.

Результат:

- выбран активный проект;
- автор переходит в IDE только когда хочет строить/отлаживать сборку.

## IDE Workflow Pages

### 1. Brief — storefront listing

Цель: собрать карточку релиза так, как её увидит игрок на Modrinth / CurseForge, и параллельно сохранить внутренние planning notes.

Действия:

- **Identity** — название, summary (мягкий лимит ~256 символов), авторы, категории;
- **Icon** — загрузка квадратной иконки в `.tuffbox/listing/icon.*` (в экспорт уходит как `overrides/icon.png` / CurseForge `pack.png`);
- **Description** — длинный markdown body с live-превью (картинки по URL и локальные из gallery);
- **Gallery** — локальные скрины / URL, вставка в описание, paste из буфера;
- **Author notes** (свёрнуто) — goal / audience / pillars / constraints / release targets / notes в `manifest.brief`.

Live preview справа: переключатель Modrinth | CurseForge card + вкладка Page preview.

При Save: `manifest.listing` + sync `project.name` ← listing.name, `project.description` ← listing.summary; auto-snapshot `update-listing`.

Результат:

- готовая storefront-карточка (summary + icon + body);
- артефакты для Export (`.mrpack` summary/icon, CF pack.png);
- planning notes без удаления контракта `manifest.brief`.

### 2. Setup — проект и runtime

Цель: определить техническую основу.

Действия:

- Minecraft version;
- loader: Vanilla/Fabric/Forge/NeoForge/Quilt;
- loader version;
- Java runtime;
- memory budget;
- JVM args;
- profiles: client/server/dev/release/low-end.

Результат:

- валидный manifest;
- базовые profiles;
- готовый runtime.

### 3. Content — моды как зависимости

Цель: добавить контент управляемо, а не вручную кидать jar-файлы.

Действия:

- **Add** browser: плотная сетка карточек (≈300px), Search glyph слева / spinner справа; placeholder по content-type; `/` фокус поиска, Esc закрывает;
- поиск Modrinth / CurseForge (и unified);
- add/update/remove + bulk install с deps (MR и CF);
- **Import…** — local jar/zip → copy в `mods/`/`resourcepacks/`/… + sync/identify;
- **Resync** folders с summary (новые / wrong-loader / duplicates);
- side labeling; source metadata;
- wrong-loader panel (disable / remove loose jars);
- missing deps → Auto-download или **Open in Resolve**;
- trail: History / Resolve / Snapshots после мутаций;
- auto snapshot перед опасными изменениями (+ pack events в History).

Результат:

- mod list;
- dependency metadata;
- reproducible manifest.

### 4. Resolve — dependency graph

Цель: понять структуру сборки и проблемы совместимости.

Действия:

- построить graph;
- missing dependencies;
- conflicts;
- duplicates;
- side mismatch;
- unknown side;
- change plan preview.

Результат:

- диагностированный граф;
- понятные причины проблем;
- план исправления.

### 5. Tune — configs/scripts/overrides

Цель: настроить сборку без потери контроля (config lab; Recipes/Quests — отдельные экраны).

Действия:

- редактировать `config/`, `defaultconfigs/`, `kubejs/`, `scripts/`, `overrides/`, `options.txt`;
- filter по имени + content search с прыжком к строке;
- Format JSON/TOML; Lint on open/save;
- snippets KubeJS / CraftTweaker (вставка в курсор);
- auto snapshot перед сохранением + ссылки в History / Snapshots;
- confirm при уходе со stage с unsaved edits.

Результат:

- tracked config changes;
- rollback-safe tuning;
- scripts/overrides готовы к экспорту.

### 5b. Quests — FTB Quests editor + AI

Цель: собирать полноценные quest lines (описания, deps, tasks/rewards) без in-game editor.

Действия:

- визуальный canvas + inspector (SNBT round-trip); AI sidebar **закрыт по умолчанию** (открыть кнопкой; состояние в `localStorage`);
- **Ctrl/Cmd+S** — Save all (SNBT); уход со stage / Reload при dirty — confirm;
- клик по validation issues → jump + fit к квесту; search: Enter = first match, Esc = clear;
- **AI sidebar** — example chips, multi-turn chat, multi-pass для линий на 20+ квестов; Advanced: Force AI / Paste JSON / Lore / Extend;
- Review → **Apply** обновляет editor в памяти; **Save** пишет SNBT (Apply ≠ Save);
- Book / Groups — drawer под toolbar (dirty-dot); Save book/groups входит в Save all;
- quest search; description formatting (`&` codes).

Результат:

- валидные `config/ftbquests/quests/**` главы;
- lore + progression, читаемые FTB Quests in-game.

### 6. History — timeline сборки

Цель: хронологический журнал всего, что произошло со сборкой (activity feed; Snapshots — отдельно, checkpoints).

Действия:

- смотреть ленту по времени (launcher ops, AI apply, ручные правки на диске);
- **Scan now** — delta vs baseline (mtime/size/hash), не dump всех файлов;
- фильтры по category / actor (`Launcher` / `Disk` / `AI` / `You`);
- jar drift: jar в `mods/` без записи в манифесте;
- Explain change / Open in Diagnose (контекст для AI, без auto-apply);
- opt-in focused scan (debounce ~60s, пока IDE открыта).

Результат:

- видимые внешние правки;
- `recent_changes` для Diagnose/AI из того же журнала;
- ссылки на snapshot при rollback-safe ops.

### 7. Test — тестовые запуски

Цель: проверить, что сборка реально запускается (launch lab; Diagnose — для разбора крашей).

Действия:

- **Smoke client** — выбранный client-профиль + preflight;
- **Run server** — выбор папки → stage only `both`/`server` mods → `server.properties` → launch + Server console;
- **Run client 4 RAM** — client с override 4096 MB;
- Quick Play в мир из `saves/`;
- sequential **matrix** профилей (stop-on-fail);
- tail `logs/latest.log` + CPU/RAM meters;
- Pass / Fail / TimedOut / Crashed + auto-capture логов и crash-reports.

Результат:

- run history с verdict badge и startup time;
- captured logs в `.tuffbox/test-runs/{id}/`;
- переход в Diagnose без дублирования ActionPlan.

### 8. Diagnose — здоровье и краши

Цель: превратить ошибки в понятный список действий (author workspace).

Действия:

- source picker: crash-report / `latest.log` / `launcher.log` / `hs_err_pid*.log`; empty-state подсказки;
- Evidence: signal groups + crash sections (клик → jump в лог);
- Suspected mods + recent snapshots; bisect checklist (disable half);
- Analysis tools: Class finder / Who depends / MCreator list;
- Failure cards: OOM, cascading (NeoForge mask), mixin, client-only, world coords;
- graph diagnostics + wrong/dup jars;
- Recent pack changes + History focus (auto-select log + highlight);
- AI ActionPlan / Rules — Review → snapshot → Apply (без silent write);
- Support pack zip (`.tuffbox/support/`) + drop/import player crash / mclo.gs URL;
- Save KB: solution, symptoms, actions JSON, notes, copy export, open folder.

Shortcuts / UX:

- Prefer crash-report; if folder empty → latest / launcher / hs_err;
- Apply ≠ auto-fix: всегда review;
- Support pack path копируется в clipboard.

Результат:

- объяснение проблемы + гипотезы;
- безопасный change plan;
- пакет для Discord/GitHub поддержки.

### 9. Snapshots — checkpoints и rollback

Цель: дать автору свободу экспериментировать. Это **не** activity feed — лента изменений живёт во вкладке History.

Действия:

- manual snapshot;
- auto snapshots перед risky changes;
- compare snapshots;
- rollback;
- diff tracked changed files.

Результат:

- безопасные checkpoint'ы;
- быстрый откат;
- точки восстановления (events ссылаются на `snapshotId` когда есть).

### 10. Export — сборка артефактов

Цель: подготовить модпак к распространению.

Цели экспорта:

- `.mrpack`;
- Prism instance zip;
- CurseForge zip;
- server pack;
- overrides;
- changelog.

Результат:

- готовые файлы релиза;
- reproducible lockfile;
- server/client file split.

### 11. Release — публикация и поддержка

Цель: довести сборку до пользователей.

Действия:

- release snapshot;
- release notes;
- Modrinth draft publishing;
- GitHub Releases;
- hotfix branch/snapshot;
- collect crash reports from users.

Результат:

- опубликованная версия;
- changelog;
- support checklist;
- план hotfix'ов.

## UX-правила

1. **Launcher не перегружать.** Home — запуск и выбор instance. IDE — отдельная кнопка.
2. **Каждая вкладка имеет результат.** Пользователь понимает, что должно получиться на этапе.
3. **Опасные действия через plan/snapshot/apply.** Нельзя молча удалять/обновлять/мигрировать.
4. **Graph — центральная модель.** Mods, configs, profiles, snapshots и export должны ссылаться на graph.
5. **AI не применяет изменения сам.** AI объясняет и предлагает план, core применяет детерминированно.
6. **Обычный игрок не обязан понимать всё.** Для него есть happy path: Create → Add mods → Resolve → Test → Export.
7. **Разработчик получает глубину.** Profiles, lockfile, diff, configs, server pack, release notes.
8. **Знакомые ментальные модели.** Нижний workflow rail — как этапы в NLE (DaVinci Resolve); Brief ≈ Modrinth listing; Content ≈ Prism/Modrinth; Tune ≈ VS Code; Test ≈ terminal runner; Diagnose ≈ вердикт → план. Отдельного Guided/Pro-режима нет: глубина в inspector/details.
9. **Layout contract (fill-stage).** Приоритетные стадии (Brief, Content, Resolve, Tune, Test, Diagnose, Quests, World) занимают высоту stage: sticky chrome ≤ ~64px, primary canvas (list/editor/graph/log) — остаток. Дублирующие jump-кнопки на History/Resolve/Snapshots не нужны в chrome — они уже на rail.
10. **Слабые ПК.** `potato-pc` глушит continuous force/3D; длинные списки и логи виртуализуются; focused scan крутится только на History/Diagnose.

## Каркас в текущем UI

Добавлена вкладка/режим:

```text
Sidebar → Open IDE → IdeWorkspace
```

Внутри IDE уже есть workflow rail:

```text
Brief | Setup | Content | Resolve | Tune | History | Test | Diagnose | Snapshots | Export | Release
```

Реальные подключённые страницы:

- Brief → storefront `manifest.listing` + collapsed Author notes (`manifest.brief`);
- Setup → ProjectSettings;
- Content → Mods (Import/Resync, CF+MR bulk, wrong-loader, Ideas «Often together», Resolve/History trails);
- Resolve → Graph;
- Tune → ConfigEditor (roots + search jump + format/lint + snippets + snapshot trail);
- History → ChangeHistory (chrono timeline, delta scan, AI context);
- Test → запуск выбранного profile и tail `latest.log`;
- Diagnose → Diagnostics (+ Recent pack changes; **AI ActionPlanReviewPanel** vs heuristic Fix plan);
- Snapshots → Snapshots with compare, rollback and inline tracked-file diff (checkpoints, not activity feed);
- Export → базовый `.mrpack` и server pack builder;
- Release → version bump, export validation, generated changelog and release snapshot.

Skeleton pages:

- больше нет полностью пустых workflow pages; публикация в Modrinth/GitHub пока будущий этап.

Следующая задача — углублять реальные сервисы: inline diff, server pack builder, Modrinth draft publishing, crash parser и change plan preview.
