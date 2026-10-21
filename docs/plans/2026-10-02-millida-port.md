# План: перенос из Millida Launcher (по отчёту docs/18-millida-launcher-reference.md)

Дата: 2026-10-02. Источник отчёта: `docs/18-millida-launcher-reference.md`.
Соседний план (референс 2, UClient): [`2026-10-02-uclient-port.md`](2026-10-02-uclient-port.md).
Целевой репо: `U:/TuffBox/tuffbox` (MIT OR Apache-2.0).
Статус: **трек A реализован** (2026-10-02), треки B–D не начаты.

---

## Статус трека A (реализовано)

| # | Что сделано | Файлы |
|---|---|---|
| A1 | Единый валидатор недоверенных путей: `validate_rel_path`/`is_safe_rel_path`/`safe_join`, отбой absolute/`..`/`:`/control/зарезервированных имён Windows/trailing dot-space. На него переведены `github_pack::safe_relative_path`, `importer::is_unsafe_zip_rel_path` (оставлен публичной обёрткой — его зовёт src-tauri), `extract_curseforge_overrides`, а также три рукописных guard'а в src-tauri: `sanitize_drop_rel`, `extract_mrpack_overrides`, `extract_prism_zip_content` (у двух последних дыра: `rel.contains("..")` пропускал `//etc/x` и `C:/x`). | `crates/tuffbox-core/src/safepath.rs` (new), `importer.rs`, `github_pack/import.rs`, `apps/tuffbox-desktop/src-tauri/src/lib.rs` |
| A2 | Санитайзер JVM-флагов: `sanitize_jvm_args` (отбой `-javaagent`/`-agentlib`/`-Xbootclasspath`/`-cp`/`-jar`/`-Dlog4j*.configuration*`/`-Djava.system.class.loader`/`-Dlog4j2.formatMsgNoLookups`/не-опций/control), применяется в `build_command`, отброшенное логируется в прогресс. | `crates/tuffbox-core/src/launch_args.rs` (new), `launcher.rs` |
| A3 | Log4Shell: `-Dlog4j2.formatMsgNoLookups=true` на каждом запуске + для MC < 1.17 (log4j2 < 2.10, где флаг не существует) hardened-конфиг `log4j2-hardened.xml` (`strict="true"`, `%msg{nolookups}`), добавляемый **после** `-Dlog4j.configurationFile` из version json (JVM берёт последнее значение). | `launch_args.rs`, `launcher.rs` |
| A4 | Windows `@argfile`: при длине командной строки > 30000 (лимит `CreateProcess` 32767) аргументы пишутся в `logs/tuffbox-args.txt` и запуск идёт как `java @file`. Кодировка — **системная ANSI** (`WideCharToMultiByte(CP_ACP)` через сырой FFI, без новой зависимости): JDK читает argfile в системной кодировке, а не UTF-8 (JDK-8286998), иначе кириллический путь в classpath превращается в мусор. Бэкслеши экранируются, аргументы с пробелами/кавычками/`#` квотируются. | `launcher.rs` |

Отклонения от исходного текста плана: (1) A1 — не «заменить `is_unsafe_zip_rel_path`», а свести всё на один модуль, сохранив публичные имена (их использует desktop-крейт); (2) A3 — конфиг вместо override log4j-библиотеки в classpath, потолок помечен `ponytail:`-комментарием в `needs_log4j_config`; (3) A4 — ANSI-кодировка через собственный FFI-биндинг вместо добавления `windows`-крейта.

Проверки: `cargo test -p tuffbox-core --tests` — 620 + 5 + 7 + 7 passed, 0 failed (кроме двух **предсуществующих** падений в `crash_kb` и `fs_util`, не связанных с диффом и не трогавшихся файлов); `cargo check -p tuffbox-desktop --all-targets` — собирается; `cargo clippy -p tuffbox-core -p tuffbox-desktop --all-targets` — ни одного замечания в изменённых файлах; `graphify update .` — 12077 узлов / 31393 ребра.

Смоук A4 против настоящей JVM (Windows 10, ACP = **windows-1251**), одноразовые скрипты удалены:
- экранированный путь `"-Dtb.probe=C:\\...\\tb argfile probe"` → JVM вернула значение **байт-в-байт**;
- тот же путь без экранирования → `C:UsersadminAppDataLocalTemp<TAB>b argfile probe` (бэкслеши съедены) — экранирование обязательно, правило `argfile_line` подтверждено;
- каталог с кириллицей (`Иван`) в argfile, записанном в ACP: байты `... c8 e2 e0 ed` (CP1251, не UTF-8) → JVM прочитала путь точно. Выбор системной кодировки вместо UTF-8 подтверждён.

---

## 0. Сверка отчёта с кодом TuffBox (отчёт частично устарел)

Проверено в этом репо:

| Утверждение отчёта | Факт |
|---|---|
| «GC/память-тюнинг — gap (item 7)» | **Уже есть и подключено.** `crates/tuffbox-core/src/jvm_tuning.rs` (438 стр.), вызывается в `apps/tuffbox-desktop/src-tauri/src/lib.rs:15942–15989`; команды `recommend_heap_cmd`/`recommend_jvm_cmd` (`:18731`, `:18760`). См. `docs/sprint-jvm-tuning.md`. → **item 7 вычеркнуть.** |
| «Реестр задач установок — проверить (item 13)» | Эквивалента нет: grep `install_jobs|InstallRegistry|cancel_install|jobs::` по `src-tauri/src/lib.rs` — 0. Кандидат, но не приоритет. |
| «Пиннинг-тесты capabilities/CSP (item 11)» | Частично: `tauri.conf.json` уже `withGlobalTauri: false`, `csp.script-src: ["'self'"]`, `assetProtocol` scope широкий (`$HOME/**` и т.д.). Тестов-«гвоздей» нет. |
| Автообновление (item 1) | Подтверждено: `tauri-plugin-updater` в `src-tauri/Cargo.toml` **отсутствует**, в `tauri.conf.json` нет секции `plugins.updater`, `minisign`/`selfupdate` — 0 совпадений. Есть только `integrations::check_for_app_update` (`lib.rs:21851`). |
| `@argfile` (item 6) | Подтверждено: нет. `build_command` (`crates/tuffbox-core/src/launcher.rs:315`) кладёт classpath одним `cmd.arg(classpath)` (`:469`). |
| Log4Shell (item 3) | Подтверждено: `formatMsgNoLookups` — 0. `launcher.rs:455` ставит `-Dlog4j.configurationFile` из Mojang version json — не митигация. |
| Чёрный список JVM-флагов (item 4) | Подтверждено: `launcher.rs:429` `cmd.args(&options.jvm_args)` без фильтра. |
| Единый safe-path (item 5) | Частично: есть `importer.rs:584 is_unsafe_zip_rel_path` (zip-slip для архивов). Единой точки для путей из mrpack/manifest нет. |

Вывод: реальные gaps — (A) безопасность запуска, (B) автообновление/инсталлер, (C) импорт прочих лаунчеров, (D) дешёвое упрочнение.

---

## 1. Лицензия — решено

Millida = GPL-3.0-only, TuffBox = MIT OR Apache-2.0. Копирование строк Millida делает бинарь TuffBox GPL-3.0-only → несовместимо с текущей лицензией репо.

**Стратегия (а): чистый рерайт** по описанию алгоритмов из отчёта. Ни одной строки Millida не копировать. Все пункты ниже — независимые реализации по ТЗ, а не порт.

---

## 2. Треки, объём, порядок

### Трек A — Безопасность запуска (делаем первым)

Всё в `crates/tuffbox-core`, одна точка сборки команды (`TestLauncher::build_command`) → все вызовы (`launch_profile` → `build_and_spawn` в `src-tauri/src/lib.rs`, серверный запуск, CLI) чинятся одним диффом.

| # | Что | Где | LOC (impl+тесты) |
|---|---|---|---|
| A1 | `safepath.rs`: `safe_join`/`safe_child`/`safe_file_name` — отброс absolute, `..`, control-символов, зарезервированных имён Windows, trailing dot/space; проверка по канонизированному пути. Переиспользовать в `importer.rs` (заменить локальный `is_unsafe_zip_rel_path` на общий хелпер) и в местах, где путь приходит из mrpack/manifest/внешних данных. | новый `crates/tuffbox-core/src/safepath.rs`; правка `importer.rs:584` | ~150 + 110 |
| A2 | Санитайзер JVM-аргументов: чёрный список (`-javaagent`, `-agentpath`, `-Xbootclasspath*`, `-classpath`/`-cp`, `-jar`, `-Dlog4j.configurationFile`, `-Dlog4j2.configurationFile`, `-Djava.system.class.loader`, `-Dsun.boot.class.path`), whitelist-логика для профильных, лог отброшенных в `progress`. Применять внутри `build_command` к `options.jvm_args` — один guard, без правки вызывающих. | новый `crates/tuffbox-core/src/launch_args.rs`; вызов в `launcher.rs:429` | ~120 + 90 |
| A3 | Log4Shell-митигация: `-Dlog4j2.formatMsgNoLookups=true` первым JVM-аргументом всегда; для 1.7–1.11.2 — подмена конфига log4j (встроенный ресурс + `needs_log4j_config`/`ensure_log4j_config`). | `launcher.rs::build_command` + `assets/` ресурс | ~60 + 50 |
| A4 | `@argfile` для Windows: если собранная командная строка (classpath + args) > ~32000 символов — записать аргументы в temp argfile (ANSI/UTF-8) и запускать `java @argfile`; фолбэк-сообщение при переполнении. Только `cfg(windows)`, порог захардкожен с `ponytail:`-комментом (лимит CreateProcess 32767). | `launcher.rs` (`build_command`/`classpath_string`), новый хелпер `write_argfile` | ~110 + 40 |

**Итого трек A ≈ 730 LOC.** Файлы: `safepath.rs` (new), `launch_args.rs` (new), `launcher.rs`, `lib.rs` (core — 2 строки `pub mod`), `importer.rs`, 1 ресурс-ассет.

Проверки: `cargo test -p tuffbox-core` (новые `#[cfg(test)]`: safepath-кейсы `..`/absolute/Windows-имена; sanitizer пропускает валидные и режет опасные; argfile-порог на синтетическом classpath > 32767).
Smoke: реальный запуск Forge-сборки с большим classpath на Windows (наблюдать `java @…argfile` в логе), запуск с `-javaagent` в профильных аргументах → флаг отброшен и залогирован.

### Трек B — Автообновление + инсталлер (Stage 15), отдельно

**Не портировать 2402 строки Millida (`selfupdate.rs` 673 + `installer/` 1729).** Tauri 2 уже поставляет `tauri-plugin-updater`: minisign-проверка подписи, скачивание, установка NSIS/MSI/AppImage — ровно то, что Millida написала руками. Ленивый корректный путь:

- `src-tauri/Cargo.toml`: `tauri-plugin-updater`, `tauri-plugin-process`.
- `tauri.conf.json`: `plugins.updater` (pubkey, endpoints → `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`), `bundle.createUpdaterArtifacts: true`.
- `capabilities/default.json`: `updater:default`, `process:allow-restart`.
- `src-tauri/src/updater.rs`: тонкая обёртка (`check`/`install`/`restart`) ~80 LOC; UI-баннер обновления (Svelte 5) ~150 LOC.
- CI: генерация ключевой пары (приватный ключ в secrets), подпись артефактов, генерация `latest.json` на релизе — ~120 LOC YAML.

**Итого трек B ≈ 350–450 LOC + конфиг/CI/ключи.** Блокер: наличие GitHub Releases и приватного minisign-ключа. Не начинать, пока A не закрыт.

### Трек C — Импорт прочих лаунчеров (средний приоритет)

Есть: Prism/MultiMC (`importer.rs:1225`), CurseForge-паки (`:279`), packwiz (`importer/packwiz.rs`). Добавить: ATLauncher (`instance.json` + `configs/`), GDLauncher (`instance.json`), Modrinth App (`profiles/<name>/profile.json`), общий `.minecraft`/TLauncher. Ложится в существующие `import_instance_directory`/`detect_instance_meta` (`importer.rs:728`, `:855`).

**Итого ≈ 300 LOC + 150 тестов.** Правки только `importer.rs` (+ `importer/` при разрастании).

### Трек D — Дешёвое упрочнение (backlog)

| Что | Где | LOC |
|---|---|---|
| D1 Пиннинг-тесты capabilities/CSP (гвозди против регрессий) | `src-tauri/src/lib.rs` `#[cfg(test)]` | ~120 |
| D2 Проверка jar (fractureiser-класс): сверить с `mod_scan.rs` (507), добавить блоклист-хэшей | `crates/tuffbox-core/src/` | ~200 + тесты |
| D3 Реестр идущих установок (ключ → отмена/прогресс/антидубль) | `src-tauri/src/lib.rs` | ~200 + тесты |

---

## 3. Что НЕ берём

- **Краш-анализ/диагностика** — TuffBox `crash.rs` (5324) + AI-планы сильнее Millida `crashcause.rs`/`crashfix.rs`.
- **Граф/резолвер** — TuffBox `resolver.rs`+`graph.rs`+`resolve.rs` глубже Millida `deps.rs`.
- **JVM-тюнинг (item 7)** — уже реализован (`jvm_tuning.rs`).
- **Всё, привязанное к `api.millida.net`/`cdn.millida.net`** (~20 кусков из отчёта §4.2) — переносить некуда.
- **React-фронтенд Millida** — другой стек; только паттерны (ленивые экраны, two-phase модалки), не код.

---

## 4. Порядок работ

1. **A** (A1 → A2 → A3 → A4) — один PR на трек, все правки в `crates/tuffbox-core`, полный прогон `cargo test -p tuffbox-core` + `cargo clippy --all-targets -- -D warnings`.
2. `graphify update .` после трека (новые файлы иначе не в графе).
3. **B** — отдельный PR, только после решения по GitHub Releases/ключу.
4. **C**, затем **D** по остатку приоритета.

Правила репо: любые новые Tauri-команды с I/O — `async` + `spawn_blocking`; запуск внешних процессов — с дедлайном (`run_with_probe_timeout`).

---

## 5. Первый шаг следующей сессии

Начать с **A1**: создать `crates/tuffbox-core/src/safepath.rs`, зарегистрировать `pub mod safepath;` в `lib.rs`, перевести `importer.rs::is_unsafe_zip_rel_path` на общий хелпер, покрыть тестами `..`/absolute/Windows-зарезервированные имена.
