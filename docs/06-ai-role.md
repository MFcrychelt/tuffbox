# 06. Роль ИИ в TuffBox

## Главный принцип

ИИ не является dependency resolver. ИИ не должен сам решать, какие версии модов ставить, удалять или обновлять.

ИИ — это помощник для анализа логов, объяснений и генерации гипотез.

## Что делает код

Код отвечает за:

- построение графа зависимостей;
- чтение metadata модов;
- проверку версий;
- проверку loader compatibility;
- проверку side mismatch;
- создание snapshots;
- применение изменений;
- rollback;
- экспорт;
- запуск Minecraft;
- парсинг логов;
- извлечение stacktrace.

## Что делает ИИ

ИИ отвечает за:

- объяснение stacktrace человеческим языком;
- предположение причины краша;
- ранжирование подозрительных модов;
- предложение плана исправления;
- генерацию текста changelog;
- помощь с config values;
- объяснение предупреждений.

## Что ИИ не должен делать напрямую

- молча удалять моды;
- молча обновлять моды;
- менять loader;
- переписывать configs без diff;
- применять fixes без snapshot;
- принимать окончательное решение вместо пользователя.

## Безопасный AI pipeline

```text
Crash happens
→ TuffBox collects latest.log/crash-report
→ local parser extracts stacktrace
→ graph maps classes/packages to mod nodes
→ recent changes are attached
→ AI receives compact context
→ AI returns structured explanation and proposed plan
→ resolver validates plan
→ UI shows diff
→ user confirms
→ snapshot is created
→ deterministic actions are applied
→ test run starts
```

## Формат ответа ИИ

ИИ должен возвращать JSON, а не свободный текст:

```json
{
  "humanExplanation": "Игра вылетела во время инициализации рендера. Вероятно, конфликтуют Oculus и Embeddium.",
  "confidence": 0.78,
  "suspectedNodes": ["oculus", "embeddium"],
  "recommendedPlan": [
    {
      "type": "update_mod",
      "modId": "oculus",
      "targetVersion": "1.7.0",
      "reason": "Версия 1.6.x часто конфликтует с текущей версией Embeddium."
    }
  ],
  "needsUserReview": true
}
```

## Валидация AI-плана

Перед применением AI-план должен пройти через ResolverService:

```text
AI plan
→ normalize actions
→ check availability
→ check version constraints
→ check conflicts
→ create change plan
→ show diff
```
