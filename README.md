# TuffBox IDE

**TuffBox** — IDE для разработки Minecraft-сборок: управление модами как зависимостями, граф совместимости, снапшоты, тестовые запуски, экспорт и ИИ-анализ краш-логов.

Главный принцип проекта:

> Основная логика должна быть детерминированной и описанной кодом: зависимости, версии, совместимость, профили, снапшоты и экспорт. ИИ не принимает архитектурные решения за систему, а только анализирует логи, объясняет ошибки и предлагает план исправления.

## Основная идея

TuffBox представляет сборку Minecraft как проект:

```text
Project
├── Minecraft version
├── Loader: Fabric / Forge / NeoForge / Quilt
├── Profiles: client / server / dev / release / low-end
├── Mod dependency graph
├── Lockfile
├── Configs / KubeJS / CraftTweaker scripts
├── Snapshots
├── Test runs
├── Crash reports
└── Export targets
```

## Ключевой технический принцип

Моды, библиотеки, загрузчики, конфиги и профили описываются как **граф зависимостей**.

```text
Mod A ─requires──> Library B
Mod A ─conflicts──> Mod C
Mod D ─optional──> Library E
Mod F ─replaces──> Mod G
Mod H ─requires-loader──> Forge 47.x
```

ИИ работает поверх этого графа:

```text
Crash log
→ deterministic parser
→ suspected graph nodes
→ recent project changes
→ AI explanation
→ proposed fix plan
→ user confirmation
→ snapshot
→ apply deterministic actions
→ test run
```

## Файлы в этом репозитории идей

```text
docs/
  01-product.md             # продуктовая концепция
  02-stack.md               # стек и технические решения
  03-architecture.md        # архитектура приложения
  04-feature-map.md         # карта функций и приоритеты
  05-dependency-graph.md    # модель графа зависимостей модов
  06-ai-role.md             # границы роли ИИ

schemas/
  tuffbox.project.schema.json # схема проекта TuffBox
  mod-node.schema.json        # схема узла графа мода

examples/
  sample-project.tuffbox.json # пример проекта
  sample-graph.json           # пример графа зависимостей

src-concepts/
  rust/domain.rs              # концептуальные Rust-типы домена
  rust/resolver.rs            # концепт dependency resolver
  typescript/ui-types.ts      # типы для frontend/UI
```

## MVP-ядро

1. Project workspace.
2. Lockfile.
3. Mod dependency graph.
4. Deterministic dependency resolver.
5. Snapshots and rollback.
6. Test launcher.
7. Crash log parser.
8. AI explanation layer.
9. Config editor.
10. Export `.mrpack` / zip / Prism instance.

## Текущий статус разработки

Реализованы Stage 1–5, начаты Stage 7 (Desktop UI) и Stage 9 (Test Launcher):

```text
crates/tuffbox-core        # deterministic core + provider layer + launcher skeleton
crates/tuffbox-cli         # CLI/dev harness
apps/tuffbox-desktop       # Tauri + Svelte desktop shell
```

- Core загружает manifest, строит граф, находит конфликты, missing deps, дубликаты и side mismatch.
- Lockfile фиксирует версии, хеши, source metadata и рёбра графа.
- Snapshot system умеет создавать, списывать, сравнивать и откатывать состояние проекта.
- Provider layer подключается к Modrinth API и умеет вычислять хеши локальных JAR.
- Через CLI можно искать моды на Modrinth, добавлять/удалять/обновлять их в проекте.
- Desktop UI на Tauri + Svelte в стиле Modrinth/Lunar: dark theme, компактная sidebar с крупными иконками, главный экран с большой кнопкой Play, плитки сборок с меню быстрых действий, диалог создания сборки, настройки сборки с выбором версий из официальных API, picker Java runtime, окно лога запуска.
- Реальный асинхронный запуск Minecraft: скачивание клиента, библиотек, natives, assets, профиля Fabric/Quilt и фоновый запуск Java-процесса.
- Test Launcher умеет находить Java и готовить instance directory; реальный запуск игры — в разработке.

Подробности: `docs/08-current-implementation.md`.

## Что TuffBox не должен делать в первой версии

- Полностью полагаться на ИИ при подборе модов.
- Молча удалять или обновлять моды без снапшота и подтверждения.
- Обещать идеальную автоматическую миграцию Forge → NeoForge.
- Быть обычным лаунчером для игроков без IDE-функций.

