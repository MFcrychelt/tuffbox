//! Create Mode chat / pack assemble Tauri commands.

use crate::{
    auto_snapshot, download_project_mods_tracked, install_modrinth_with_dependencies_rounds,
    manifest_parent, save_manifest,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use tuffbox_core::create_mode::{
    assemble_pack_draft as run_assemble_pack_draft, delete_create_chat as delete_create_chat_file,
    list_create_chats as list_create_chat_files, load_create_chat as load_create_chat_file,
    new_chat_id, now_iso, parse_create_mode_ai_response, save_create_chat as save_create_chat_file,
    AssembleOptions, CreateChatMessage, CreateChatSession, LiveModrinthSearch, PackBrief, PackDraft,
    CREATE_MODE_SYSTEM_PROMPT,
};
use tuffbox_core::graph::loader_kind_slug;
use tuffbox_core::{ContentProvider, ModrinthProvider, ProjectManifest};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateModeProgress {
    phase: String,
    done: usize,
    total: usize,
    current: String,
}

fn emit_create_progress(
    app: &AppHandle,
    phase: &str,
    done: usize,
    total: usize,
    current: &str,
) {
    let _ = app.emit(
        "create-mode://progress",
        CreateModeProgress {
            phase: phase.to_string(),
            done,
            total,
            current: current.to_string(),
        },
    );
}

fn ensure_brief_from_manifest(mut brief: PackBrief, manifest: &ProjectManifest) -> PackBrief {
    if brief.mc_version.trim().is_empty() {
        brief.mc_version = manifest.minecraft.version.clone();
    }
    if brief.loader.trim().is_empty() {
        brief.loader = loader_kind_slug(&manifest.loader.kind).to_string();
    }
    brief
}

#[tauri::command(rename_all = "camelCase")]
pub async fn create_mode_chat(
    path: String,
    chat_id: Option<String>,
    message: String,
    target_count: Option<u32>,
    history: Option<Vec<CreateChatMessage>>,
    existing_brief: Option<PackBrief>,
) -> Result<Value, String> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("message is empty".into());
    }

    let project_dir = manifest_parent(&path)?;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let mc = manifest.minecraft.version.clone();
    let loader = loader_kind_slug(&manifest.loader.kind).to_string();
    let target = target_count.unwrap_or(80).clamp(40, 120);

    let mut trends_hint = String::new();
    // Local pairs always; Supabase/hub pairs when swarm is on.
    if let Ok(trends) = crate::swarm_api::get_creation_trends(path.clone(), Some(20)).await {
        if let Some(hint) = trends.get("promptHint").and_then(|v| v.as_str()) {
            if !hint.trim().is_empty() && !hint.contains("(no stats yet") {
                trends_hint = format!(
                    "\n\nOptional co-occurrence hints (local + community when available):\n{hint}"
                );
            }
        }
    }

    let system = format!(
        "{CREATE_MODE_SYSTEM_PROMPT}\n\nProject context: Minecraft {mc}, loader {loader}, preferred targetCount {target}.{trends_hint}"
    );

    let mut messages: Vec<Value> = Vec::new();
    if let Some(hist) = &history {
        for m in hist {
            if m.role == "user" || m.role == "assistant" {
                messages.push(json!({"role": m.role, "content": m.content}));
            }
        }
    }
    let mut user_content = message.clone();
    if let Some(brief) = &existing_brief {
        if let Ok(s) = serde_json::to_string_pretty(brief) {
            user_content = format!(
                "{user_content}\n\nCurrent PackBrief (refine this if appropriate):\n{s}"
            );
        }
    }
    messages.push(json!({"role": "user", "content": user_content}));

    let settings = crate::integrations::read_settings().ai;
    let raw = crate::integrations::call_ai_messages(&settings, &system, &messages, true).await?;
    let raw_str = serde_json::to_string(&raw).unwrap_or_else(|_| "{}".into());
    let parsed = parse_create_mode_ai_response(&raw_str)
        .or_else(|_| {
            // call_ai_messages already returns Value; serialize fields if present
            let reply = raw
                .get("reply")
                .and_then(|v| v.as_str())
                .unwrap_or("Here is a pack plan.")
                .to_string();
            let brief = raw
                .get("brief")
                .cloned()
                .and_then(|v| serde_json::from_value::<PackBrief>(v).ok());
            if brief.is_none() && raw.get("title").is_some() {
                serde_json::from_value::<PackBrief>(raw.clone())
                    .map(|b| tuffbox_core::create_mode::CreateModeAiResponse {
                        reply,
                        brief: Some(b),
                    })
                    .map_err(|e| e.to_string())
            } else {
                Ok(tuffbox_core::create_mode::CreateModeAiResponse { reply, brief })
            }
        })
        .map_err(|e| e)?;

    let brief = parsed.brief.map(|b| ensure_brief_from_manifest(b, &manifest));

    // Persist chat session
    let id = chat_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(new_chat_id);
    let mut session = load_create_chat_file(&project_dir, &id).unwrap_or_else(|_| CreateChatSession {
        id: id.clone(),
        title: brief
            .as_ref()
            .map(|b| b.title.clone())
            .unwrap_or_else(|| {
                message.chars().take(48).collect::<String>()
            }),
        messages: history.unwrap_or_default(),
        draft: None,
        updated_at: now_iso(),
    });
    session.messages.push(CreateChatMessage {
        role: "user".into(),
        content: message,
        created_at: Some(now_iso()),
    });
    session.messages.push(CreateChatMessage {
        role: "assistant".into(),
        content: parsed.reply.clone(),
        created_at: Some(now_iso()),
    });
    if let Some(b) = &brief {
        if session.title == "New chat" || session.title.is_empty() {
            session.title = b.title.clone();
        }
    }
    session.updated_at = now_iso();
    save_create_chat_file(&project_dir, &session)?;

    Ok(json!({
        "chatId": id,
        "reply": parsed.reply,
        "brief": brief,
        "session": session,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn assemble_pack_draft(
    app: AppHandle,
    path: String,
    brief: PackBrief,
) -> Result<PackDraft, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let brief = ensure_brief_from_manifest(brief, &manifest);
        let installed = tuffbox_core::create_mode::installed_mod_keys(&manifest);
        let searcher = LiveModrinthSearch(ModrinthProvider::new());
        let app2 = app.clone();
        let mut progress =
            |phase: &str, done: usize, total: usize, current: &str| {
                emit_create_progress(&app2, phase, done, total, current);
            };
        run_assemble_pack_draft(
            &searcher,
            AssembleOptions {
                brief: &brief,
                installed_ids: installed,
                max_pages_per_category: 3,
                page_size: 100,
                on_progress: Some(&mut progress),
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn preview_pack_draft(
    path: String,
    draft: PackDraft,
    sample_limit: Option<u32>,
) -> Result<Value, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let provider = ModrinthProvider::new();
        let loader = loader_kind_slug(&manifest.loader.kind).to_string();
        let limit = sample_limit.unwrap_or(draft.mods.len() as u32) as usize;
        let mut ok = 0usize;
        let mut failures: Vec<Value> = Vec::new();
        for m in draft.mods.iter().take(limit) {
            let id = if m.project_id.is_empty() {
                m.slug.as_str()
            } else {
                m.project_id.as_str()
            };
            let query = tuffbox_core::ProviderSearchQuery {
                minecraft_version: Some(manifest.minecraft.version.clone()),
                loader: Some(loader.clone()),
                ..Default::default()
            };
            match provider.get_versions(id, &query) {
                Ok(versions) if !versions.is_empty() => ok += 1,
                Ok(_) => failures.push(json!({
                    "slug": m.slug,
                    "projectId": m.project_id,
                    "error": "no compatible version",
                })),
                Err(e) => failures.push(json!({
                    "slug": m.slug,
                    "projectId": m.project_id,
                    "error": e.to_string(),
                })),
            }
        }
        Ok(json!({
            "checked": limit.min(draft.mods.len()),
            "ok": ok,
            "failures": failures,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn install_pack_draft(
    app: AppHandle,
    path: String,
    draft: PackDraft,
    confirmed: bool,
    side: Option<String>,
) -> Result<Value, String> {
    if !confirmed {
        return Err("install requires explicit confirm".into());
    }
    if draft.mods.is_empty() {
        return Err("pack draft has no mods to install".into());
    }

    let side = side.unwrap_or_else(|| "both".into());
    // Prefer Modrinth slugs for co-occurrence (readable for AI / community).
    let cooccur_ids: Vec<String> = draft
        .mods
        .iter()
        .map(|m| {
            if !m.slug.is_empty() {
                m.slug.clone()
            } else if !m.project_id.is_empty() {
                m.project_id.clone()
            } else {
                m.name.clone()
            }
        })
        .collect();
    let mod_ids: Vec<String> = draft
        .mods
        .iter()
        .map(|m| {
            if !m.project_id.is_empty() {
                m.project_id.clone()
            } else {
                m.slug.clone()
            }
        })
        .collect();
    let total = mod_ids.len();
    let path_for_stats = path.clone();

    let result = tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "create-mode-pack-install").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let mut all_installed = Vec::new();
        const CHUNK: usize = 25;
        let chunks: Vec<_> = mod_ids.chunks(CHUNK).collect();
        let chunk_count = chunks.len();

        for (i, chunk) in chunks.into_iter().enumerate() {
            emit_create_progress(
                &app,
                "install",
                i * CHUNK,
                total,
                &format!("batch {}/{}", i + 1, chunk_count),
            );
            let installed =
                install_modrinth_with_dependencies_rounds(&mut manifest, chunk, &side, 200);
            all_installed.extend(installed);
        }

        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods_tracked(&app, &manifest_path, &manifest, None, true);
        emit_create_progress(&app, "install", total, total, "done");

        Ok::<_, String>(json!({
            "installedCount": all_installed.len(),
            "installed": all_installed,
            "requested": total,
        }))
    })
    .await
    .map_err(|e| e.to_string())??;

    // Fire-and-forget local + Supabase co-occurrence (does not fail install).
    let stats = crate::swarm_api::record_and_upload_cooccurrence(
        &path_for_stats,
        &cooccur_ids,
        "create_mode_install",
    )
    .await
    .unwrap_or_else(|e| json!({ "local": false, "uploaded": false, "uploadError": e }));

    Ok(json!({
        "installedCount": result.get("installedCount").cloned().unwrap_or(json!(0)),
        "installed": result.get("installed").cloned().unwrap_or(json!([])),
        "requested": result.get("requested").cloned().unwrap_or(json!(0)),
        "cooccurrence": stats,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_create_chats(path: String) -> Result<Vec<CreateChatSession>, String> {
    let project_dir = manifest_parent(&path)?;
    list_create_chat_files(&project_dir)
}

#[tauri::command(rename_all = "camelCase")]
pub fn save_create_chat(path: String, session: CreateChatSession) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    save_create_chat_file(&project_dir, &session)?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub fn load_create_chat(path: String, chat_id: String) -> Result<CreateChatSession, String> {
    let project_dir = manifest_parent(&path)?;
    load_create_chat_file(&project_dir, &chat_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn delete_create_chat(path: String, chat_id: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    delete_create_chat_file(&project_dir, &chat_id)
}

#[tauri::command(rename_all = "camelCase")]
pub fn new_create_chat(path: String, title: Option<String>) -> Result<CreateChatSession, String> {
    let project_dir = manifest_parent(&path)?;
    let session = CreateChatSession {
        id: new_chat_id(),
        title: title.unwrap_or_else(|| "New chat".into()),
        messages: vec![],
        draft: None,
        updated_at: now_iso(),
    };
    save_create_chat_file(&project_dir, &session)?;
    Ok(session)
}
