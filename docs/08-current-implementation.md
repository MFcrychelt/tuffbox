# 08. Текущая реализация

## Что создано

Начата Stage 1–3 разработка.

Добавлен Rust workspace:

```text
Cargo.toml
crates/
  tuffbox-core/
  tuffbox-cli/
```

## tuffbox-core

Реализованы базовые модули:

```text
manifest.rs      # ProjectManifest, mods, profiles, loader, source metadata
graph.rs         # DependencyGraph, GraphNode, GraphEdge
diagnostics.rs   # Diagnostic model
crash.rs         # Crash parser / Diagnose 2.0 models and heuristics
change_plan.rs   # ChangePlan and ChangeAction
lockfile.rs      # initial lockfile generator
resolver.rs      # deterministic resolver skeleton
```

## Что уже умеет core

- Загружать `sample-project.tuffbox.json`.
- Строить граф зависимостей из manifest.
- Создавать узлы:
  - Minecraft version;
  - loader;
  - Java;
  - profiles;
  - mods.
- Создавать рёбра:
  - requires;
  - optional;
  - conflicts;
  - breaks_with;
  - replaces;
  - requires_loader;
  - requires_minecraft;
  - requires_java;
  - included_in_profile.
- Находить diagnostics:
  - missing dependency;
  - direct conflicts;
  - duplicate mod nodes;
  - wrong side in profile;
  - unknown side;
  - profile edge pointing to unknown mod.
- Создавать простой fix plan для:
  - missing dependency;
  - direct conflict;
  - crash suspects из `crash-reports/*.txt` и `logs/latest.log`.
- Анализировать краши в Diagnose 2.0:
  - находить `crash-reports/*.txt`;
  - читать tail `logs/latest.log`;
  - извлекать parser signals по `Mod File`, `Caused by`, `Mixin`, `Exception`, `Suspected Mods`, OpenGL debug, performance stalls и resource warnings;
  - маппить suspected mods на manifest по id/name/fileName/projectId;
  - формировать deterministic fix plan без авто-применения.
- Генерировать lockfile из manifest + graph с:
  - dependency edges;
  - source metadata (project_id, file_id, url, path);
  - file hashes;
  - generated timestamp.
- Создавать, списывать, сравнивать и откатывать snapshots.
- Работать с провайдерами контента:
  - `ContentProvider` trait;
  - `ModrinthProvider`: search, project info, versions, hashes, download URL, dependencies;
  - `LocalJarProvider`: sha1/sha512, file info.
- Базовый Test Launcher:
  - поиск Java (JAVA_HOME / PATH);
  - парсинг major-версии Java;
  - подготовка instance directory (mods, configs);
  - заглушка запуска с записью log-файла.

## tuffbox-desktop

Реализован Tauri + Svelte desktop shell (Stage 7):

```text
apps/tuffbox-desktop/
  src/           # Svelte frontend
  src-tauri/     # Rust backend
```

- Dark theme в стиле Modrinth/Lunar: `#0b0b0d` фон, `#1bd96a` зелёный акцент, карточки с большим border-radius, стеклянный header.
- Компактная sidebar с крупными иконками Lucide (как у Modrinth/Lunar).
- Главный экран (Home):
  - большая кнопка Play для выбранной сборки;
  - плитки последних сборок (хранятся в localStorage, не копируются в IDE);
  - плитка "Add instance" для открытия `.tuffbox.json`, `.mrpack` или Prism `.zip`.
- Настройки сборки (Project Settings):
  - Minecraft version, loader и его версия — select'ы с официальным Mojang API и мета-API loader'ов;
  - Java executable с иконкой лупы: при нажатии открывается окно со списком найденных Java (PATH, JAVA_HOME, реестр Windows, типичные папки), можно выбрать из списка или указать путь вручную; рядом отображается версия Java;
  - ползунок памяти с метками (1 GB – 16 GB);
  - JVM arguments;
  - сохранение изменений в manifest.
- Окно лога запуска (Launch Log): открывается при нажатии Play, показывает последние строки `logs/latest.log` и обновляется каждую секунду.
- Асинхронный запуск Minecraft: процесс стартует в фоновом потоке, UI не блокируется.
- Mods: установленный список отображается длинными компактными карточками, Add Modrinth использует плитки с иконками Modrinth, version/side/source-тегами и корректным Installed detection по slug/projectId. Иконки установленных Modrinth-модов догружаются lazy-командой `get_modrinth_project_icon`. Перед установкой показывается install plan с dependencies, доступна установка одного мода, установка с зависимостями и bulk install выбранных карточек через один snapshot. Перед изменением manifest создаётся auto snapshot.
- Graph: вместо сырого JSON добавлен визуальный обзор графа — runtime/profile/mod-ноды, счетчики, карточка выбранного узла, прямые связи и панель missing dependencies.
- IDE Workflow: добавлен DaVinci Resolve-like production flow: Brief → Setup → Content → Resolve → Tune → History → Test → Diagnose → Snapshots → Export → Release. Вкладки этапов перенесены в нижний sticky rail, рабочая область занимает около 76% высоты экрана, лишние hero/output panels удалены. Brief сохраняется в manifest, Test запускает реальные profiles, пишет историю запусков в `.tuffbox/test-runs.json` и показывает `latest.log`, Export собирает `.mrpack`, server pack, Prism zip и CurseForge zip, Release делает version bump, validation, artifact checklist, changelog и release snapshot.
- Config Editor: добавлена вкладка для просмотра и редактирования файлов `config/`, `defaultconfigs/`, `kubejs/` и `scripts/` с whitelist расширений, ограничением размера и auto snapshot перед сохранением.
- Change History: новая IDE-вкладка показывает историю изменений timeline-деревом по категориям Mods/Configs/Shaders/Resource Packs/World/Data/Other, поддерживает сворачиваемые preview, persistent tracked-folder чекбоксы, регистрацию выбранных папок snapshot'ом и rollback отдельного tracked file.
- Schema migrations: core умеет нормализовать manifest/lockfile schema `0.1`/`0.1.0` к текущей `0.1.0`, а desktop backend получил команды статуса и миграции manifest.
- Snapshots: UI получил rollback, compare panel и inline text diff для tracked changed files; diff теперь сравнивает содержимое файлов, а не только списки путей.
- Diagnose 2.0: Diagnostics page расширена до crash parser workspace — список crash reports, открытие выбранного отчёта, tail `latest.log` и `launcher.log`, grouped parser signals (Entrypoint/Loader mismatch/Render/Performance), crash report sections (`-- Head --`, `-- Mods --`, `-- System Details --`) с preview и parsed Mods section, suspected mods panel, последние snapshots/изменения рядом и кнопка **Create fix plan**.
- Diagnostics/Settings: переоформлены в едином стиле.
- Поддержка импорта:
  - `.mrpack` — парсинг `modrinth.index.json`, создание `tuffbox.json` в выбранной папке;
  - Prism instance `.zip` — парсинг `instance.cfg`, создание `tuffbox.json`;
  - папка Minecraft instance — автоопределение Fabric/Forge/Neoforge и версии Minecraft по модам в `mods/`.
- Главный экран:
  - кнопка Play отображает под надписью **Play** название сборки, версию Minecraft и модлоадер;
  - отдельная боковая кнопка **Open IDE** открывает production workflow по этапам, чтобы лаунчер не смешивался с рабочей IDE;
  - меню быстрых действий (троеточие) на плитке сборки: Change Version, Create Desktop Shortcut, Download Server Pack, Links, Open Folder, Create logs.zip, Copy Modpack Link, Profile Options, Clone as..., Share Profile, Repair Profile, Remove from launcher, Delete Profile.
- Модальное окно **Add Instance**: имя, выбор версии Minecraft (популярные версии сверху, затем релизы по убыванию), выбор loader (Vanilla/Fabric/Forge/NeoForge/Quilt) и его версии (по умолчанию последняя stable), папка для сохранения.
- Test Runs: вкладка Test ведёт историю запусков в `.tuffbox/test-runs.json`, показывает status/duration и умеет сохранять логи конкретного run в `.tuffbox/test-runs/<run-id>/`.
- Реальный запуск Minecraft:
  - скачивание client jar, библиотек, natives и assets по манифесту Mojang;
  - загрузка профиля Fabric/Quilt из мета-API, корректный разбор `mainClass`, проверка sha1 loader-библиотек и sequential retry/fallback для нестабильных загрузок с Fabric Maven;
  - формирование classpath через системный path separator и корректная подстановка `${library_directory}` в JVM arguments;
  - запуск Java-процесса в фоновом потоке (`spawn_and_track`), UI не блокируется;
  - использование выбранной в настройках Java или автоопределение;
  - лог пишется в `logs/latest.log`.
- Tauri commands:
  - `validate_project` — открыть и валидировать project manifest;
  - `list_mods` — список модов;
  - `get_project_brief` / `update_project_brief` — сохранение pre-production brief в manifest с auto snapshot;
  - `list_profiles` / `list_test_runs` / `capture_test_run_logs` — профили Test page, история запусков и capture логов по run;
  - `search_modrinth_mods` / `get_modrinth_project_icon` — поиск Modrinth с фильтрами текущих Minecraft/loader и lazy-загрузка иконок;
  - `add_modrinth_mod` / `remove_project_mod` / `update_project_mod` — безопасное управление модами из UI с auto snapshot;
  - `list_config_files` / `read_config_file` / `write_config_file` — безопасный Config Editor для текстовых конфигов проекта;
  - `get_project_schema_status` / `migrate_project_schema` — проверка и миграция schemaVersion manifest с auto snapshot;
  - `get_graph` — граф зависимостей;
  - `get_diagnostics` — диагностики;
  - `get_crash_diagnosis` / `create_crash_fix_plan` — Diagnose 2.0: crash reports, latest.log, suspected mods, recent snapshots и план исправления;
  - `list_snapshots` / `create_snapshot` / `diff_snapshots` / `get_snapshot_file_diff` / `rollback_snapshot` — управление snapshots, rollback и inline сравнение tracked changed files;
  - `validate_modrinth_export` / `generate_release_changelog` / `update_project_version` / `create_release_snapshot` / `list_release_artifacts` / `create_release_draft` — release workflow, artifact registry и draft metadata;
  - `export_modrinth_pack` — базовый экспорт `.mrpack` с remote mod downloads и overrides;
  - `export_server_pack` — базовый server pack zip: server-safe mods, configs/scripts, download manifest, README и start scripts;
  - `export_prism_instance` / `export_curseforge_pack` — базовые Prism/CurseForge zip builders с metadata, overrides и `tuffbox.remote-mods.json`;
  - `generate_lockfile` — генерация lockfile;
  - `launch_profile` — подготовка и запуск профиля (заглушка);
  - `import_project` — импорт `.mrpack` / Prism `.zip`.
- Запуск: `npm run tauri:dev` из `apps/tuffbox-desktop`.

## tuffbox-cli

Добавлен CLI/dev harness:

```bash
tuffbox project validate <manifest>
tuffbox project lock <manifest>
tuffbox project add-mod <manifest> <mod_id> [--side client|server|both]
tuffbox project remove-mod <manifest> <mod_id>
tuffbox project update-mod <manifest> <mod_id>
tuffbox graph print <manifest>
tuffbox graph diagnostics <manifest>
tuffbox resolve <manifest>
tuffbox snapshot create <project_dir> --name <name> [--reason <reason>]
tuffbox snapshot list <project_dir>
tuffbox snapshot diff <project_dir> <from> <to>
tuffbox snapshot rollback <project_dir> <id>
tuffbox modrinth search <query> [--mc <version>] [--loader <loader>]
tuffbox modrinth versions <project_id> [--mc <version>] [--loader <loader>]
tuffbox launch <manifest> [--profile <profile_id>]
```

Операции `add-mod`, `remove-mod` и `update-mod` автоматически создают snapshot перед изменениями.

## Статус сборки

Rust toolchain и Node.js подключены. Проект успешно собирается и проходит тесты:

```bash
cargo fmt
cargo test
cargo check -p tuffbox-desktop
npm run tauri:dev   # из apps/tuffbox-desktop
```

## Следующие задачи

1. Улучшить Crash parser: Forge/NeoForge sections, Fabric loader table, Quilt reports, deobfuscated stacktrace hints.
2. Подключить schema status/migration controls в Project Settings UI и расширить миграции под будущие версии.
3. Расширить Snapshot diff на manifest/lockfile и добавить side-by-side режим.
4. Улучшить Config Editor: подсветка синтаксиса/форматирование JSON/TOML и поиск по содержимому.
5. Улучшить Graph view: интерактивная раскладка/мини-карта и группировка по профилям.
6. Добавить change plan preview перед add/update/remove модов в UI.
7. Test Launcher: расширить установку Forge/NeoForge и улучшить захват логов/статуса процесса.
