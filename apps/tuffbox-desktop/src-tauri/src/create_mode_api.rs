//! Create Mode chat / pack assemble Tauri commands.

use crate::{
    auto_snapshot, download_project_mods_tracked, install_curseforge_with_dependencies_rounds,
    install_modrinth_with_dependencies_rounds, manifest_parent, save_manifest,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};
use tuffbox_core::create_mode::{
    assemble_pack_draft as run_assemble_pack_draft, brief_from_prompt,
    delete_create_chat as delete_create_chat_file, list_create_chats as list_create_chat_files,
    load_create_chat as load_create_chat_file, merge_mpi_hints_into_brief, new_chat_id, now_iso,
    parse_create_mode_ai_response, save_create_chat as save_create_chat_file, search_from_brief,
    AssembleOptions, CreateChatMessage, CreateChatSession, LiveCatalogSearch, PackBrief,
    PackDraft, CREATE_MODE_SYSTEM_PROMPT,
};
use tuffbox_core::graph::loader_kind_slug;
use tuffbox_core::mod_suggest::{
    enrich_partners_with_descriptions, merge_partner_stats, partners_from_pairs, resolve_seed_mods,
    soft_boost_partners, CandidateAddon,
};
use tuffbox_core::modpack_index::{format_tags_for_prompt, MpiModHint, MpiSearchQuery};
use tuffbox_core::swarm::ModPairStat;
use tuffbox_core::swarm_supabase::{partners_for_mod_mpi_supabase, partners_for_mod_supabase};
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

/// Pull reply / brief / search from an AI JSON value.
fn parse_ai_value(raw: &Value) -> (String, Option<PackBrief>, Option<MpiSearchQuery>) {
    let reply = raw
        .get("reply")
        .and_then(|v| v.as_str())
        .unwrap_or("Here is a pack plan.")
        .to_string();
    let brief = raw
        .get("brief")
        .cloned()
        .and_then(|v| serde_json::from_value::<PackBrief>(v).ok())
        .or_else(|| {
            if raw.get("title").is_some() {
                serde_json::from_value::<PackBrief>(raw.clone()).ok()
            } else {
                None
            }
        });
    let search = raw
        .get("search")
        .cloned()
        .and_then(|v| serde_json::from_value::<MpiSearchQuery>(v).ok())
        .or_else(|| brief.as_ref().map(search_from_brief));
    (reply, brief, search)
}

fn candidates_to_mpi_hints(candidates: &[CandidateAddon]) -> Vec<MpiModHint> {
    candidates
        .iter()
        .map(|c| MpiModHint {
            name: c.name.clone(),
            slug: c.slug.clone(),
            summary: c.summary.clone(),
            categories: vec![],
            // merge_mpi_hints_into_brief only keeps keyword:/theme-name: sources
            source: format!("keyword:{}", c.source),
        })
        .collect()
}

/// Seed mods + hub/Supabase co-occurrence partners → catalog candidates for PackBrief refine.
/// Prefer hub GET /v1/mods/cooccurrence (via get_creation_trends) so clients never hit MPI.
async fn collect_candidates(
    path: &str,
    search: &MpiSearchQuery,
    mc: &str,
    loader: &str,
) -> Vec<CandidateAddon> {
    let search_owned = search.clone();
    let mc_owned = mc.to_string();
    let loader_owned = loader.to_string();
    let seeds = match tokio::task::spawn_blocking(move || {
        resolve_seed_mods(&search_owned, &mc_owned, &loader_owned)
    })
    .await
    {
        Ok(v) => v,
        Err(_) => Vec::new(),
    };
    if seeds.is_empty() {
        return seeds;
    }

    // 1) Supabase TuffSwarm partners_for_mod — primary for suggestions.
    let mut primary = Vec::new();
    if let (Some(url), Some(key)) = (
        crate::integrations::swarm_supabase_url(),
        crate::integrations::swarm_supabase_anon_key(),
    ) {
        let mut batches = Vec::new();
        for seed in seeds.iter().take(3) {
            if let Ok(batch) = partners_for_mod_supabase(
                &url,
                &key,
                &seed.slug,
                12,
                Some(loader),
                Some(mc),
            )
            .await
            {
                batches.push(batch);
            }
        }
        primary = merge_partner_stats(&batches, 24);

        // Soft boost from separate Modpack Index graph.
        let mut mpi_batches = Vec::new();
        for seed in seeds.iter().take(3) {
            if let Ok(batch) = partners_for_mod_mpi_supabase(
                &url,
                &key,
                &seed.slug,
                12,
                Some(loader),
                Some(mc),
                None,
            )
            .await
            {
                mpi_batches.push(batch);
            }
        }
        let mpi = merge_partner_stats(&mpi_batches, 24);
        if primary.is_empty() {
            primary = mpi;
        } else if primary.len() < 5 {
            primary = merge_partner_stats(&[primary, mpi], 24);
        } else {
            primary = soft_boost_partners(&primary, &mpi, 24);
        }
    }

    // 2) Trends/local pairs — soft boost matches or fallback when SB empty.
    let mut local = Vec::new();
    if let Ok(trends) = crate::swarm_api::get_creation_trends(path.to_string(), Some(40)).await {
        let pairs: Vec<ModPairStat> = trends
            .get("mergedPairs")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let mut batches = Vec::new();
        for seed in seeds.iter().take(3) {
            batches.push(partners_from_pairs(&seed.slug, &pairs, 12));
        }
        local = merge_partner_stats(&batches, 24);
    }
    let partners = soft_boost_partners(&primary, &local, 24);

    let seeds_for_enrich = seeds.clone();
    match tokio::task::spawn_blocking(move || {
        enrich_partners_with_descriptions(&seeds_for_enrich, &partners, 24)
    })
    .await
    {
        Ok(v) => v,
        Err(_) => seeds,
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn create_mode_chat(
    app: AppHandle,
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

    emit_create_progress(&app, "plan", 0, 0, "Calling AI…");

    let tags = format_tags_for_prompt();
    let system = format!(
        "{CREATE_MODE_SYSTEM_PROMPT}\n\n{tags}\n\nProject context: Minecraft {mc}, loader {loader}, preferred targetCount {target}.\n\nImportant: reply, title, and reasons must use the same language as the latest user message (not Chinese unless the user wrote Chinese)."
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
    emit_create_progress(&app, "plan", 0, 0, "Parsing brief…");
    let raw_str = serde_json::to_string(&raw).unwrap_or_else(|_| "{}".into());
    let parsed = match parse_create_mode_ai_response(&raw_str) {
        Ok(p) => p,
        Err(_) => {
            let (reply, brief, search) = parse_ai_value(&raw);
            tuffbox_core::create_mode::CreateModeAiResponse {
                reply,
                brief,
                search,
            }
        }
    };

    let used_prompt_fallback = parsed.brief.is_none();
    let mut brief = match parsed.brief {
        Some(b) => ensure_brief_from_manifest(b, &manifest),
        None => ensure_brief_from_manifest(
            brief_from_prompt(&message, &mc, &loader, target),
            &manifest,
        ),
    };
    // Prefer AI target when present; otherwise keep UI/clamped target on fallback brief.
    if used_prompt_fallback {
        brief.target_count = target;
    }
    let search = parsed
        .search
        .unwrap_or_else(|| search_from_brief(&brief));

    emit_create_progress(&app, "search", 0, 0, "Collecting Modrinth candidates…");
    let candidates = collect_candidates(&path, &search, &mc, &loader).await;
    let hints = candidates_to_mpi_hints(&candidates);
    merge_mpi_hints_into_brief(&mut brief, &hints, 8);
    let brief = ensure_brief_from_manifest(brief, &manifest);

    let mut reply = parsed.reply.trim().to_string();
    if reply.is_empty() {
        reply = if used_prompt_fallback {
            "Built a draft brief from your prompt (AI JSON incomplete).".into()
        } else {
            format!(
                "Pack brief ready: {} ({} must-have from catalog candidates).",
                brief.title,
                brief.must_have.len()
            )
        };
    } else if used_prompt_fallback {
        reply = format!(
            "{reply}\n\n(Note: AI JSON had no PackBrief — filled from your prompt.)"
        );
    }

    emit_create_progress(&app, "plan", 0, 0, "Saving session…");
    let id = chat_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(new_chat_id);
    let mut session = load_create_chat_file(&project_dir, &id).unwrap_or_else(|_| CreateChatSession {
        id: id.clone(),
        title: brief.title.clone(),
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
        content: reply.clone(),
        created_at: Some(now_iso()),
    });
    if session.title == "New chat" || session.title.is_empty() {
        session.title = brief.title.clone();
    }
    session.updated_at = now_iso();
    save_create_chat_file(&project_dir, &session)?;

    Ok(json!({
        "chatId": id,
        "reply": reply,
        "brief": brief,
        "search": search,
        "candidates": candidates,
        "session": session,
    }))
}

/// Deterministic PackBrief from free text (no LLM) — fallback when AI is unavailable.
#[tauri::command(rename_all = "camelCase")]
pub async fn create_mode_quick_brief(
    app: AppHandle,
    path: String,
    chat_id: Option<String>,
    message: String,
    target_count: Option<u32>,
) -> Result<Value, String> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("message is empty".into());
    }

    emit_create_progress(&app, "plan", 0, 0, "Building brief…");

    let project_dir = manifest_parent(&path)?;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let mc = manifest.minecraft.version.clone();
    let loader = loader_kind_slug(&manifest.loader.kind).to_string();
    let target = target_count.unwrap_or(80).clamp(40, 120);

    let mut brief = ensure_brief_from_manifest(
        brief_from_prompt(&message, &mc, &loader, target),
        &manifest,
    );
    let search = search_from_brief(&brief);
    emit_create_progress(&app, "search", 0, 0, "Collecting Modrinth candidates…");
    let candidates = collect_candidates(&path, &search, &mc, &loader).await;
    let hints = candidates_to_mpi_hints(&candidates);
    merge_mpi_hints_into_brief(&mut brief, &hints, 8);
    let brief = ensure_brief_from_manifest(brief, &manifest);

    let reply = format!(
        "Quick assemble plan: {} ({} mods, {} must-have from catalog). No AI — default category budgets + names from your prompt.",
        brief.title,
        brief.target_count,
        brief.must_have.len()
    );

    emit_create_progress(&app, "plan", 0, 0, "Saving session…");
    let id = chat_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(new_chat_id);
    let mut session = load_create_chat_file(&project_dir, &id).unwrap_or_else(|_| CreateChatSession {
        id: id.clone(),
        title: brief.title.clone(),
        messages: vec![],
        draft: None,
        updated_at: now_iso(),
    });
    session.messages.push(CreateChatMessage {
        role: "user".into(),
        content: message.clone(),
        created_at: Some(now_iso()),
    });
    session.messages.push(CreateChatMessage {
        role: "assistant".into(),
        content: reply.clone(),
        created_at: Some(now_iso()),
    });
    if session.title == "New chat" || session.title.is_empty() {
        session.title = brief.title.clone();
    }
    session.updated_at = now_iso();
    save_create_chat_file(&project_dir, &session)?;

    Ok(json!({
        "chatId": id,
        "reply": reply,
        "brief": brief,
        "search": search,
        "candidates": candidates,
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
        let searcher = LiveCatalogSearch::new();
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

    let mr_ids: Vec<String> = draft
        .mods
        .iter()
        .filter(|m| m.provider != "curseforge")
        .map(|m| {
            if !m.project_id.is_empty() {
                m.project_id.clone()
            } else {
                m.slug.clone()
            }
        })
        .collect();
    let cf_ids: Vec<String> = draft
        .mods
        .iter()
        .filter(|m| m.provider == "curseforge")
        .map(|m| m.project_id.clone())
        .filter(|id| !id.is_empty())
        .collect();
    let total = mr_ids.len() + cf_ids.len();
    let path_for_stats = path.clone();

    let result = tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "create-mode-pack-install").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let mut all_installed = Vec::new();
        let mut done = 0usize;

        for cf_id in &cf_ids {
            emit_create_progress(
                &app,
                "install",
                done,
                total,
                &format!("curseforge:{cf_id}"),
            );
            match install_curseforge_with_dependencies_rounds(
                &mut manifest,
                &[cf_id.clone()],
                &side,
                50,
            ) {
                Ok(ids) => all_installed.extend(ids),
                Err(e) => {
                    // Continue remaining mods; surface later via counts.
                    let _ = e;
                }
            }
            done += 1;
        }

        const CHUNK: usize = 25;
        let chunks: Vec<_> = mr_ids.chunks(CHUNK).collect();
        let chunk_count = chunks.len().max(1);
        for (i, chunk) in chunks.into_iter().enumerate() {
            emit_create_progress(
                &app,
                "install",
                done + i * CHUNK,
                total,
                &format!("modrinth batch {}/{}", i + 1, chunk_count),
            );
            let installed =
                install_modrinth_with_dependencies_rounds(&mut manifest, chunk, &side, 200)?;
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

/// Resolve Modpack Index hints (blocking MPI HTTP from this process).
/// Prefer hub-seeded co-occurrence for Create Mode; this command is discovery-only
/// and should not be used as the default pack-builder data path (MPI privacy).
#[tauri::command(rename_all = "camelCase")]
pub async fn resolve_modpack_index_search(
    search: MpiSearchQuery,
    per_source: Option<u32>,
) -> Result<Vec<MpiModHint>, String> {
    let per = per_source.unwrap_or(8).clamp(1, 40) as usize;
    tokio::task::spawn_blocking(move || {
        tuffbox_core::modpack_index::gather_search_hints(&search, per)
    })
    .await
    .map_err(|e| e.to_string())?
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
