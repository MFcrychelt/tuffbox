# 06. Роль ИИ в TuffBox

## Главный принцип

ИИ не является dependency resolver. ИИ не должен сам решать, какие версии модов ставить, удалять или обновлять.

ИИ — это помощник для анализа логов, объяснений и генерации гипотез. Исполняет изменения только детерминированный код лаунчера после подтверждения пользователя.

## Dual-mode диагностика крашей

Настройка `ai.diagnoseMode` (Settings → Crash KB):

| Режим | Поведение |
|-------|-----------|
| **`server`** (default) | `POST /v1/crash/diagnose` на `ai.crashKbEndpoint` — сервер матчит приватную KB ± LLM, клиенту только `ActionPlan`. Без endpoint → локальный LLM (как раньше). |
| **`local`** | `POST /v1/crash/lookup` → top-N similar cases в prompt → Ollama / openai-compatible → тот же `ActionPlan`. |
| **`kb_only`** | Только matched case → plan из `actions` кейса, без LLM. |

Приватный корпус KB **никогда не шипится** в лаунчер. Builtin seed в клиенте — тонкий offline fallback.

```text
Crash logs
→ local fingerprint + inventory
→ DiagnoseMode router
→ ActionPlan JSON (единый контракт)
→ validate → UI confirm → snapshot → apply
```

API (ваш сервер):

- `POST /v1/crash/lookup` — fingerprint → hits (`solution`, `actions`, score); без `notes` / полного корпуса
- `POST /v1/crash/diagnose` — context → готовый `ActionPlan`
- Auth: bearer token (`crash_kb` в keyring); **нет** bulk dump KB

## Что делает код

- граф зависимостей, metadata, версии, loader/side checks;
- snapshots, apply, rollback, export, launch;
- парсинг логов / stacktrace / fingerprint;
- валидация и применение `ActionPlan`.

## Что делает ИИ

- объяснение stacktrace;
- гипотезы и ранжирование подозрительных модов;
- предложение **структурированного** плана (`ActionPlan`);
- помощь с config values (через `edit_config`).

## Что ИИ не должен делать напрямую

- молча удалять / обновлять моды;
- менять loader;
- переписывать configs без diff / confirm;
- применять fixes без snapshot;
- принимать окончательное решение вместо пользователя.

## System prompt (канон)

Текст живёт в `tuffbox_core::action_plan::ACTION_PLAN_SYSTEM_PROMPT` — один и тот же для server и local.

**AI Decision making** (порядок рассуждения перед JSON):

1. **Understand the context** — только shared info из промпта (MC/loader/Java, inventory, culprits, findings, KB, graph, excerpts).
2. **Isolate the problem** — одна primary root cause; ранний hard failure важнее cascading noise.
3. **Accept the risk** — у каждого action явный `risk`; `needsUserReview` / `confidence` честные.
4. **Map decision** — минимальный набор `actions` с `op`, reason ↔ isolated cause.

```text
You are TuffBox Crash Planner. You only output ONE JSON object matching schemaVersion 1.
You do NOT apply fixes. You propose an ActionPlan for the launcher.
… (см. код: decision steps + hard rules)
```
## Формат ответа ИИ — ActionPlan

Единственный executable контракт (`schemaVersion: 1`):

```json
{
  "schemaVersion": 1,
  "humanExplanation": "Игра вылетела во время инициализации рендера. Вероятно, конфликтуют Oculus и Embeddium.",
  "confidence": 0.78,
  "suspectedMods": ["oculus", "embeddium"],
  "needsUserReview": true,
  "source": "hybrid",
  "matchedCaseIds": ["case-oculus-embeddium"],
  "actions": [
    {
      "op": "update_mod",
      "modId": "oculus",
      "version": "1.7.0",
      "reason": "Версия 1.6.x часто конфликтует с текущим Embeddium.",
      "risk": "medium"
    },
    {
      "op": "install_mod",
      "modId": "indium",
      "provider": "modrinth",
      "reason": "Missing dependency for Sodium",
      "risk": "low"
    },
    {
      "op": "edit_config",
      "path": "config/example.toml",
      "patchType": "toml_set",
      "patch": { "section.key": "value" },
      "reason": "Disable conflicting feature",
      "risk": "low"
    }
  ]
}
```

Допустимые `op`: `install_mod`, `remove_mod`, `disable_mod`, `update_mod`, `change_mod_version`, `reinstall_mod`, `edit_config`.

Legacy `recommended_actions` / `action_type` ещё парсятся и нормализуются в `op`.

## Валидация и apply

```text
ActionPlan
→ validate_action_plan (unknown op = reject)
→ UI diff / review
→ user confirms
→ snapshot
→ apply_action_plan → FixAction / ChangeAction / edit_config patch
→ test run
```

`edit_config.patchType`: `json_merge` | `toml_set` | `properties_set` | `replace_file`.

## Как расширять приватную Crash KB (авторский workflow)

### В IDE (удобно)

1. Открой Diagnostics → выбери crash-report.
2. (Опционально) **AI explain** / **Apply plan** — чтобы подтянуть solution и actions.
3. Нажми **Save KB case** (или «Save as KB case» под AI).
4. Заполни **Solution**, поправь **Actions JSON**, при желании **Notes** (только для тебя).
5. **Save case** → пишется в `.tuffbox/crash_kb/cases.jsonl` (`source: authored`) и публичный файл в `.tuffbox/crash_kb/export/<id>.json` **без notes**.
6. **Copy export JSON** / **Open export folder** — залей export на свой сервер KB.

Fingerprint подставляется автоматически из текущего краша.

### Вручную / на сервере

1. Кейсы храните **на сервере** (JSONL/DB), не в релизе лаунчера.
2. Каждый кейс минимум:
   - `id`, fingerprint / matchRules
   - `solution` (текст для UI/RAG)
   - `actions[]` уже в executable `op`-формате
   - `notes` / внутренние правила — **не отдавать** клиенту
3. Сильный матч → `kb_only` / diagnose без LLM.
4. Opt-in «Helped» пишет в локальный project JSONL; авторский кейс — через форму выше.

Пример кейса (как в export):

```json
{
  "id": "mixin-create-flywheel",
  "fingerprint": { "exception": "MixinTransformerError", "key": "..." },
  "solution": "Update Create and Flywheel to matching versions for this MC.",
  "actions": [
    { "op": "update_mod", "modId": "create", "version": null, "reason": "Latest compatible", "risk": "medium" }
  ]
}
```

## Network evolution (TuffSwarm)

Локальный AI и ActionPlan — **foundation**. Поверх них развивается сеть **TuffSwarm** (канон: [`13-tuffswarm-network.md`](13-tuffswarm-network.md)):

| Контур | Роль относительно этого документа |
|--------|-----------------------------------|
| Knowledge Torrent | Обмен `ExperienceCapsule` (= fingerprint + solution + `actions[]` ActionPlan), не raw logs |
| Creation Marketplace | Чужие GPU генерируют артефакты; заказчик верифицирует; награда = Kudos / premium |

**Инварианты не меняются:**

- ИИ / swarm **MUST NOT** применять фиксы без confirm + snapshot.
- Словарь действий — только executable `op` из ActionPlan (`schemaVersion: 1`).
- Author `notes` **MUST NOT** уходить в сеть.
- Dual-mode diagnose (`server` / `local` / `kb_only`) остаётся Phase B (HTTP remote KB); P2P — Phase C+.

**Два AI-контура (не смешивать):**

| Контур | Роль |
|--------|------|
| **AI Explain** | Диагностика текущего краша; может **читать** сеть (lookup/diagnose); **не** публикует капсулы |
| **Resolution Distill** | После verified fix: сжать историю действий пользователя → показать план → **Confirm/Edit** → только тогда publish |

Authored export из Diagnostics («Save KB case») — прямой вход в capsule format для будущей сети.
