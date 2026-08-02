//! Quest AI chat sessions + multi-pass quest-line generation.

use std::path::PathBuf;

use serde::Serialize;
use tuffbox_core::{
    auto_layout_plan, build_lore_user_message, build_outline_user_message, delete_quest_chat,
    detect_target_quest_count, fill_template_lore, filter_plan_selection, ground_items_in_plan,
    list_quest_chats, load_quest_chat, merge_quest_plan, new_quest_chat_id, now_iso,
    parse_quest_plan, save_quest_chat, stitch_lore_into_plan, try_heuristic_quest_plan,
    QuestAuthorContext, QuestChatMessage, QuestChatSession, QuestPlan, QuestPlanMergeResult,
    QUEST_LORE_SYSTEM_PROMPT, QUEST_OUTLINE_SYSTEM_PROMPT, QUEST_PLAN_SYSTEM_PROMPT,
};

use crate::integrations;
use crate::manifest_parent;

fn project_dir(path: &str) -> Result<PathBuf, String> {
    manifest_parent(path)
}

fn collect_items(path: &str) -> Vec<String> {
    let Ok(manifest_path) = crate::resolve_manifest_path(path) else {
        return Vec::new();
    };
    crate::collect_catalog_item_ids(&manifest_path)
        .unwrap_or_default()
        .into_iter()
        .take(120)
        .collect()
}

fn author_ctx(
    path: &str,
    book: &tuffbox_core::unified::QuestBook,
    pack_hint: Option<String>,
) -> QuestAuthorContext {
    QuestAuthorContext {
        existing_chapters: book.chapters.iter().map(|c| c.title.clone()).collect(),
        sample_items: collect_items(path),
        pack_hint: pack_hint.or_else(|| book.title.clone()),
    }
}

async fn ai_json(
    system: &str,
    user: &str,
    history: &[QuestChatMessage],
) -> Result<serde_json::Value, String> {
    let settings = integrations::get_integration_status().settings;
    let mut messages = Vec::new();
    for m in history.iter().rev().take(6).collect::<Vec<_>>().into_iter().rev() {
        if m.role == "user" || m.role == "assistant" {
            messages.push(serde_json::json!({
                "role": m.role,
                "content": m.content,
            }));
        }
    }
    messages.push(serde_json::json!({"role": "user", "content": user}));
    integrations::call_ai_messages(&settings.ai, system, &messages, true).await
}

fn value_to_raw(value: serde_json::Value) -> String {
    if value.is_string() {
        value.as_str().unwrap_or("").to_string()
    } else {
        value.to_string()
    }
}

/// Outline AI call with one repair retry on invalid QuestPlan JSON.
async fn ai_quest_plan(
    system: &str,
    user: &str,
    history: &[QuestChatMessage],
) -> Result<QuestPlan, String> {
    let value = ai_json(system, user, history).await?;
    match parse_quest_plan(&value_to_raw(value)) {
        Ok(plan) => Ok(plan),
        Err(first_err) => {
            let repair = format!(
                "{user}\n\nYour previous answer was invalid QuestPlan JSON ({first_err}).\nReturn ONLY one JSON object matching schemaVersion 1 (chapters with quests, tasks, rewards). No markdown fences."
            );
            let value2 = ai_json(system, &repair, &[]).await?;
            parse_quest_plan(&value_to_raw(value2))
                .map_err(|e| format!("Invalid QuestPlan JSON after retry: {e}"))
        }
    }
}

/// Multi-pass: outline → lore chunks → ground items → layout.
pub async fn run_generate_quest_line(
    path: &str,
    prompt: &str,
    book: &tuffbox_core::unified::QuestBook,
    history: &[QuestChatMessage],
    force_ai: bool,
    intent: Option<&str>,
    pending_plan: Option<&QuestPlan>,
) -> Result<(QuestPlan, Vec<String>), String> {
    let mut log = Vec::new();
    let intent = intent.unwrap_or("generate");
    let ctx = author_ctx(path, book, None);
    let target = detect_target_quest_count(prompt);
    log.push(format!("Target quest count ≈ {target}"));

    // Follow-up: lore-only on pending — handled by caller with existing plan
    let mut plan = if !force_ai && intent == "generate" {
        if let Some(p) = try_heuristic_quest_plan(prompt) {
            log.push("Outline: offline heuristic".into());
            p
        } else {
            log.push("Outline: AI…".into());
            let user = build_outline_user_message(prompt, &ctx, target);
            ai_quest_plan(QUEST_OUTLINE_SYSTEM_PROMPT, &user, history).await?
        }
    } else if intent == "generate" || intent == "extend" {
        log.push("Outline: AI…".into());
        let user = if intent == "extend" {
            let pending_json = pending_plan
                .and_then(|p| serde_json::to_string_pretty(p).ok())
                .unwrap_or_else(|| "(no pending plan yet)".into());
            format!(
                "{prompt}\n\nExtend / append about {target} additional quests to the existing pending plan below. Keep prior quests; add new ones with dependencies.\n\nPending plan JSON:\n{pending_json}"
            )
        } else {
            build_outline_user_message(prompt, &ctx, target)
        };
        ai_quest_plan(QUEST_OUTLINE_SYSTEM_PROMPT, &user, history).await?
    } else {
        return Err(format!("unknown intent: {intent}"));
    };

    plan.source = Some(if plan.source.as_deref() == Some("heuristic") {
        "heuristic+multipass".into()
    } else {
        "ai-multipass".into()
    });

    // Lore pass in chunks of 6
    let indices: Vec<(usize, usize)> = plan
        .chapters
        .iter()
        .enumerate()
        .flat_map(|(ci, ch)| (0..ch.quests.len()).map(move |qi| (ci, qi)))
        .collect();
    let chunk_size = 6usize;
    let total_chunks = indices.chunks(chunk_size).count().max(1);
    let mut chunk_i = 0usize;
    for chunk in indices.chunks(chunk_size) {
        chunk_i += 1;
        log.push(format!("Lore {chunk_i}/{total_chunks}…"));
        let user = build_lore_user_message(&plan, chunk);
        match ai_json(QUEST_LORE_SYSTEM_PROMPT, &user, &[]).await {
            Ok(value) => {
                match stitch_lore_into_plan(&mut plan, &value_to_raw(value)) {
                    Ok(n) => log.push(format!("Lore chunk updated {n} quest(s)")),
                    Err(e) => log.push(format!("Lore stitch skip: {e}")),
                }
            }
            Err(e) => {
                log.push(format!("Lore AI unavailable ({e}) — template fill later"));
            }
        }
    }

    fill_template_lore(&mut plan);
    log.push("Lore: ensured ≥2 description lines".into());

    let ground_notes = ground_items_in_plan(&mut plan, &ctx.sample_items);
    log.extend(ground_notes.iter().cloned().take(12));
    log.push(format!("Grounding: {} note(s)", ground_notes.len()));

    auto_layout_plan(&mut plan);
    log.push("Layout: DAG auto-layout applied".into());

    if plan.human_explanation.trim().is_empty() {
        plan.human_explanation = format!("Generated quest line (~{target} quests) from author request.");
    }

    Ok((plan, log))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestChatTurnResult {
    pub session: QuestChatSession,
    pub merge: QuestPlanMergeResult,
    pub progress_log: Vec<String>,
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_quest_chat_sessions(path: String) -> Result<Vec<QuestChatSession>, String> {
    let dir = project_dir(&path)?;
    list_quest_chats(&dir)
}

#[tauri::command(rename_all = "camelCase")]
pub fn save_quest_chat_session(path: String, session: QuestChatSession) -> Result<(), String> {
    let dir = project_dir(&path)?;
    save_quest_chat(&dir, &session)?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub fn load_quest_chat_session(path: String, chat_id: String) -> Result<QuestChatSession, String> {
    let dir = project_dir(&path)?;
    load_quest_chat(&dir, &chat_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_quest_chat_session(path: String, chat_id: String) -> Result<(), String> {
    let dir = project_dir(&path)?;
    delete_quest_chat(&dir, &chat_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn new_quest_chat_session(
    path: String,
    title: Option<String>,
) -> Result<QuestChatSession, String> {
    let dir = project_dir(&path)?;
    let session = QuestChatSession {
        id: new_quest_chat_id(),
        title: title.unwrap_or_else(|| "Quest line".into()),
        messages: vec![],
        pending_plan: None,
        updated_at: now_iso(),
    };
    save_quest_chat(&dir, &session)?;
    Ok(session)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn quest_chat_turn(
    path: String,
    chat_id: Option<String>,
    message: String,
    force_ai: Option<bool>,
    intent: Option<String>,
) -> Result<QuestChatTurnResult, String> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("Empty message".into());
    }
    let project_dir = project_dir(&path)?;
    let book = tuffbox_core::unified::QuestBook::load_from_project(&project_dir)?;
    let force_ai = force_ai.unwrap_or(false);
    let intent_s = intent.unwrap_or_else(|| "generate".into());

    let mut session = if let Some(id) = chat_id.filter(|s| !s.is_empty()) {
        load_quest_chat(&project_dir, &id).unwrap_or_else(|_| QuestChatSession {
            id: new_quest_chat_id(),
            title: truncate_title(&message),
            messages: vec![],
            pending_plan: None,
            updated_at: now_iso(),
        })
    } else {
        QuestChatSession {
            id: new_quest_chat_id(),
            title: truncate_title(&message),
            messages: vec![],
            pending_plan: None,
            updated_at: now_iso(),
        }
    };

    session.messages.push(QuestChatMessage {
        role: "user".into(),
        content: message.clone(),
        created_at: Some(now_iso()),
        plan: None,
        progress_log: None,
    });

    let (mut plan, log) = if intent_s == "lore" {
        let mut base = session
            .pending_plan
            .clone()
            .ok_or_else(|| "No pending plan to regenerate lore".to_string())?;
        log_lore_only(&path, &mut base, &mut Vec::new()).await?;
        fill_template_lore(&mut base);
        auto_layout_plan(&mut base);
        (base, vec!["Lore-only regenerate done".into()])
    } else {
        run_generate_quest_line(
            &path,
            &message,
            &book,
            &session.messages,
            force_ai,
            Some(intent_s.as_str()),
            session.pending_plan.as_ref(),
        )
        .await?
    };

    // Prefer full system prompt validation path
    if plan.schema_version == 0 {
        plan.schema_version = 1;
    }

    let merge = merge_quest_plan(&book, &plan)?;
    session.pending_plan = Some(plan.clone());
    let assistant_text = format!(
        "{}\n\n(progress: {})",
        plan.human_explanation,
        log.join(" · ")
    );
    session.messages.push(QuestChatMessage {
        role: "assistant".into(),
        content: assistant_text,
        created_at: Some(now_iso()),
        plan: Some(plan),
        progress_log: Some(log.clone()),
    });
    session.updated_at = now_iso();
    save_quest_chat(&project_dir, &session)?;

    Ok(QuestChatTurnResult {
        session,
        merge,
        progress_log: log,
    })
}

async fn log_lore_only(
    path: &str,
    plan: &mut QuestPlan,
    log: &mut Vec<String>,
) -> Result<(), String> {
    let indices: Vec<(usize, usize)> = plan
        .chapters
        .iter()
        .enumerate()
        .flat_map(|(ci, ch)| (0..ch.quests.len()).map(move |qi| (ci, qi)))
        .collect();
    for chunk in indices.chunks(6) {
        let user = build_lore_user_message(plan, chunk);
        match ai_json(QUEST_LORE_SYSTEM_PROMPT, &user, &[]).await {
            Ok(value) => {
                let _ = stitch_lore_into_plan(plan, &value_to_raw(value));
                log.push("Lore chunk ok".into());
            }
            Err(e) => log.push(format!("Lore fail: {e}")),
        }
    }
    let _ = path;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn generate_quest_line(
    path: String,
    prompt: String,
    force_ai: Option<bool>,
) -> Result<serde_json::Value, String> {
    let prompt = prompt.trim().to_string();
    if prompt.is_empty() {
        return Err("Empty prompt".into());
    }
    let project_dir = project_dir(&path)?;
    let book = tuffbox_core::unified::QuestBook::load_from_project(&project_dir)?;
    let (plan, log) =
        run_generate_quest_line(
            &path,
            &prompt,
            &book,
            &[],
            force_ai.unwrap_or(false),
            Some("generate"),
            None,
        )
            .await?;
    let mut merge = merge_quest_plan(&book, &plan)?;
    merge.notes.extend(log);
    serde_json::to_value(merge).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn filter_and_merge_quest_plan(
    path: String,
    plan: QuestPlan,
    chapter_keys: Vec<String>,
    quest_keys: Vec<String>,
) -> Result<serde_json::Value, String> {
    let project_dir = project_dir(&path)?;
    let book = tuffbox_core::unified::QuestBook::load_from_project(&project_dir)?;
    let filtered = filter_plan_selection(&plan, &chapter_keys, &quest_keys);
    let result = merge_quest_plan(&book, &filtered)?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

fn truncate_title(s: &str) -> String {
    let t: String = s.chars().take(48).collect();
    if s.chars().count() > 48 {
        format!("{t}…")
    } else if t.is_empty() {
        "Quest line".into()
    } else {
        t
    }
}

// Silence unused import if QUEST_PLAN_SYSTEM_PROMPT kept for future
#[allow(dead_code)]
fn _prompt_ref() -> &'static str {
    QUEST_PLAN_SYSTEM_PROMPT
}
