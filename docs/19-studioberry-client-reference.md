# Undefined Client (studioberry) — референс-отчёт 2: куски для рерайта в TuffBox

**Дата:** 2026-10-02
**Источник:** `U:\TuffBox\studioberry-client\client-main` (Undefined Client / UClient, **GPL-3.0-only + доп. условия**, Electron 43 + TypeScript 5.3)
**Цель:** `U:\TuffBox\tuffbox` (TuffBox, MIT OR Apache-2.0, Tauri 2 + Svelte 5)
**Связь:** продолжение `docs/18-millida-launcher-reference.md` (референс 1). Общая стратегия — **рерайт, не копирование** (GPL vs MIT/Apache, см. раздел «Лицензии»).
**Назначение файла:** быстрый справочный источник для ИИ-агентов.

**Методика:** `repomix-output.xml` (36 МБ) использовался только точечным grep (внутренности `eml-lib`); основной анализ — прямое чтение кода. Билд-артефакты (`src/renderer/app.js` 49k, `world/world.js` 694k, `threeWorker.js`, `mesherWasm.js`, `console.js` — выводы esbuild) **не анализировались**.

---

## TL;DR для агента

1. **Запуск Minecraft переносить нельзя и не нужно:** он идёт через `eml-lib 2.6` (npm, чёрный ящик, патчится в рантайме) — у TuffBox свой Rust-пайплайн, он сильнее. Ценность UClient — в обвязке вокруг.
2. **Топ-кандидаты для рерайта (новое для TuffBox):**
   - **3D-просмотр мира** — `src/main/anvil.ts` + `worldViewer.ts` + `src/shared/world-preview-matrix.ts` + `src/renderer/worldlib/*` (в TuffBox только 2D-карта `WorldMap.svelte`);
   - **AI confirm/undo/diff UI** — `src/renderer/ai/confirm-ui.ts`, `turn-ui.ts`, `types.ts` (паттерна «подтверждение действия + undo-стек + build-diff» в TuffBox нет вообще);
   - **Шаринг инстансов ссылкой** — `src/main/instanceShare.ts` (фичи нет; замена бэкенда UClient — GitHub Pack Transport или свой);
   - **Эталоны усиления своего кода** — `importModpack.ts` (ZIP64 + path-traversal), `deepLink.ts` (валидация payload), `modDependencies.ts` (батчи по 80 + кэш).
3. **Не тащить:** `app.ts` (14 666 стр., DOM-монолит на innerHTML), `messenger/ui.ts` (6 223, чужой бэкенд), `main.ts` (Electron), `installer/*.py`, локали кроме ru/en, собственные темы (у TuffBox 19 тем + WCAG-тесты).
4. **3D-движок скин** (`skinviewengine/`) — в TuffBox уже заменён на `skinview3d` (MIT); дельта к нему есть готовая: движок **уже вендорен в Millida-репо как `src/vendor/mine3d` под MIT** («Copyright (c) 2026 Undefined Studio») — законный источник для TuffBox.
5. **Риски:** `emotes/*` (конверт из SPEmotes/Emotecraft, лицензия не указана) и `skin-outer-voxel.ts` (порт 3D Skin Layers) — **не переносить до выяснения лицензий**.

---

## 1. Профиль репозитория

| Параметр | Значение |
|---|---|
| Стек | Electron 43 + TypeScript 5.3, esbuild, electron-builder (NSIS) |
| Запуск MC | `eml-lib ^2.6` (чёрный ящик; патчится в рантайме: sha1-верификация загрузок `launcher.ts:260`, подмена JSON-кэша `:358`) |
| Объём исходников TS | ≈ 44 000 строк (`src/`), плюс `skinviewengine/` ≈ 7 000+ |
| Крупнейшие файлы | `renderer/app.ts` 14 666, `messenger/ui.ts` 6 223, `main/launcher.ts` 4 987, `skinviewengine/core/scene-loop.ts` 1 837, `main/ai-tools.ts` 2 244, `ai-tools-extended.ts` 1 919, `importModpack.ts` 1 424 |
| Свой бэкенд | `https://uprojects.site/client` — каталог/прокси, messenger, AI-прокси, relay, instance-share, news, auto-update mirror |
| Лицензия | `package.json:5` → GPL-3.0-only + доп. условия (CLA со `Signed-off-by`, запрет снятия уведомлений, имя «UClient» и логотипы под запретом) |
| Установщик | Python (`installer/installer.py` 1800+ стр., TK-UI, запись схемы `uclient://` в HKCU) + `updater.py` (9-строчный трюк, убивает клиент и ждёт) |

### Карта `src/main/` (20 файлов)

| Файл (строк) | Что делает | Портируемость |
|---|---|---|
| `launcher.ts` (4987) | Ядро: `initLauncher` (589), `runLaunch` (4414), Java-детект `detectJavaRuntimes` (3542 — PATH+реестр), auth (Ely OAuth 1325, MS через eml-lib 1457, offline 1463), Discord RPC (142–227), косметика/CSL (1820–2600), enrichment инстанса `scanFolder`/`enrichWithModrinth` (3775–3948, свой ZIP-reader `readZipEntry` 3672), краш-репорт (4362), NBT `servers.dat` (4850), 63 IPC-хендлера | ⚠️ Только фрагменты (см. §3.8). Сам запуск — нет (eml-lib) |
| `importModpack.ts` (1424) | Импорт mrpack/CF/zip: свой ZIP Central Directory reader + **ZIP64** (110), `extractArchive` через `tar` (276), **path-traversal защита** (406), CF-каталог-скрейп (1002) + forgecdn fallback (995) | ✅ Эталон усиления своего `importer.rs` |
| `modDependencies.ts` (811) | Резолвер: BFS `resolveModInstallPlan` (405), батчи `/v2/projects?ids=` **по 80** (244), `pickCompatibleModrinthVersion` (147), кэш `.uclient-cache.json` (188), типы required/optional/incompatible/embedded, `installModWithDependencies` (768) | ✅ Сравнить со своим граф-резолвером |
| `curseforgeFingerprint.ts` (86) | MurmurHash2 (seed=1) со стрипом whitespace | ⚠️ У TuffBox уже есть `crates/tuffbox-core/src/murmur2.rs` — только сверка на эталонных файлах |
| `modrinthDownload.ts` (262) | Proxy-first → direct CDN, 503 backoff, `.part`+sha1+**stall-timeout 60 с**, лимит прокси **4 параллельные загрузки на IP** | ✅ Сверить со `download_engine.rs` |
| `anvil.ts` (651) | Свой парсер Anvil RegionFile: WireColumn/WireSection, **DataVersion 1519–4903 (1.13–26.2)**, spanning/no-span битмассивы (1.13–1.17), `.mcc`/LZ4, completeSkyLight | ✅ **Новая фича для TuffBox** — рерайт на Rust |
| `worldViewer.ts` (792) | `describeWorld` (509), WorldSession с LRU (8 регионов / 4096 колонок), IPC `worldview:*`, `listAllWorlds` | ✅ Новая фича (бэкенд-часть) |
| `clientJarAssets.ts` (128) | Извлечение block-текстур из client.jar для вьюера | ✅ Маленький |
| `worldExport.ts` (350) | Экспорт мира через внешний бинарь `Arcus92/minecraft-web-exporter` | ⚠️ Спавн бинаря из Tauri — возможно |
| `instanceShare.ts` (690) | Шаринг инстанса ссылкой: пакет, заливка, импорт по ссылке | ⚠️ Идея + формат; бэкенд чужой — замена: GitHub Pack Transport |
| `exportInstance.ts` (372) | Экспорт инстанса (zip/mrpack, adm-zip) | ✅ Сверить с `exporter.rs` |
| `deepLink.ts` (462) | Схема `uclient://`: `install\|import-instance\|join-group\|launch`, строгая валидация payload | ✅ Эталон к `deep_link.rs` |
| `game-process.ts` (265) | Спавн/мониторинг процесса, чтение лога, перехват крашей | ✅ Сверить с `process.rs` |
| `game-relay.ts` (774) | LAN-порт из latest.log, TCP-туннель к VPS (`HOST/DATA/CONNECT`, relayFetch `/api/relay`) | ⚠️ Протокол портируем, нужен свой endpoint |
| `ai-tools.ts` (2244) | 47+ инструментов, `listAiToolSchemas` (2036), `runAiTool` (2178), deferred-действия (`install_mod` → `install_content`) | ⚠️ **Схемы — готовое ТЗ**; обёртки над своей логикой |
| `ai-tools-extended.ts` (1919) | Ещё ~50 инструментов через helper `tool()` (89): `diagnose_build`, `find_mod_conflicts`, `check_updates`, `install_java`, `launch_build`, `backup_build`, `update_outdated_mods`… | ⚠️ То же |
| `ai-action-bridge.ts` (58) | Мост отложенных действий AI → IPC | ❌ Electron-specific (идея переносима) |
| `messenger-api.ts` (369) | Чат/сессии/вложения через `/api/messenger/*` + bearer | ❌ Чужой бэкенд |
| `main.ts` (976) | Bootstrap: GPU-свитчи ANGLE (25), схема `app://` для WASM (164), WebContentsView-модалка мира (279), single-instance (476), deep-link lifecycle (438–502), автообновление `updates:check` (731 — сайт→GitHub fallback) / `updates:launch` (791 — spawn `updater.exe`), AI-прокси `ai:chat` (867) с **санацией tool-call орфанов (901–935)** | ❌ Electron; но кейсы полезны |

---

## 2. Что в TuffBox уже есть (не дублировать)

Проверено по `U:\TuffBox\tuffbox`:

- **CurseForge fingerprint** — свой `crates/tuffbox-core/src/murmur2.rs` ✅
- **Импорт mrpack/CF/Prism** — `importer.rs` (zip-slip-guard уже есть `:562, 583`, тесты `:1334`) ✅
- **Deep-link** — `deep_link.rs` (`tuffbox://install?repo=…`) ✅
- **Скины 3D** — `SkinPreview3D.svelte` на `skinview3d@^3.4.2` ✅ (движок UClient не нужен)
- **Карта мира 2D** — `WorldMap.svelte` (canvas 2D, pan/click/region/poly, toDataURL); **3D-просмотра нет** ❌
- **AI** — `crash.rs` (5 324 стр.), `AiSettingsPanel`/`TuneAiSidebar`/`QuestAiSidebar` — но **нет confirm/undo/diff-паттерна** ❌
- **Экспорт** — `exporter.rs` (mrpack/Prism/server pack) ✅
- **Процесс игры** — `process.rs` (orphan-watcher, kill, реестр `running-instances.json`) ✅
- **Логи/краш** — `errorReport.ts`, `launchFailure.ts` ✅

---

## 3. Кандидаты для рерайта (по приоритету)

### 3.1 ★★★ 3D-просмотр мира — новая фича (1–2 недели)

Единственный крупный **новый** функциональный блок из UClient. Состав:

| Файл | Объём | Что рерайтить |
|---|---|---|
| `src/shared/world-preview-matrix.ts` | **135 стр.** | Чистые функции без DOM/Electron → `src/lib/worldPreviewMatrix.ts`: `MESHER_MAX_DATA_VERSION=4435 (1.21.6)`, `READ_MAX_DATA_VERSION=4903 (26.2)`, `MIN_READ_DATA_VERSION=1519 (1.13)`, `resolvePreviewStrategy` → `live\|export\|unsupported`, `guessVersionName`, `previewBadge`. **Самый дешёвый кандидат, рерайт 1:1 по логике** |
| `src/main/anvil.ts` | 651 | Парсер Anvil → **на Rust** (в Tauri читать регионы безопаснее и быстрее, чем в Electron main с `'r+'`) |
| `src/main/worldViewer.ts` | 792 | WorldSession/LRU/describe → в Rust-слой `api.worlds.*` |
| `src/main/clientJarAssets.ts` | 128 | Извлечение block-текстур из client.jar |
| `src/renderer/world.ts` + IPC wire-формат | 703 | **Архитектура**: батч-ингест с бюджетом **5 мс/кадр**, `ColumnStore` с `knownMissing`/waiter-пробуждением, NEAR 3 → FAR 6, mesher в воркерах — бюджеты не терять, иначе UI подвиснет |
| `src/renderer/worldlib/fly-controls.ts` | 271 | WASD + pointer lock, конвенция `rendererYaw = π − mcYaw`, throttled `onMove` — математика переносится дословно по смыслу |
| `src/renderer/worldlib/player-marker.ts` | 83 | Сущность «save-player»: скин + ник шрифтом `mojangles` |
| `src/renderer/worldlib/mapping.ts` | 350 | Имена блоков/биомов → state ID реестра 1.21.6, rename-/drop-пропсы, статистика промахов (привязан к версии реестра — нужен план обновления) |

**Ограничения (переносить осознанно):**
- чтение миров только **1.13–26.2** (DataVersion 1519–4903), миры 1.12 и старше не открываются;
- мешер отстаёт от чтения: `MESHER_MAX 4435` при `READ_MAX 4903` → миры новее 1.21.6 показываются стратегией `export`/`unsupported` (принципиальное ограничение, не баг);
- рендер-цепочка UClient хрупкая: `scripts/build-world.mjs` вручную чинит npm `minecraft-renderer` (несуществующий `main` в package.json, **изолированная копия three 0.184 рядом с 0.156**, шимы zlib/util/assert, подмена `minecraft-data` на мини-шим «из-за ~500 МБ JSON»), `copy-world-assets.mjs:38–50` патчит minified `mesherWasm.js` с обрывом при смене сигнатуры; WASM-мешер грузится по абсолютному пути → кастомная схема `app://`.
- **Лицензия рендера:** сам `minecraft-renderer@0.1.96` (zardoy) — **MIT** (подтверждено), `prismarine-chunk`/`mc-assets`/`prismarine-nbt` — публичные пакеты. GPL покрывает только обвязку UClient (`world.ts`, `worldlib/*`, `anvil.ts`, `worldViewer.ts`) — её рерайтим.
- **Альтернатива:** подключить `minecraft-renderer` отдельным entry с алиасами под three 0.184, либо отказаться от 3D в пользу существующей 2D-карты.

### 3.2 ★★★ AI confirm / undo / diff UI — единственный паттерн, которого нет (3–5 дней)

| Файл | Объём | Что даёт |
|---|---|---|
| `src/renderer/ai/confirm-ui.ts` | 516 | Карточка подтверждения в ленте (`askAiConfirmInChat` → `Promise<boolean>`), батч `askAiConfirmBatch` (approve/reject all), undo-стек на **20 шагов** (`pushAiUndo`/`renderAiUndoChip`), `renderAiBuildDiff` (компактный diff «Java 25 → 8»), `describeAiAction` (title/detail человекочитаемо) |
| `src/renderer/ai/types.ts` | 42 | Модели: `AiConfirmRequest{risk:'write'\|'read'}`, `AiPlanStep`, `AiUndoAction` |
| `src/renderer/ai/turn-ui.ts` | 388 | Ход агента: план/статусы/stop/skeleton, `wrapAiToolCollapsible` |
| `src/renderer/ai/shell-ui.ts` | 388 | Контекст-бар, empty-state сценарии |
| `src/renderer/ai/attach-ui.ts` | 953 | @-меню как в Cursor: build/mod/rp/shader/dp/file/crash/log/web/commands/diagnose, лимиты `MAX_ATTACH=12`, `MAX_TEXT_CHARS=12_000` |
| `src/renderer/ai/integrations-ui.ts` | 177 | Краш-цитата в `<details>`, баннер, бейдж «модифицировано ИИ» в sessionStorage |

**Куда:** Svelte-компоненты рядом с `TuneAiSidebar.svelte` / `QuestAiSidebar.svelte`. **НЕ копировать:** «парковку» DOM-карточек (`parkAiConfirmsFromRoot`/`restoreAiConfirmsForSession` — перенос pending-карточек через реальный DOM) — в Svelte это нормальное реактивное состояние.

**Цифры для переноса:** `AI_MAX_TOOL_ROUNDS=5`, `AI_CONTEXT_BUDGET_TOKENS=65_000` (компакт при 0.88); хранение сессий перевести со `localStorage` («Undefined Client-ai-sessions») на свой стор.

**Схемы инструментов как ТЗ:** `ai-tools.ts` (47+) + `ai-tools-extended.ts` (~50 через helper `tool()` `:89`) = **~100 имён с JSON-схемами** (`listAiToolSchemas` `:2036`) — готовая спецификация расширения AI-слоя TuffBox. Обёртки тонкие, над своей логикой.

**Кейс из `main.ts:901–935` — санация tool-call орфанов:** `slice(-48)` в истории оставляет `tool`-сообщения без парных `assistant.tool_calls` → OpenAI-совместимые API молча ломаются; лечится фиктивными `interrupted/incomplete`. Взять к сведению для своего AI-прокси.

### 3.3 ★★★ Шаринг инстансов ссылкой (фичи нет; 1 неделя + решение по бэкенду)

`src/main/instanceShare.ts` (690) + `src/shared/instanceShare.ts` (90): пакет инстанса → заливка → импорт по ссылке. **В TuffBox фичи нет** (проверено: найденные `share_*` — swarm/cosmetics, другое). Бэкенд UClient (`/download/zip`, instance-share API) чужой — замена: **GitHub Pack Transport** (`github_pack/*` уже в tuffbox-core, `docs/15-github-pack-transport.md`) или TuffSwarm. Плюс сверить `exportInstance.ts` (372) с `exporter.rs`.

### 3.4 ★★ Усиление своего кода по эталонам (3–4 дня суммарно)

| Эталон | Что брать | Куда |
|---|---|---|
| `importModpack.ts` (1424) | **ZIP64 Central Directory reader** (`:110`), path-traversal защита (`:406`), CF-каталог-скрейп (`:1002`) + forgecdn fallback (`:995`) | `crates/tuffbox-core/src/importer.rs` — сверить глубину защит; `extractArchive` через внешний `tar` (`:276`) — **не повторять** (точка отказа на Windows) |
| `deepLink.ts` (462) | Действия `install\|import-instance\|join-group\|launch`, строгая валидация payload, краевые случаи | `apps/tuffbox-desktop/src-tauri/src/deep_link.rs` |
| `modDependencies.ts` (811) | Батчи `/v2/projects?ids=` **по 80**, кэш установленных project-id (`.uclient-cache.json`), поведение «конфликт не ставит, пишет `mod_conflicts` без force» | Сравнить с `resolver.rs`/`graph.rs`/`resolve.rs` — UClient проще, но эти оптимизации переносимы |
| `modrinthDownload.ts` (262) | Stall-timeout 60 с, `.part`+sha1, лимит 4 параллельных загрузки через прокси (IP-ограничение) | `download_engine.rs` — сверить |
| `console.ts` (584) | **Только `classifyLogLine`** — классификация строк лога (Log4j-уровни, i18n-ключи, мультиязычные regex вкл. kz/tt/чеченский); рендер через `innerHTML` — нет | В свой логгер (`errorReport.ts`/`launchFailure.ts`) |

### 3.5 ★ Фрагменты `launcher.ts` (по частям)

- `detectJavaRuntimes` (3542–3661) — детект Java через PATH + реестр Windows — сверить с `jre.rs`;
- enrichment инстанса `readZipEntry` (3672) + `scanFolder`/`enrichWithModrinth` (3775–3948) — скан jar для связей;
- NBT `servers.dat` (4850) — сверить с `servers_dat.rs` (у TuffBox уже есть, включая пинг);
- семантика краш-репорта (4362) — у TuffBox `crash.rs` сильнее, только посмотреть.

### 3.6 ★ `game-relay.ts` (774) — LAN-share через VPS (идея, если нужен фича)

LAN-порт из `latest.log` → TCP-туннель (`HOST/DATA/CONNECT`) к своему VPS + `relayFetch`. Протокол чистый TCP, портируемый, но **свой endpoint обязателен** (`uprojects.site:25570` — чужой).

### 3.7 ★ Идеи skinviewengine (движок в TuffBox заменён)

В `tuffbox-desktop` уже `skinview3d` (MIT). Дельта, если понадобится больше:
- **Готовый легальный источник:** `U:\TuffBox\milida-new\launcher-main\src\vendor\mine3d\` — тот же движок, `LICENSE` = MIT, «Copyright (c) 2026 Undefined Studio», файлы `core/{scene-loop, skin-animations, product-visuals, pixel-particles, bendable-skeleton, skin-outer-voxel, camera-framing, studio-postfx, locator-color}...` — переиспользование внутри TuffBox, а не копирование из GPL-дерева;
- дельта: `product-visuals.ts` (581 — студийный свет/пол/тени, IBL), `pixel-particles.ts` (454), `bendable-skeleton.ts` (414), `studio-postfx.ts` (Bloom+SMAA), `camera-framing.ts`;
- **НЕ переносить до выяснения лицензий:** `emotes/*` (конвертированы из SPEmotes_3.0, у Emotecraft своя лицензия, нигде не указана), `skin-outer-voxel.ts` (порт Java-мода 3D Skin Layers tr7zw, лицензия не указана).

### 3.8 Либо не брать вообще

`main.ts`, `messenger-api.ts` + `messenger/ui.ts` (6 223 — чужой бэкенд SSE), `ai-action-bridge.ts`, `installer/*.py` (заменяется Tauri-bundler/updater — см. референс 1, §3.1), `scripts/*`, Electron-специфика (WebContentsView-окно мира `main.ts:151–293`, схема `app://`, ANGLE-свитчи, single-instance, `app://` WASM), `styles.css`/темы (у TuffBox 19 тем + WCAG-тесты), локали `kk/tt/uk/kbd` (неполные, с мусором), `locales` как файлы (у TuffBox свой `dict` в `i18n.ts`; ru/en — только как источник формулировок).

---

## 4. Привязка к бэкенду UClient — непереносимо

Все URL — `src/shared/apiBase.ts`:

- каталог `catalog/file` (Modrinth+CF), `catalog/fingerprints`, image-proxy, skin/profile;
- `/api/messenger/*` — messenger-api.ts;
- `/api/ai/status` + `/api/ai/chat` (прокси Timeweb, MCP-тулы на клиенте) — `main.ts:851, 867`;
- `/api/relay` + VPS `uprojects.site:25570` — game-relay.ts;
- instance-share (`/download/zip`), `release/latest` (зеркало автообновления);
- `mineserv.top` скрейпится как каталог серверов (`launcher.ts:1064 parseMineServRows`);
- **Ely.by OAuth**: client_id `uclient`, secret захардкожен (`launcher.ts:96`), redirect `http://127.0.0.1:29123/oauth2/ely` — нельзя переносить;
- **Discord RPC**: CLIENT_ID `1532393186591375390` (`launcher.ts:~150`) — их приложение.

**`eml-lib 2.6` непереносим:** резолв/запуск, патчится в рантайме; производный код — GPL. У TuffBox пайплайн свой.

---

## 5. Лицензии

- **UClient: GPL-3.0-only** (`package.json:5`) + доп. условия: CLA (`Signed-off-by`), запрет снятия уведомлений об авторстве, имя «UClient»/логотипы — только с письменного согласия, ребазинг форков в 30 дней (`CONTRIBUTING.md`).
- **TuffBox: MIT OR Apache-2.0** (`LICENSE.md`).
- **Следствие (то же, что в `docs/18`):** прямое копирование любого кода UClient в TuffBox невозможно. Варианты: **(а) чистый рерайт** по идеям/алгоритмам (идеи, математика, UX-паттерны не охраняются) — рекомендуется; (б) письменное разрешение Undefined Studio; (в) GPL-часть отдельным модулем — на практике нежелательно.
- **Исключения:** `skinviewengine/README.md:16` заявляет MIT (репо `studioberry-hub/mine3d-embedded`), **но LICENSE-файла в папке нет** и в `package.json` поля `license` нет; канонический источник расходится (README vs `github:undefined-studio/mine3d-embedded`) — уточнить. Работающий прецедент: `src/vendor/mine3d/LICENSE` в Millida-репо (MIT, Undefined Studio) — использовать его как источник.
- **Риски без лицензий:** `emotes/*` (SPEmotes/Emotecraft — GPL-мод), `skin-outer-voxel.ts` (3D Skin Layers) — не переносить до выяснения.
- **3D-мир по лицензии чист:** `minecraft-renderer` MIT, обвязка UClient GPL → рерайт.

---

## 6. Неочеводные находки (краевые случаи)

1. **Forge 1.12.2 special-case в `src/main/` НЕ существует** — фикс живёт внутри eml-lib (CHANGELOG: обход отсутствия `meta.json`, установка через `installer.jar`, префикс версий `{mc}-`). При внимании к старому Forge TuffBox придётся реализовывать это самому.
2. **Санация tool-call орфанов** — `main.ts:901–935` (см. §3.2) — ломает OpenAI-совместимые API молча.
3. **Лимит прокси: ровно 4 параллельные загрузки на IP**, нарушение → 503/backoff (`PROXY_MAX_CONCURRENT_DOWNLOADS=4`).
4. **CF-проекты = id вида `cf:12345`** во всём коде — при маппинге на свои типы не забыть префикс.
5. **Просмотр мира:** read 1.13–26.2, mesher до 1.21.6; 1.13–1.17 — два layout'а колонок (spanning/no-span), оба реализованы; поддержка `.mcc`.
6. **Две копии three.js** в одном приложении (0.156 у skinviewengine, 0.184 изолированно у мира) — в Vite/Tauri воспроизводить через alias/define/optimizeDeps либо выносить 3D-мир в отдельный webview.
7. **Бандлы лежат в исходниках** (`npm run build` пишет esbuild'ом прямо в `src/renderer/app.js`) — смотреть бессмысленно, это выводы TS.
8. **Single-instance не только ради фокуса:** второй экземпляр держал бы лок профиля Chromium → `localStorage` блокировался бы ~6 с (`main.ts:473`).
9. **Состояние UClient (для проектирования своего):** `builds.json`, `accounts.json`, `active-account.json`, `running-game.json`, `game-relay-share.json`, `manifest-cache`, `active-cosmetics/meta.json` (с миграцией `migrateCapeMetaIfNeeded` 2126), `.uclient-cache.json` в инстансе; инстансы в `%APPDATA%\.uclient\<id>`, данные — `%APPDATA%\.Undefined Client\`.
10. **Косметика «тихая»:** `neutralizeConflictingCapeMods` (2499) выключает конфликтующие cape-моды, CSL-кэш копируется в инстанс (2553), Mojang cape-id из локального (2093).
11. **Консоль-окно держит 2000 строк истории на стороне main** (`main.ts:95–103`) и переигрывает при открытии — лог нельзя терять при закрытии окна.
12. **Автообновление двухуровневое:** зеркало `release/latest` → fallback GitHub `studioberry-hub/client`, asset `latest-windows-amd64.zip`, применение — отдельный `updater.exe`, убивающий клиент (`installer.py kill_client/do_updater`).
13. **AI-сессии в localStorage** с префиксом «Undefined Client-ai-sessions» — хранение перевести на свой стор.

---

## 7. Порядок работ (быстрый выигрыш)

1. Рерайт `src/shared/world-preview-matrix.ts` (135) → `src/lib/worldPreviewMatrix.ts` + тесты — дёшево, сразу даёт бейджи «просмотр / нужен экспорт / не поддерживается» для 1.13…26.2.
2. Рерайт `ai/confirm-ui.ts` + `turn-ui.ts` + `types.ts` → Svelte-компоненты confirm/undo/plan/diff для AI-сайдбаров — единственный UX-паттерн, которого у TuffBox нет.
3. Сверка `murmur2.rs` ↔ `curseforgeFingerprint.ts` и `download_engine.rs` ↔ `modrinthDownload.ts` — быстрые тесты на эталонных файлах.
4. Решение по 3D-миру: (а) рерайт `anvil.ts`/`worldViewer.ts` на Rust + `worldlib`-математика, рендер через `minecraft-renderer` (MIT) отдельным entry; (б) отказ в пользу 2D-карты. От выбора зависит объём (1–2 недели vs 0).
5. Усиление `importer.rs` (ZIP64, traversal-эталон) и `deep_link.rs` (валидация payload) по эталонам.
6. Шаринг инстансов: формат из `instanceShare.ts` + свой хостинг (GitHub Pack Transport / TuffSwarm).
7. Лицензионный чек-поинт: фиксация «всё — независимый рерайт»; `emotes/*` и `skin-outer-voxel.ts` — под запрет до выяснения.

---

## 8. Сводная таблица приоритетов

| # | Что берём | Файл-источник | Объём | Трудоёмкость | Ценность |
|---|---|---|---|---|---|
| 1 | AI confirm/undo/diff UI | `ai/confirm-ui.ts` + `turn-ui.ts` + `types.ts` + `attach-ui.ts` | ~1 900 | 3–5 дней | ★★★ (нет паттерна) |
| 2 | 3D-просмотр мира (матрица версий) | `shared/world-preview-matrix.ts` | 135 | 0.5–1 дня | ★★★ (дёшево) |
| 3 | 3D-просмотр мира (парсер+вьюер) | `anvil.ts`, `worldViewer.ts`, `clientJarAssets.ts`, `worldlib/*` | ~2 100 | 1–2 недели | ★★★ (новая фича) |
| 4 | Схемы ~100 AI-инструментов | `ai-tools.ts`, `ai-tools-extended.ts` | схемы | 2–3 дня на маппинг | ★★★ (готовое ТЗ) |
| 5 | Шаринг инстансов | `instanceShare.ts`, `exportInstance.ts` | ~1 060 + бэкенд | 1 неделя + решение | ★★ (гэп фичи) |
| 6 | Усиление импорта (ZIP64/traversal/CF) | `importModpack.ts` (избирательно) | ~600 | 3–4 дня | ★★ |
| 7 | Валидация deep-link | `deepLink.ts` | 462 | 2–3 дня | ★★ |
| 8 | Батчи/кэш резолвера | `modDependencies.ts` | 811 | 2–3 дня (сверка) | ★★ |
| 9 | Stall-timeout/лимиты скачивания | `modrinthDownload.ts` | 262 | 1 день (сверка) | ★★ |
| 10 | Классификация строк лога | `console.ts` (одна функция) | ~200 | 1 день | ★★ |
| 11 | LAN-relay через свой VPS | `game-relay.ts` | 774 | 1 неделя + endpoint | ★ (по потребности) |
| 12 | Фрагменты launcher.ts (Java-детект, enrichment) | `launcher.ts:3542–3948, 4850` | по частям | по необходимости | ★ |
| 13 | Дельта skinviewengine | из `src/vendor/mine3d` (MIT), не из GPL-дерева | 5 файлов | по необходимости | ★ |
| ✗ | Запуск, мессенджер, Electron, Python-installer, темы, локали кроме ru/en | — | — | — | не брать |

---

*Отчёт собран прямым чтением кода (2 агента-исследователя: `src/main/` + рендерер/skinviewengine) с точечной верификацией; `repomix-output.xml` — только grep по внутренностям eml-lib; билд-бандлы исключены. Обновлять при изменении UClient или закрытии гэпов TuffBox.*
