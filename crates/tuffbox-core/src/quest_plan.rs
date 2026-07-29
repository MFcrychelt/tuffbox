//! QuestPlan — executable contract for AI-authored FTB Quests.
//!
//! Same dual-mode idea as ActionPlan: the model only emits JSON; the launcher
//! parses, validates, merges into QuestBook, then the user confirms Save → SNBT.
//!
//! # ponytail
//! Declarative `chapters[]` only (no micro-ops). Add ops later if edit-diff
//! generation proves useful.

use crate::unified::quest_book::{
    Chapter, ChapterGroup, Quest, QuestBook, QuestValidationError, Reward, Task,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const QUEST_PLAN_SCHEMA_VERSION: u32 = 1;

/// System prompt for quest-authoring models (local or server).
pub const QUEST_PLAN_SYSTEM_PROMPT: &str = r#"You are TuffBox Quest Planner. You only output ONE JSON object matching QuestPlan schemaVersion 1.
You do NOT write SNBT files and you do NOT apply changes. The launcher merges your plan into the FTB Quests book after the user confirms.

AI Decision making (follow in order before emitting JSON):
1. Understand the context — pack theme, available mods/items, existing chapters if provided.
2. Isolate the goal — one coherent chapter (or a small set) that teaches a progression beat.
3. Accept the risk — mark needsUserReview true when item ids or deps are uncertain; keep confidence honest.
4. Map decision — emit chapters/quests/tasks/rewards the launcher can merge; prefer concrete item ids (mod:path).

Hard rules:
1. Output ONLY valid JSON (no markdown outside optional ```json fences the parser strips).
2. schemaVersion must be 1.
3. Prefer 16-char uppercase hex ids for chapter/quest/task/reward ids. If omitted, the launcher generates them.
4. dependencies may be quest ids OR exact quest titles (launcher resolves titles within the plan + existing book). Dependencies may also target task ids.
5. Every quest MUST have at least one task. Default checkmark is fine for narrative unlocks.
6. Prefer ≥2 description lines (lore). Empty descriptions are warned; multi-pass lore fills them.
7. item tasks/rewards: properties.item is a string "mod:id" (or a small NBT object with id). Do not invent mods not in context.
8. Coordinates x/y are FTB quest-space floats (not pixels). Space nodes ~2–3 units apart; launcher can auto-layout.
9. Do not invent cyclic dependencies.

JSON shape:
{
  "schemaVersion": 1,
  "humanExplanation": "string",
  "confidence": 0.0-1.0,
  "needsUserReview": true,
  "source": "ai",
  "chapterGroups": [{ "id": "HEX", "title": "Group name" }],
  "chapters": [{
    "id": "HEX16",
    "title": "Chapter title",
    "icon": "mod:item",
    "group": "group id or empty",
    "orderIndex": 0,
    "mode": "upsert",
    "quests": [{
      "id": "HEX16",
      "title": "Quest title",
      "subtitle": null,
      "description": ["line"],
      "x": 0.0,
      "y": 0.0,
      "icon": "mod:item",
      "dependencies": [],
      "optional": false,
      "shape": "circle",
      "size": 1.0,
      "tasks": [{ "id": "HEX", "type": "item", "title": null, "properties": { "item": "minecraft:cobblestone", "count": 1 } }],
      "rewards": [{ "id": "HEX", "type": "xp", "properties": { "xp": 10 } }]
    }]
  }]
}

Allowed task types: item, checkmark, kill, dimension, biome, xp, advancement, stat, stage, fluid, location, observation, structure, custom.
Allowed reward types: item, xp, xp_levels, command, random, choice, stage, toast, custom.
Chapter mode: "upsert" (default, merge by id) or "replace" (replace quests list for that chapter id).

When the user writes Russian/casual names, map to vanilla ids, e.g.:
дерево/дрова/брёвна → minecraft:oak_log; булыга/булыжник → minecraft:cobblestone;
палки → minecraft:stick; камень → minecraft:stone; уголь → minecraft:coal;
железо → minecraft:iron_ingot; золото → minecraft:gold_ingot; алмаз → minecraft:diamond;
доски → minecraft:oak_planks; верстак → minecraft:crafting_table; печка → minecraft:furnace.
Counts like "10 дерева" → properties.count = 10.
One numbered list item = one quest; commas inside the item = multiple tasks on that quest.
"#;

/// Context injected into the user message for the quest author model.
#[derive(Debug, Clone, Default)]
pub struct QuestAuthorContext {
    pub existing_chapters: Vec<String>,
    pub sample_items: Vec<String>,
    pub pack_hint: Option<String>,
}

/// Build the user-turn message for quest authoring (system prompt is separate).
pub fn build_quest_author_user_message(request: &str, ctx: &QuestAuthorContext) -> String {
    let mut p = String::new();
    p.push_str("User request (natural language — invent a QuestPlan that matches it):\n");
    p.push_str(request.trim());
    p.push('\n');
    if let Some(hint) = &ctx.pack_hint {
        p.push_str("\nPack / theme: ");
        p.push_str(hint);
        p.push('\n');
    }
    if !ctx.existing_chapters.is_empty() {
        p.push_str("\nExisting chapters (do not duplicate titles unless asked to extend):\n");
        for t in ctx.existing_chapters.iter().take(40) {
            p.push_str("- ");
            p.push_str(t);
            p.push('\n');
        }
    }
    if !ctx.sample_items.is_empty() {
        p.push_str("\nPrefer item ids from this pack catalog when possible:\n");
        for id in ctx.sample_items.iter().take(80) {
            p.push_str("- ");
            p.push_str(id);
            p.push('\n');
        }
    }
    p.push_str("\nRespond with ONLY the QuestPlan JSON object.\n");
    p
}

/// Offline parse for simple “глава … квесты 1. … — награда …” prompts.
/// Returns None when the request is too free-form (caller should use the LLM).
///
/// # ponytail
/// Covers the happy-path NL the product demo uses; escalate to LLM for everything else.
pub fn try_heuristic_quest_plan(request: &str) -> Option<QuestPlan> {
    let text = request.trim();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    let looks_like_list = lower.contains("квест")
        || lower.contains("quest")
        || text.chars().any(|c| c == '1') && (text.contains('.') || text.contains(')'));
    if !looks_like_list {
        return None;
    }

    let chapter_title = extract_chapter_title(text).unwrap_or_else(|| "New Chapter".into());

    // Body after "квест" / "quest" / first numbered item
    let body = if let Some(idx) = lower.find("квест") {
        &text[idx..]
    } else if let Some(idx) = lower.find("quest") {
        &text[idx..]
    } else {
        text
    };

    let quest_chunks = split_numbered_items(body);
    if quest_chunks.is_empty() {
        return None;
    }

    let mut quests = Vec::new();
    let mut prev_title: Option<String> = None;
    for (i, chunk) in quest_chunks.iter().enumerate() {
        let (task_part, reward_part) = split_reward(chunk);
        let tasks = parse_gather_clauses(task_part);
        if tasks.is_empty() {
            continue;
        }
        let rewards = reward_part
            .map(parse_reward_clause)
            .unwrap_or_default();
        let title = quest_title_from_tasks(&tasks, i);
        let mut deps = Vec::new();
        if let Some(p) = &prev_title {
            deps.push(p.clone());
        }
        let icon = tasks
            .first()
            .and_then(|t| t.properties.get("item"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        quests.push(QuestPlanQuest {
            id: None,
            title: title.clone(),
            subtitle: None,
            description: vec![chunk.trim().to_string()],
            x: (i as f64) * 2.5,
            y: 0.0,
            icon,
            dependencies: deps,
            tasks,
            rewards,
            optional: false,
            shape: Some("circle".into()),
            size: Some(1.0),
        });
        prev_title = Some(title);
    }
    if quests.is_empty() {
        return None;
    }

    let icon = quests
        .first()
        .and_then(|q| q.icon.clone())
        .or_else(|| Some("minecraft:oak_log".into()));

    Some(QuestPlan {
        schema_version: QUEST_PLAN_SCHEMA_VERSION,
        human_explanation: format!("Heuristic draft from: {}", truncate(text, 120)),
        confidence: 0.72,
        needs_user_review: true,
        source: Some("heuristic".into()),
        chapter_groups: vec![],
        reward_tables: vec![],
        chapters: vec![QuestPlanChapter {
            id: None,
            title: chapter_title,
            icon,
            group: None,
            order_index: Some(0),
            mode: Some("upsert".into()),
            quests,
        }],
    })
}

fn extract_chapter_title(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    // "главу 1: …" / "глава 1: …" / "chapter 1: …"
    for (key, prefix) in [("глав", "Глава"), ("chapter", "Chapter")] {
        if let Some(idx) = lower.find(key) {
            // skip past the whole word (глава/главу/главе…)
            let after_key = idx + key.len();
            let rest_start = text[after_key..]
                .find(|c: char| c.is_whitespace() || c.is_ascii_digit() || c == ':' || c == '-')
                .map(|d| after_key + d)
                .unwrap_or(after_key);
            let rest = text[rest_start..].trim_start();
            let rest_l = rest.to_lowercase();
            let end = rest
                .find([',', '.', ';'])
                .or_else(|| rest_l.find("в ней"))
                .or_else(|| rest_l.find("квест"))
                .or_else(|| rest_l.find("quest"))
                .unwrap_or(rest.len());
            let title = rest[..end].trim().trim_matches(|c| c == '-' || c == '–').trim();
            if title.is_empty() {
                continue;
            }
            if !title.to_lowercase().starts_with(&prefix.to_lowercase()) {
                return Some(format!("{prefix} {title}"));
            }
            return Some(title.to_string());
        }
    }
    None
}

fn split_numbered_items(body: &str) -> Vec<String> {
    // Find "1." / "1)" starts
    let mut starts = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'.' || bytes[j] == b')') {
                let at_boundary = i == 0 || bytes[i - 1].is_ascii_whitespace() || bytes[i - 1] == b'-';
                if at_boundary {
                    starts.push(i);
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    if starts.is_empty() {
        // Single blob after dash: "квесты - добудь..."
        let cleaned = body
            .trim_start_matches(|c: char| !c.is_alphabetic())
            .trim();
        if cleaned.len() > 8 {
            return vec![cleaned.to_string()];
        }
        return Vec::new();
    }
    let mut out = Vec::new();
    for (n, &s) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(body.len());
        let mut chunk = body[s..end].trim();
        // strip leading "1. "
        if let Some(pos) = chunk.find(|c| c == '.' || c == ')') {
            chunk = chunk[pos + 1..].trim();
        }
        if !chunk.is_empty() {
            out.push(chunk.to_string());
        }
    }
    out
}

fn split_reward(chunk: &str) -> (&str, Option<&str>) {
    let lower = chunk.to_lowercase();
    let key_at = lower
        .find("наград")
        .or_else(|| lower.find("reward"));
    if let Some(idx) = key_at {
        let before = chunk[..idx]
            .trim()
            .trim_end_matches(['-', '–', '—', ':', ' '])
            .trim();
        // Drop the keyword token, keep the remainder ("10 палок").
        let after_kw = chunk[idx..]
            .split_once(char::is_whitespace)
            .map(|(_, rest)| rest.trim())
            .unwrap_or("")
            .trim_start_matches([':', '-', '–', '—'])
            .trim();
        if !after_kw.is_empty() {
            return (before, Some(after_kw));
        }
    }
    if let Some(idx) = chunk.rfind(['-', '–', '—']) {
        let after = chunk[idx + after_dash_len(chunk, idx)..].trim();
        if looks_like_item_amount(after) {
            return (chunk[..idx].trim(), Some(after));
        }
    }
    (chunk, None)
}

fn after_dash_len(s: &str, idx: usize) -> usize {
    s[idx..]
        .chars()
        .next()
        .map(|c| c.len_utf8())
        .unwrap_or(1)
}

fn looks_like_item_amount(s: &str) -> bool {
    let mut parts = s.split_whitespace();
    matches!(parts.next(), Some(p) if p.chars().all(|c| c.is_ascii_digit()))
        && parts.next().is_some()
}

fn parse_gather_clauses(s: &str) -> Vec<QuestPlanTask> {
    let mut tasks = Vec::new();
    for clause in split_clauses(s) {
        if let Some(task) = parse_one_gather(&clause) {
            tasks.push(task);
        }
    }
    tasks
}

fn split_clauses(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let lower_full = s.to_ascii_lowercase();
    // Split on comma; also on " и " when both sides look like gather phrases
    for (i, ch) in s.char_indices() {
        if ch == ',' || ch == ';' {
            let t = cur.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
            cur.clear();
            continue;
        }
        cur.push(ch);
        // detect " и " at end of cur
        let cl = cur.to_ascii_lowercase();
        if cl.ends_with(" и ") || cl.ends_with(" and ") {
            let cut = if cl.ends_with(" and ") { 5 } else { 3 };
            let left = cur[..cur.len() - cut].trim();
            if !left.is_empty() && looks_like_gather(left) {
                out.push(left.to_string());
                cur.clear();
            }
        }
        let _ = (i, &lower_full);
    }
    let t = cur.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    out
}

fn looks_like_gather(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    l.contains(char::is_numeric)
        || [
            "добы", "накоп", "собери", "принеси", "craft", "mine", "get", "gather", "kill",
        ]
        .iter()
        .any(|k| l.contains(k))
}

fn parse_one_gather(clause: &str) -> Option<QuestPlanTask> {
    let cleaned = clause
        .trim()
        .trim_matches(|c: char| c == '.' || c == '!' || c == ';' || c == ':');
    let re_parts: Vec<&str> = cleaned.split_whitespace().collect();
    if re_parts.is_empty() {
        return None;
    }
    let mut count: i64 = 1;
    let mut item_words = Vec::new();
    let mut seen_count = false;
    for w in &re_parts {
        let wl = w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
        if wl.is_empty() {
            continue;
        }
        if is_gather_verb(&wl) {
            continue;
        }
        if !seen_count && wl.chars().all(|c| c.is_ascii_digit()) {
            count = wl.parse().unwrap_or(1);
            seen_count = true;
            continue;
        }
        item_words.push(wl);
    }
    if item_words.is_empty() {
        return None;
    }
    let item_phrase = item_words.join(" ");
    let item_id = resolve_item_phrase(&item_phrase)?;
    let mut properties = HashMap::new();
    properties.insert("item".into(), Value::String(item_id));
    if count != 1 {
        properties.insert("count".into(), Value::from(count));
    }
    Some(QuestPlanTask {
        id: None,
        task_type: "item".into(),
        title: None,
        value: None,
        properties,
    })
}

fn is_gather_verb(w: &str) -> bool {
    [
        "добудь",
        "добыть",
        "накопай",
        "накопать",
        "собери",
        "собрать",
        "принеси",
        "craft",
        "mine",
        "get",
        "gather",
        "kill",
        "smelt",
        "place",
    ]
    .iter()
    .any(|v| w.starts_with(v) || *v == w)
}

fn parse_reward_clause(s: &str) -> Vec<QuestPlanReward> {
    let mut rewards = Vec::new();
    for clause in split_clauses(s) {
        if let Some(task) = parse_one_gather(&clause) {
            // reuse gather parser → item reward
            rewards.push(QuestPlanReward {
                id: None,
                reward_type: "item".into(),
                title: None,
                properties: task.properties,
            });
        } else if let Some(xp) = parse_xp(&clause) {
            let mut properties = HashMap::new();
            properties.insert("xp".into(), Value::from(xp));
            rewards.push(QuestPlanReward {
                id: None,
                reward_type: "xp".into(),
                title: None,
                properties,
            });
        }
    }
    rewards
}

fn parse_xp(s: &str) -> Option<i64> {
    let l = s.to_ascii_lowercase();
    if !(l.contains("xp") || l.contains("опыт") || l.contains("опыта")) {
        return None;
    }
    s.split_whitespace()
        .find(|w| w.chars().all(|c| c.is_ascii_digit()))
        .and_then(|w| w.parse().ok())
}

fn resolve_item_phrase(phrase: &str) -> Option<String> {
    let l = phrase.to_lowercase();
    let l = l
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
        .to_string();
    if l.contains(':') {
        return Some(l);
    }
    if l.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && l.contains('_') {
        return Some(format!("minecraft:{l}"));
    }
    const MAP: &[(&str, &str)] = &[
        ("дерева", "minecraft:oak_log"),
        ("дерево", "minecraft:oak_log"),
        ("дров", "minecraft:oak_log"),
        ("брёвн", "minecraft:oak_log"),
        ("бреве", "minecraft:oak_log"),
        ("log", "minecraft:oak_log"),
        ("wood", "minecraft:oak_log"),
        ("булыг", "minecraft:cobblestone"),
        ("булыж", "minecraft:cobblestone"),
        ("cobble", "minecraft:cobblestone"),
        ("палок", "minecraft:stick"),
        ("палка", "minecraft:stick"),
        ("палки", "minecraft:stick"),
        ("палк", "minecraft:stick"),
        ("stick", "minecraft:stick"),
        ("доск", "minecraft:oak_planks"),
        ("plank", "minecraft:oak_planks"),
        ("камн", "minecraft:stone"),
        ("stone", "minecraft:stone"),
        ("угол", "minecraft:coal"),
        ("coal", "minecraft:coal"),
        ("желез", "minecraft:iron_ingot"),
        ("iron", "minecraft:iron_ingot"),
        ("золот", "minecraft:gold_ingot"),
        ("gold", "minecraft:gold_ingot"),
        ("алмаз", "minecraft:diamond"),
        ("diamond", "minecraft:diamond"),
        ("верстак", "minecraft:crafting_table"),
        ("печк", "minecraft:furnace"),
        ("furnace", "minecraft:furnace"),
        ("хлеб", "minecraft:bread"),
        ("bread", "minecraft:bread"),
        ("яблок", "minecraft:apple"),
        ("apple", "minecraft:apple"),
    ];
    for (k, id) in MAP {
        if l.contains(k) {
            return Some((*id).into());
        }
    }
    None
}

fn quest_title_from_tasks(tasks: &[QuestPlanTask], index: usize) -> String {
    if tasks.len() == 1 {
        if let Some(item) = tasks[0].properties.get("item").and_then(|v| v.as_str()) {
            let leaf = item.split(':').next_back().unwrap_or(item);
            let count = tasks[0]
                .properties
                .get("count")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);
            return if count > 1 {
                format!("{count}× {leaf}")
            } else {
                leaf.replace('_', " ")
            };
        }
    }
    if tasks.len() > 1 {
        return format!("Gather set {}", index + 1);
    }
    format!("Quest {}", index + 1)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let t: String = s.chars().take(max).collect();
    format!("{t}…")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlan {
    pub schema_version: u32,
    pub human_explanation: String,
    pub confidence: f64,
    #[serde(default = "default_true")]
    pub needs_user_review: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub chapter_groups: Vec<ChapterGroup>,
    #[serde(default)]
    pub reward_tables: Vec<QuestPlanRewardTable>,
    #[serde(default)]
    pub chapters: Vec<QuestPlanChapter>,
}

/// Reward table draft inside a QuestPlan (merged into QuestBook.reward_tables).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlanRewardTable {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub entries: Vec<QuestPlanWeightedReward>,
    #[serde(default)]
    pub empty_weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlanWeightedReward {
    pub reward_id: String,
    #[serde(default = "default_reward_weight")]
    pub weight: f64,
}

fn default_reward_weight() -> f64 {
    1.0
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlanChapter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_index: Option<i64>,
    /// `upsert` (default) or `replace`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default)]
    pub quests: Vec<QuestPlanQuest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlanQuest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub description: Vec<String>,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub tasks: Vec<QuestPlanTask>,
    #[serde(default)]
    pub rewards: Vec<QuestPlanReward>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlanTask {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", alias = "taskType")]
    pub task_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default)]
    pub properties: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlanReward {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", alias = "rewardType")]
    pub reward_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub properties: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlanValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub book_errors: Vec<QuestPlanBookIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlanBookIssue {
    pub quest_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestPlanMergeResult {
    pub plan: QuestPlan,
    pub validation: QuestPlanValidation,
    pub book: QuestBook,
    pub touched_chapter_ids: Vec<String>,
    pub notes: Vec<String>,
}

const KNOWN_TASK_TYPES: &[&str] = &[
    "item", "checkmark", "kill", "dimension", "biome", "xp", "advancement", "stat", "stage",
    "fluid", "location", "observation", "structure", "custom",
];
const KNOWN_REWARD_TYPES: &[&str] = &[
    "item", "xp", "xp_levels", "command", "random", "choice", "stage", "toast", "custom",
];

pub fn parse_quest_plan(json_str: &str) -> Result<QuestPlan, String> {
    let trimmed = strip_fences(json_str);
    let v: Value =
        serde_json::from_str(trimmed).map_err(|e| format!("Invalid QuestPlan JSON: {e}"))?;
    parse_quest_plan_value(&v)
}

pub fn parse_quest_plan_value(v: &Value) -> Result<QuestPlan, String> {
    // Prefer strict serde when shape looks right.
    if v.get("chapters").is_some() || v.get("schemaVersion").is_some() {
        if let Ok(mut plan) = serde_json::from_value::<QuestPlan>(v.clone()) {
            normalize_plan(&mut plan);
            if plan.chapters.is_empty() && plan.chapter_groups.is_empty() {
                return Err("QuestPlan has no chapters".into());
            }
            return Ok(plan);
        }
    }

    // Loose: { chapters: [...] } with snake_case / missing meta
    let human = str_field(v, &["humanExplanation", "human_explanation"])
        .unwrap_or_else(|| "AI quest draft.".into());
    let confidence = v
        .get("confidence")
        .and_then(|c| c.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let needs_review = v
        .get("needsUserReview")
        .or_else(|| v.get("needs_user_review"))
        .and_then(|b| b.as_bool())
        .unwrap_or(true);
    let chapters_val = v
        .get("chapters")
        .cloned()
        .ok_or_else(|| "QuestPlan missing chapters".to_string())?;
    let chapters: Vec<QuestPlanChapter> = serde_json::from_value(chapters_val)
        .map_err(|e| format!("QuestPlan chapters: {e}"))?;
    let groups = v
        .get("chapterGroups")
        .or_else(|| v.get("chapter_groups"))
        .cloned()
        .and_then(|g| serde_json::from_value(g).ok())
        .unwrap_or_default();

    let mut plan = QuestPlan {
        schema_version: v
            .get("schemaVersion")
            .or_else(|| v.get("schema_version"))
            .and_then(|n| n.as_u64())
            .map(|n| n as u32)
            .unwrap_or(QUEST_PLAN_SCHEMA_VERSION),
        human_explanation: human,
        confidence,
        needs_user_review: needs_review,
        source: str_field(v, &["source"]),
        chapter_groups: groups,
        reward_tables: v
            .get("rewardTables")
            .or_else(|| v.get("reward_tables"))
            .cloned()
            .and_then(|t| serde_json::from_value(t).ok())
            .unwrap_or_default(),
        chapters,
    };
    normalize_plan(&mut plan);
    if plan.chapters.is_empty() {
        return Err("QuestPlan has no chapters".into());
    }
    Ok(plan)
}

fn normalize_plan(plan: &mut QuestPlan) {
    if plan.schema_version == 0 {
        plan.schema_version = QUEST_PLAN_SCHEMA_VERSION;
    }
    plan.confidence = plan.confidence.clamp(0.0, 1.0);
    if plan.human_explanation.trim().is_empty() {
        plan.human_explanation = "AI quest draft.".into();
    }
    for ch in &mut plan.chapters {
        ch.title = ch.title.trim().to_string();
        if let Some(m) = &mut ch.mode {
            *m = m.trim().to_ascii_lowercase();
        }
        for q in &mut ch.quests {
            q.title = q.title.trim().to_string();
            for t in &mut q.tasks {
                t.task_type = t.task_type.trim().to_ascii_lowercase();
            }
            for r in &mut q.rewards {
                r.reward_type = r.reward_type.trim().to_ascii_lowercase();
            }
        }
    }
}

/// Validate plan shape (before or after merge). Does not require a book.
pub fn validate_quest_plan(plan: &QuestPlan) -> QuestPlanValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if plan.schema_version != QUEST_PLAN_SCHEMA_VERSION {
        warnings.push(format!(
            "schemaVersion {} (expected {QUEST_PLAN_SCHEMA_VERSION})",
            plan.schema_version
        ));
    }
    if plan.chapters.is_empty() {
        errors.push("No chapters in plan".into());
    }
    if plan.confidence < 0.35 {
        warnings.push(format!(
            "Low confidence ({:.2}) — review carefully",
            plan.confidence
        ));
    }

    let mut seen_quest_ids: HashSet<String> = HashSet::new();
    for (ci, ch) in plan.chapters.iter().enumerate() {
        let cl = format!("chapters[{ci}]");
        if ch.title.is_empty() {
            errors.push(format!("{cl}: empty title"));
        }
        if let Some(mode) = &ch.mode {
            if mode != "upsert" && mode != "replace" {
                errors.push(format!("{cl}: mode must be upsert|replace (got {mode})"));
            }
        }
        if ch.quests.is_empty() {
            warnings.push(format!("{cl} '{}': no quests", ch.title));
        }
        for (qi, q) in ch.quests.iter().enumerate() {
            let ql = format!("{cl}.quests[{qi}]");
            if q.title.is_empty() {
                errors.push(format!("{ql}: empty title"));
            }
            if let Some(id) = &q.id {
                if !seen_quest_ids.insert(id.clone()) {
                    errors.push(format!("{ql}: duplicate quest id '{id}' in plan"));
                }
            }
            if q.tasks.is_empty() {
                errors.push(format!("{ql} '{}': no tasks", q.title));
            }
            let desc_lines: Vec<_> = q
                .description
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if desc_lines.is_empty() {
                warnings.push(format!(
                    "{ql} '{}': empty description — lore pass recommended",
                    q.title
                ));
            } else if desc_lines.len() < 2 {
                warnings.push(format!(
                    "{ql} '{}': short description (want ≥2 lines)",
                    q.title
                ));
            }
            for t in &q.tasks {
                if !KNOWN_TASK_TYPES.contains(&t.task_type.as_str()) {
                    warnings.push(format!(
                        "{ql}: unknown task type '{}' (kept as custom-ish)",
                        t.task_type
                    ));
                }
            }
            for r in &q.rewards {
                if !KNOWN_REWARD_TYPES.contains(&r.reward_type.as_str()) {
                    warnings.push(format!(
                        "{ql}: unknown reward type '{}'",
                        r.reward_type
                    ));
                }
            }
        }
    }

    QuestPlanValidation {
        valid: errors.is_empty(),
        errors,
        warnings,
        book_errors: vec![],
    }
}

/// Merge plan into a copy of `book`. Resolves title deps, generates missing ids.
pub fn merge_quest_plan(book: &QuestBook, plan: &QuestPlan) -> Result<QuestPlanMergeResult, String> {
    let mut validation = validate_quest_plan(plan);
    if !validation.valid {
        return Ok(QuestPlanMergeResult {
            plan: plan.clone(),
            validation,
            book: book.clone(),
            touched_chapter_ids: vec![],
            notes: vec!["Merge skipped: plan has errors".into()],
        });
    }

    let mut out = book.clone();
    let mut notes = Vec::new();
    let mut touched = Vec::new();

    // Merge chapter groups by id
    for g in &plan.chapter_groups {
        if g.id.is_empty() || g.title.is_empty() {
            continue;
        }
        if let Some(existing) = out.chapter_groups.iter_mut().find(|x| x.id == g.id) {
            existing.title = g.title.clone();
            notes.push(format!("Updated chapter group '{}'", g.title));
        } else {
            out.chapter_groups.push(g.clone());
            notes.push(format!("Added chapter group '{}'", g.title));
        }
    }

    // Merge reward tables by id
    for rt in &plan.reward_tables {
        if rt.id.trim().is_empty() {
            continue;
        }
        let table = crate::unified::RewardTable {
            id: rt.id.clone(),
            title: rt.title.clone(),
            entries: rt
                .entries
                .iter()
                .map(|e| crate::unified::WeightedReward {
                    reward_id: e.reward_id.clone(),
                    weight: e.weight,
                })
                .collect(),
            empty_weight: rt.empty_weight,
            source_file: None,
        };
        if let Some(existing) = out.reward_tables.iter_mut().find(|x| x.id == rt.id) {
            *existing = table;
            notes.push(format!("Updated reward table '{}'", rt.id));
        } else {
            out.reward_tables.push(table);
            notes.push(format!("Added reward table '{}'", rt.id));
        }
    }

    // Title → id map from existing book (for dep resolution)
    let mut title_to_id: HashMap<String, String> = HashMap::new();
    for ch in &out.chapters {
        for q in &ch.quests {
            title_to_id.insert(norm_title(&q.title), q.id.clone());
        }
    }

    // Pre-assign ids for plan quests so cross-chapter deps inside the plan resolve
    let mut plan_chapters: Vec<(String, Chapter, String)> = Vec::new(); // (id, chapter, mode)
    for pch in &plan.chapters {
        let ch_id = pch
            .id
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| new_hex_id(16));
        let mode = pch
            .mode
            .clone()
            .unwrap_or_else(|| "upsert".into());

        let mut quests = Vec::new();
        for pq in &pch.quests {
            let qid = pq
                .id
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| new_hex_id(16));
            title_to_id.insert(norm_title(&pq.title), qid.clone());
            // Also map temporary: if plan uses title as future dep target
            quests.push(materialize_quest(pq, &qid));
        }

        plan_chapters.push((
            ch_id,
            Chapter {
                id: String::new(), // fill below
                title: pch.title.clone(),
                icon: pch.icon.clone(),
                quests,
                group: pch.group.clone(),
                order_index: pch.order_index,
                filename: None,
                default_quest_shape: None,
                default_hide_dependency_lines: None,
                extras: HashMap::new(),
                source_file: None,
            },
            mode,
        ));
    }

    // Second pass: rewrite deps (titles → ids) on materialized quests
    for (_, ch, _) in &mut plan_chapters {
        for q in &mut ch.quests {
            q.dependencies = q
                .dependencies
                .iter()
                .map(|d| resolve_dep_token(d, &title_to_id, &out))
                .collect();
        }
    }

    for (ch_id, mut drafted, mode) in plan_chapters {
        drafted.id = ch_id.clone();
        let idx = out.chapters.iter().position(|c| c.id == ch_id);
        match (idx, mode.as_str()) {
            (Some(i), "replace") => {
                drafted.source_file = out.chapters[i].source_file.clone();
                if drafted.group.is_none() {
                    drafted.group = out.chapters[i].group.clone();
                }
                if drafted.order_index.is_none() {
                    drafted.order_index = out.chapters[i].order_index;
                }
                if drafted.icon.is_none() {
                    drafted.icon = out.chapters[i].icon.clone();
                }
                notes.push(format!(
                    "Replaced chapter '{}' ({} quests)",
                    drafted.title,
                    drafted.quests.len()
                ));
                out.chapters[i] = drafted;
                touched.push(ch_id);
            }
            (Some(i), _) => {
                // upsert quests by id
                let existing = &mut out.chapters[i];
                existing.title = drafted.title.clone();
                if drafted.icon.is_some() {
                    existing.icon = drafted.icon.clone();
                }
                if drafted.group.is_some() {
                    existing.group = drafted.group.clone();
                }
                if drafted.order_index.is_some() {
                    existing.order_index = drafted.order_index;
                }
                for q in drafted.quests {
                    if let Some(eq) = existing.quests.iter_mut().find(|x| x.id == q.id) {
                        *eq = q;
                    } else {
                        existing.quests.push(q);
                    }
                }
                notes.push(format!("Upserted chapter '{}'", existing.title));
                touched.push(ch_id);
            }
            (None, _) => {
                notes.push(format!(
                    "Added chapter '{}' ({} quests)",
                    drafted.title,
                    drafted.quests.len()
                ));
                out.chapters.push(drafted);
                touched.push(ch_id);
            }
        }
    }

    out.chapters.sort_by(|a, b| {
        a.order_index
            .cmp(&b.order_index)
            .then_with(|| a.title.cmp(&b.title))
    });

    let book_errs = out.validate();
    validation.book_errors = book_errs
        .into_iter()
        .map(|e: QuestValidationError| QuestPlanBookIssue {
            quest_id: e.quest_id,
            message: e.message,
        })
        .collect();
    if validation.book_errors.iter().any(|e| {
        e.message.contains("cycle")
            || e.message.contains("Duplicate")
            || e.message.contains("missing")
    }) {
        // Soft: still return merged book; UI can refuse Apply if desired
        validation.warnings.push(
            "Merged book has validation issues — review before saving".into(),
        );
    }

    Ok(QuestPlanMergeResult {
        plan: plan.clone(),
        validation,
        book: out,
        touched_chapter_ids: touched,
        notes,
    })
}

/// Parse + merge in one shot (typical Tauri entry).
pub fn parse_and_merge_quest_plan(
    book: &QuestBook,
    raw: &str,
) -> Result<QuestPlanMergeResult, String> {
    let plan = parse_quest_plan(raw)?;
    merge_quest_plan(book, &plan)
}

fn materialize_quest(pq: &QuestPlanQuest, id: &str) -> Quest {
    let tasks: Vec<Task> = pq
        .tasks
        .iter()
        .map(|t| Task {
            id: t
                .id
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| new_hex_id(12)),
            task_type: if t.task_type.is_empty() {
                "checkmark".into()
            } else {
                t.task_type.clone()
            },
            title: t.title.clone(),
            value: t.value.clone(),
            properties: t.properties.clone(),
        })
        .collect();
    let rewards: Vec<Reward> = pq
        .rewards
        .iter()
        .map(|r| Reward {
            id: r
                .id
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| new_hex_id(12)),
            reward_type: if r.reward_type.is_empty() {
                "xp".into()
            } else {
                r.reward_type.clone()
            },
            title: r.title.clone(),
            properties: r.properties.clone(),
        })
        .collect();

    Quest {
        id: id.to_string(),
        title: pq.title.clone(),
        subtitle: pq.subtitle.clone(),
        description: pq.description.clone(),
        x: pq.x,
        y: pq.y,
        icon: pq.icon.clone(),
        dependencies: pq.dependencies.clone(),
        tasks,
        rewards,
        optional: pq.optional,
        shape: pq.shape.clone(),
        size: pq.size,
        hide_dependency_lines: None,
        hide_dependent_lines: None,
        min_required_dependencies: None,
        can_repeat: None,
        invisible: None,
        disable_toast: None,
        dependency_requirement: None,
        extras: HashMap::new(),
    }
}

fn resolve_dep_token(dep: &str, title_to_id: &HashMap<String, String>, book: &QuestBook) -> String {
    let d = dep.trim();
    if d.is_empty() {
        return d.to_string();
    }
    // Already an id present in book or title map values
    if book.resolve_dep(d).is_some() {
        return d.to_string();
    }
    if title_to_id.values().any(|id| id == d) {
        return d.to_string();
    }
    if let Some(id) = title_to_id.get(&norm_title(d)) {
        return id.clone();
    }
    d.to_string()
}

fn norm_title(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn new_hex_id(len: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Mix time with a simple LCG-ish scramble so rapid calls differ.
    let mut n = nanos ^ (nanos >> 17) ^ 0x9E37_79B9_7F4A_7C15;
    let mut out = String::with_capacity(len);
    const HEX: &[u8] = b"0123456789ABCDEF";
    for _ in 0..len {
        n = n
            .wrapping_mul(0x5851_F42D_4C95_7F2D)
            .wrapping_add(0x1405_7B7E_F767_814F);
        out.push(HEX[(n as usize) & 0xF] as char);
    }
    out
}

fn strip_fences(json_str: &str) -> &str {
    json_str
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

fn str_field(v: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = v.get(*k).and_then(|x| x.as_str()) {
            return Some(s.to_string());
        }
    }
    None
}

/// Outline-only system prompt (Pass A) — stubs, deps, tasks/rewards skeletons; descriptions may be empty.
pub const QUEST_OUTLINE_SYSTEM_PROMPT: &str = r#"You are TuffBox Quest Outline Planner. Output ONLY one JSON QuestPlan (schemaVersion 1).
Focus on STRUCTURE for a large quest line (titles, dependency graph, x/y optional, tasks, rewards).
Descriptions may be empty or one stub line — a later lore pass fills them.
Every quest MUST have ≥1 task. Prefer concrete item ids from context.
Do not invent cyclic dependencies. Prefer 16-char hex ids or omit for launcher generation.
"#;

/// Lore expansion prompt (Pass B) — fill description[] for listed quests.
pub const QUEST_LORE_SYSTEM_PROMPT: &str = r#"You are TuffBox Quest Lore Writer. Output ONLY JSON:
{ "quests": [ { "id": "HEX_OR_TITLE", "title": "...", "subtitle": "...", "description": ["line1","line2","line3"] } ] }
Write 3–6 flavorful description lines per quest (Minecraft pack tone). Use & formatting sparingly (&a, &7, &l).
Match ids/titles from the user list. No SNBT. No markdown fences required but ok.
"#;

/// Detect desired quest count from NL (default 16, clamp 4..=40).
pub fn detect_target_quest_count(prompt: &str) -> usize {
    let lower = prompt.to_ascii_lowercase();
    // Explicit numbers near "quest" / "квест"
    let re_candidates = [
        r"(?i)(\d+)\s*(?:\+|plus)?\s*(?:quests?|квест)",
        r"(?i)(?:quests?|квест\w*)\s*(?:на|about|of|=|:)?\s*(\d+)",
        r"(?i)линейк\w*\s*на\s*(\d+)",
        r"(?i)line\s*(?:of|=|:)?\s*(\d+)",
    ];
    for pat in re_candidates {
        if let Ok(re) = regex_lite_first_num(pat, &lower) {
            return re.clamp(4, 40);
        }
    }
    // Digits alone if prompt mentions "глава" / chapter / line
    if lower.contains("квест") || lower.contains("quest") || lower.contains("линей") || lower.contains("chapter")
    {
        for token in lower.split(|c: char| !c.is_ascii_digit()) {
            if let Ok(n) = token.parse::<usize>() {
                if (8..=40).contains(&n) {
                    return n;
                }
            }
        }
    }
    16
}

fn regex_lite_first_num(pat: &str, hay: &str) -> Result<usize, ()> {
    // Minimal without regex crate: only handle simple patterns manually for first two forms.
    let _ = pat;
    // Scan for "N quest" / "N квест"
    let bytes = hay.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let n: usize = hay[start..i].parse().unwrap_or(0);
            let rest = hay[i..].trim_start();
            if rest.starts_with("quest")
                || rest.starts_with("квест")
                || rest.starts_with("+ quest")
                || rest.starts_with("+квест")
            {
                if n >= 4 {
                    return Ok(n);
                }
            }
            continue;
        }
        i += 1;
    }
    // "на N" after линейк
    if let Some(pos) = hay.find("на ") {
        let after = &hay[pos + 3..];
        let num: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = num.parse::<usize>() {
            if n >= 4 {
                return Ok(n);
            }
        }
    }
    Err(())
}

/// Place quests along a dependency DAG when coordinates are all ~0 or overlapping.
pub fn auto_layout_plan(plan: &mut QuestPlan) {
    for ch in &mut plan.chapters {
        auto_layout_quests(&mut ch.quests);
    }
}

fn auto_layout_quests(quests: &mut [QuestPlanQuest]) {
    if quests.is_empty() {
        return;
    }
    // Build title/id index
    let mut id_of: HashMap<String, usize> = HashMap::new();
    for (i, q) in quests.iter().enumerate() {
        if let Some(id) = &q.id {
            id_of.insert(id.clone(), i);
        }
        id_of.insert(norm_title(&q.title), i);
        id_of.insert(q.title.clone(), i);
    }

    let n = quests.len();
    let mut rank = vec![0usize; n];
    let mut indeg = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    for (i, q) in quests.iter().enumerate() {
        for d in &q.dependencies {
            let di = id_of
                .get(d)
                .copied()
                .or_else(|| id_of.get(&norm_title(d)).copied());
            if let Some(j) = di {
                if j != i {
                    adj[j].push(i);
                    indeg[i] += 1;
                }
            }
        }
    }
    // Kahn topo for ranks
    let mut queue: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut seen = 0usize;
    while let Some(u) = queue.pop() {
        seen += 1;
        for &v in &adj[u] {
            rank[v] = rank[v].max(rank[u] + 1);
            indeg[v] = indeg[v].saturating_sub(1);
            if indeg[v] == 0 {
                queue.push(v);
            }
        }
    }
    if seen < n {
        // Cycle fallback: sequential
        for (i, r) in rank.iter_mut().enumerate() {
            *r = i;
        }
    }

    let mut by_rank: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, &r) in rank.iter().enumerate() {
        by_rank.entry(r).or_default().push(i);
    }
    let col_gap = 2.75_f64;
    let row_gap = 2.5_f64;
    for (r, idxs) in by_rank {
        let count = idxs.len() as f64;
        for (k, &i) in idxs.iter().enumerate() {
            let y = (k as f64 - (count - 1.0) / 2.0) * row_gap;
            quests[i].x = r as f64 * col_gap;
            quests[i].y = y;
        }
    }
}

/// Fill missing descriptions with template lore (offline fallback).
pub fn fill_template_lore(plan: &mut QuestPlan) {
    for ch in &mut plan.chapters {
        for q in &mut ch.quests {
            let nonempty: Vec<_> = q
                .description
                .iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if nonempty.len() >= 2 {
                q.description = nonempty;
                continue;
            }
            let title = q.title.clone();
            q.description = vec![
                format!("&7Complete: &f{title}"),
                format!("&8Part of chapter progress — finish the objectives to unlock the next step."),
                if q.optional {
                    "&eOptional side objective.".into()
                } else {
                    "&aRequired for the main line.".into()
                },
            ];
            if q.subtitle.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                q.subtitle = Some(format!("Progress · {title}"));
            }
        }
    }
}

/// Map vague / missing item ids toward catalog entries; returns notes.
pub fn ground_items_in_plan(plan: &mut QuestPlan, catalog: &[String]) -> Vec<String> {
    let mut notes = Vec::new();
    if catalog.is_empty() {
        return notes;
    }
    let catalog_lower: Vec<(String, String)> = catalog
        .iter()
        .map(|id| (id.to_ascii_lowercase(), id.clone()))
        .collect();

    for ch in &mut plan.chapters {
        for q in &mut ch.quests {
            for t in &mut q.tasks {
                if t.task_type != "item" {
                    continue;
                }
                if let Some(item) = t.properties.get("item").and_then(|v| v.as_str()) {
                    if let Some(resolved) = resolve_catalog_item(item, &catalog_lower) {
                        if resolved != item {
                            notes.push(format!(
                                "Grounded task item '{}' → '{}' ({})",
                                item, resolved, q.title
                            ));
                            t.properties
                                .insert("item".into(), Value::String(resolved));
                        }
                    } else if !item.contains(':') {
                        notes.push(format!(
                            "Uncertain item '{}' in quest '{}' — needsUserReview",
                            item, q.title
                        ));
                        plan.needs_user_review = true;
                    }
                }
            }
            for r in &mut q.rewards {
                if r.reward_type != "item" {
                    continue;
                }
                if let Some(item) = r.properties.get("item").and_then(|v| v.as_str()) {
                    if let Some(resolved) = resolve_catalog_item(item, &catalog_lower) {
                        if resolved != item {
                            notes.push(format!(
                                "Grounded reward item '{}' → '{}' ({})",
                                item, resolved, q.title
                            ));
                            r.properties
                                .insert("item".into(), Value::String(resolved));
                        }
                    }
                }
            }
        }
    }
    notes
}

fn resolve_catalog_item(raw: &str, catalog: &[(String, String)]) -> Option<String> {
    let needle = raw.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return None;
    }
    if let Some((_, id)) = catalog.iter().find(|(l, _)| l == &needle) {
        return Some(id.clone());
    }
    // leaf match: oak_log against minecraft:oak_log
    let leaf = needle.rsplit(':').next().unwrap_or(&needle);
    let matches: Vec<_> = catalog
        .iter()
        .filter(|(l, _)| l.rsplit(':').next() == Some(leaf) || l.contains(leaf))
        .collect();
    if matches.len() == 1 {
        return Some(matches[0].1.clone());
    }
    // Prefer minecraft: when multiple
    if let Some((_, id)) = matches.iter().find(|(l, _)| l.starts_with("minecraft:")) {
        return Some(id.clone());
    }
    None
}

/// Apply lore patch JSON `{ quests: [{ id|title, description, subtitle? }] }` onto plan.
pub fn stitch_lore_into_plan(plan: &mut QuestPlan, lore_json: &str) -> Result<usize, String> {
    let v: Value = serde_json::from_str(strip_fences(lore_json))
        .map_err(|e| format!("Invalid lore JSON: {e}"))?;
    let arr = v
        .get("quests")
        .and_then(|x| x.as_array())
        .ok_or_else(|| "lore JSON missing quests[]".to_string())?;
    let mut updated = 0usize;
    for entry in arr {
        let id = entry.get("id").and_then(|x| x.as_str()).unwrap_or("");
        let title = entry.get("title").and_then(|x| x.as_str()).unwrap_or("");
        let desc: Vec<String> = entry
            .get("description")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|l| l.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        if desc.is_empty() {
            continue;
        }
        let subtitle = entry
            .get("subtitle")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string());
        for ch in &mut plan.chapters {
            for q in &mut ch.quests {
                let match_id = !id.is_empty()
                    && q.id.as_ref().map(|x| x.as_str()) == Some(id);
                let match_title = !title.is_empty() && norm_title(&q.title) == norm_title(title);
                if match_id || match_title {
                    q.description = desc.clone();
                    if let Some(s) = &subtitle {
                        q.subtitle = Some(s.clone());
                    }
                    updated += 1;
                }
            }
        }
    }
    Ok(updated)
}

/// Filter plan to selected chapter titles/ids and quest titles/ids (for review Apply selected).
pub fn filter_plan_selection(
    plan: &QuestPlan,
    chapter_keys: &[String],
    quest_keys: &[String],
) -> QuestPlan {
    let mut out = plan.clone();
    if !chapter_keys.is_empty() {
        out.chapters.retain(|ch| {
            chapter_keys.iter().any(|k| {
                ch.id.as_ref().map(|id| id == k).unwrap_or(false)
                    || norm_title(&ch.title) == norm_title(k)
                    || ch.title == *k
            })
        });
    }
    if !quest_keys.is_empty() {
        for ch in &mut out.chapters {
            ch.quests.retain(|q| {
                quest_keys.iter().any(|k| {
                    q.id.as_ref().map(|id| id == k).unwrap_or(false)
                        || norm_title(&q.title) == norm_title(k)
                        || q.title == *k
                })
            });
        }
        out.chapters.retain(|ch| !ch.quests.is_empty());
    }
    out
}

/// Build user message for outline pass requesting N quests.
pub fn build_outline_user_message(request: &str, ctx: &QuestAuthorContext, target_count: usize) -> String {
    let mut p = build_quest_author_user_message(request, ctx);
    p.push_str(&format!(
        "\nTarget: about {target_count} quests in one coherent chapter (or split only if asked).\n"
    ));
    p.push_str(
        "Include dependency chains (and light branches). Leave description empty or 1 stub line.\n",
    );
    p
}

/// Build lore-pass user message for a batch of quests.
pub fn build_lore_user_message(plan: &QuestPlan, quest_indices: &[(usize, usize)]) -> String {
    let mut p = String::from("Write lore descriptions for these quests:\n");
    for &(ci, qi) in quest_indices {
        if let Some(ch) = plan.chapters.get(ci) {
            if let Some(q) = ch.quests.get(qi) {
                p.push_str(&format!(
                    "- id={} title=\"{}\" chapter=\"{}\" tasks={:?}\n",
                    q.id.as_deref().unwrap_or(""),
                    q.title,
                    ch.title,
                    q.tasks
                        .iter()
                        .map(|t| t.task_type.as_str())
                        .collect::<Vec<_>>()
                ));
            }
        }
    }
    p.push_str("Respond with ONLY the lore JSON object.\n");
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_book() -> QuestBook {
        QuestBook::default()
    }

    #[test]
    fn detects_quest_count() {
        assert_eq!(detect_target_quest_count("линейка на 24 квеста early game"), 24);
        assert_eq!(detect_target_quest_count("make 20 quests about nether"), 20);
        assert!(detect_target_quest_count("something vague") >= 4);
    }

    #[test]
    fn auto_layout_places_chain() {
        let mut plan = QuestPlan {
            schema_version: 1,
            human_explanation: "t".into(),
            confidence: 0.9,
            needs_user_review: false,
            source: Some("test".into()),
            chapter_groups: vec![],
            reward_tables: vec![],
            chapters: vec![QuestPlanChapter {
                id: Some("CH".into()),
                title: "Ch".into(),
                icon: None,
                group: None,
                order_index: None,
                mode: Some("upsert".into()),
                quests: (0..5)
                    .map(|i| QuestPlanQuest {
                        id: Some(format!("Q{i}")),
                        title: format!("Quest {i}"),
                        subtitle: None,
                        description: vec!["a".into(), "b".into()],
                        x: 0.0,
                        y: 0.0,
                        icon: None,
                        dependencies: if i == 0 {
                            vec![]
                        } else {
                            vec![format!("Q{}", i - 1)]
                        },
                        tasks: vec![QuestPlanTask {
                            id: None,
                            task_type: "checkmark".into(),
                            title: None,
                            value: None,
                            properties: HashMap::new(),
                        }],
                        rewards: vec![],
                        optional: false,
                        shape: None,
                        size: None,
                    })
                    .collect(),
            }],
        };
        auto_layout_plan(&mut plan);
        let xs: Vec<_> = plan.chapters[0].quests.iter().map(|q| q.x).collect();
        assert!(xs[4] > xs[0]);
    }

    #[test]
    fn fill_template_lore_fills_empty() {
        let mut plan = QuestPlan {
            schema_version: 1,
            human_explanation: "t".into(),
            confidence: 0.8,
            needs_user_review: true,
            source: None,
            chapter_groups: vec![],
            reward_tables: vec![],
            chapters: vec![QuestPlanChapter {
                id: None,
                title: "Ch".into(),
                icon: None,
                group: None,
                order_index: None,
                mode: None,
                quests: vec![QuestPlanQuest {
                    id: None,
                    title: "Wood".into(),
                    subtitle: None,
                    description: vec![],
                    x: 0.0,
                    y: 0.0,
                    icon: None,
                    dependencies: vec![],
                    tasks: vec![QuestPlanTask {
                        id: None,
                        task_type: "checkmark".into(),
                        title: None,
                        value: None,
                        properties: HashMap::new(),
                    }],
                    rewards: vec![],
                    optional: false,
                    shape: None,
                    size: None,
                }],
            }],
        };
        fill_template_lore(&mut plan);
        assert!(plan.chapters[0].quests[0].description.len() >= 2);
        let v = validate_quest_plan(&plan);
        assert!(v.valid);
    }

    #[test]
    fn parses_fenced_minimal_plan() {
        let raw = r#"```json
{
  "schemaVersion": 1,
  "humanExplanation": "Early Create",
  "confidence": 0.8,
  "chapters": [{
    "title": "Andesite Age",
    "quests": [{
      "title": "Get Cobble",
      "x": 0, "y": 0,
      "tasks": [{ "type": "item", "properties": { "item": "minecraft:cobblestone" } }],
      "rewards": [{ "type": "xp", "properties": { "xp": 5 } }]
    }, {
      "title": "Andesite Alloy",
      "x": 2, "y": 0,
      "dependencies": ["Get Cobble"],
      "tasks": [{ "type": "item", "properties": { "item": "create:andesite_alloy" } }]
    }]
  }]
}
```"#;
        let plan = parse_quest_plan(raw).unwrap();
        assert_eq!(plan.chapters.len(), 1);
        assert_eq!(plan.chapters[0].quests.len(), 2);
        let merged = merge_quest_plan(&empty_book(), &plan).unwrap();
        assert!(merged.validation.valid);
        assert_eq!(merged.book.chapters.len(), 1);
        assert_eq!(merged.book.chapters[0].quests.len(), 2);
        let alloy = merged.book.chapters[0]
            .quests
            .iter()
            .find(|q| q.title == "Andesite Alloy")
            .unwrap();
        let cobble = merged.book.chapters[0]
            .quests
            .iter()
            .find(|q| q.title == "Get Cobble")
            .unwrap();
        assert_eq!(alloy.dependencies, vec![cobble.id.clone()]);
        assert!(merged.book.validate().is_empty());
    }

    #[test]
    fn rejects_quest_without_tasks() {
        let raw = r#"{
          "schemaVersion": 1,
          "humanExplanation": "bad",
          "confidence": 0.9,
          "chapters": [{ "title": "X", "quests": [{ "title": "Y", "tasks": [] }] }]
        }"#;
        let plan = parse_quest_plan(raw).unwrap();
        let v = validate_quest_plan(&plan);
        assert!(!v.valid);
        assert!(v.errors.iter().any(|e| e.contains("no tasks")));
    }

    #[test]
    fn upsert_merges_into_existing_chapter() {
        let mut book = empty_book();
        book.chapters.push(Chapter {
            id: "CHEXIST".into(),
            title: "Old".into(),
            icon: None,
            quests: vec![Quest {
                id: "QOLD".into(),
                title: "Old Quest".into(),
                subtitle: None,
                description: vec![],
                x: 0.0,
                y: 0.0,
                icon: None,
                dependencies: vec![],
                tasks: vec![Task {
                    id: "T1".into(),
                    task_type: "checkmark".into(),
                    title: None,
                    value: None,
                    properties: HashMap::new(),
                }],
                rewards: vec![],
                optional: false,
                shape: None,
                size: None,
                hide_dependency_lines: None,
                hide_dependent_lines: None,
                min_required_dependencies: None,
                can_repeat: None,
                invisible: None,
                disable_toast: None,
                dependency_requirement: None,
                extras: HashMap::new(),
            }],
            group: None,
            order_index: Some(0),
            filename: None,
            default_quest_shape: None,
            default_hide_dependency_lines: None,
            extras: HashMap::new(),
            source_file: Some("config/ftbquests/quests/chapters/old.snbt".into()),
        });
        let plan = parse_quest_plan(
            r#"{
            "schemaVersion": 1,
            "humanExplanation": "add one",
            "confidence": 0.7,
            "chapters": [{
              "id": "CHEXIST",
              "title": "Old",
              "mode": "upsert",
              "quests": [{
                "title": "New Quest",
                "x": 3, "y": 0,
                "tasks": [{ "type": "checkmark" }]
              }]
            }]
          }"#,
        )
        .unwrap();
        let merged = merge_quest_plan(&book, &plan).unwrap();
        assert_eq!(merged.book.chapters.len(), 1);
        assert_eq!(merged.book.chapters[0].quests.len(), 2);
        assert_eq!(
            merged.book.chapters[0].source_file.as_deref(),
            Some("config/ftbquests/quests/chapters/old.snbt")
        );
    }

    #[test]
    fn heuristic_parses_russian_demo_prompt() {
        let plan = try_heuristic_quest_plan(
            "создай главу 1: начало развития, в ней квесты - 1. добудь 10 дерева, накопай 20 булыги - награда 10 палок.",
        )
        .expect("heuristic should parse demo prompt");
        assert!(
            plan.chapters[0].title.to_lowercase().contains("начало"),
            "title was {:?}",
            plan.chapters[0].title
        );
        assert_eq!(plan.chapters[0].quests.len(), 1);
        let q = &plan.chapters[0].quests[0];
        assert_eq!(q.tasks.len(), 2);
        assert_eq!(
            q.tasks[0].properties.get("item").and_then(|v| v.as_str()),
            Some("minecraft:oak_log")
        );
        assert_eq!(
            q.tasks[0].properties.get("count").and_then(|v| v.as_i64()),
            Some(10)
        );
        assert_eq!(
            q.tasks[1].properties.get("item").and_then(|v| v.as_str()),
            Some("minecraft:cobblestone")
        );
        assert_eq!(
            q.tasks[1].properties.get("count").and_then(|v| v.as_i64()),
            Some(20)
        );
        assert_eq!(q.rewards.len(), 1);
        assert_eq!(
            q.rewards[0].properties.get("item").and_then(|v| v.as_str()),
            Some("minecraft:stick")
        );
        assert_eq!(
            q.rewards[0].properties.get("count").and_then(|v| v.as_i64()),
            Some(10)
        );
        let merged = merge_quest_plan(&empty_book(), &plan).unwrap();
        assert!(merged.validation.valid);
        assert!(merged.book.validate().is_empty());
    }
}
