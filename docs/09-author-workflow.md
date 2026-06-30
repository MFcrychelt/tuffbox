# 09. Author Workflow: TuffBox как DaVinci Resolve для модпаков

## Идея

TuffBox должен ощущаться не как список instances, а как **производственная линия модпака**. Лаунчер остаётся быстрым домашним экраном для выбора сборки и запуска Minecraft. Полная IDE открывается отдельной боковой кнопкой **Open IDE**.

Аналогия с DaVinci Resolve:

```text
Media → Cut → Edit → Fusion → Color → Fairlight → Deliver
```

Для Minecraft-сборок:

```text
Brief → Setup → Content → Resolve → Tune → Test → Diagnose → Snapshots → Export → Release
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

### 1. Brief — идея сборки

Цель: зафиксировать, что именно автор делает.

Поля будущей реализации:

- название и описание модпака;
- целевая аудитория: dev/server/casual/low-end;
- gameplay pillars: tech, magic, exploration, performance, QoL;
- hard constraints: loader, Minecraft version, Java, RAM budget;
- forbidden mods / required mods;
- release target: Modrinth, CurseForge, private server, Prism zip.

Результат:

- pack brief;
- список ограничений;
- базовый чеклист качества.

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

- поиск Modrinth;
- add/update/remove;
- local jar import;
- side labeling;
- source metadata;
- auto snapshot перед опасными изменениями.

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

Цель: настроить сборку без потери контроля.

Действия:

- редактировать `config/`, `defaultconfigs/`, `kubejs/`, `scripts/`;
- format JSON/TOML;
- поиск по конфигам;
- snippets для KubeJS/CraftTweaker;
- auto snapshot перед сохранением.

Результат:

- tracked config changes;
- rollback-safe tuning;
- scripts/overrides готовы к экспорту.

### 6. Test — тестовые запуски

Цель: проверить, что сборка реально запускается.

Действия:

- client smoke test;
- server dry run;
- low-end profile run;
- matrix по Java/loader/profile;
- tail `logs/latest.log`;
- сохранить run result.

Результат:

- run history;
- latest.log;
- startup time;
- pass/fail для профилей.

### 7. Diagnose — здоровье и краши

Цель: превратить ошибки в понятный список действий.

Действия:

- graph diagnostics;
- crash report parser;
- suspected mods;
- связь с последними изменениями;
- AI explanation только как advisor;
- proposed fix plan без авто-применения.

Результат:

- объяснение проблемы;
- список гипотез;
- безопасный change plan.

### 8. Snapshots — история и rollback

Цель: дать автору свободу экспериментировать.

Действия:

- manual snapshot;
- auto snapshots перед risky changes;
- compare snapshots;
- rollback;
- diff tracked changed files.

Результат:

- безопасные checkpoint'ы;
- быстрый откат;
- понятная история изменений.

### 9. Export — сборка артефактов

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

### 10. Release — публикация и поддержка

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

## Каркас в текущем UI

Добавлена вкладка/режим:

```text
Sidebar → Open IDE → IdeWorkspace
```

Внутри IDE уже есть workflow rail:

```text
Brief | Setup | Content | Resolve | Tune | Test | Diagnose | Snapshots | Export | Release
```

Реальные подключённые страницы:

- Brief → сохраняемый pack brief в manifest;
- Setup → ProjectSettings;
- Content → Mods;
- Resolve → Graph;
- Tune → ConfigEditor;
- Test → запуск выбранного profile и tail `latest.log`;
- Diagnose → Diagnostics;
- Snapshots → Snapshots;
- Export → базовый `.mrpack` и server pack builder;
- Release → version bump, export validation, generated changelog and release snapshot.

Skeleton pages:

- больше нет полностью пустых workflow pages; публикация в Modrinth/GitHub пока будущий этап.

Следующая задача — углублять реальные сервисы: inline diff, server pack builder, Modrinth draft publishing, crash parser и change plan preview.
