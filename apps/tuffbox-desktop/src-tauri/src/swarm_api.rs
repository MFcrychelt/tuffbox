//! TuffSwarm desktop commands: pending plans, capsules, co-occurrence, share prompt.

use crate::integrations;
use serde_json::json;
use std::path::{Path, PathBuf};
use tuffbox_core::action_plan::ActionPlan;
use tuffbox_core::crash_kb::{AuthorCaseInput, CrashCase};
use tuffbox_core::swarm::{
    clear_pending_action_plan, format_cooccurrence_for_prompt, load_pending_action_plan,
    maybe_write_pending_from_score, record_mod_set_cooccurrence, top_cooccurrence_pairs,
    write_pending_action_plan, ExperienceCapsule, ModPairStat, STRONG_MATCH_THRESHOLD,
};
use tuffbox_core::{ProjectManifest, Snapshot, SnapshotMeta, SnapshotStore};

fn manifest_parent(path: &str) -> Result<PathBuf, String> {
    PathBuf::from(path)
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "manifest path has no parent".into())
}

fn last_crash_fix_path(project_dir: &Path) -> PathBuf {
    project_dir
        .join(".tuffbox")
        .join("swarm")
        .join("last_crash_fix.json")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastCrashFixMarker {
    pub snapshot_id: String,
    pub fingerprint_key: String,
    pub plan_source: Option<String>,
    pub matched_case_ids: Vec<String>,
    pub human_explanation: String,
    pub actions: Vec<tuffbox_core::action_plan::LauncherAction>,
    pub created_at: String,
    #[serde(default)]
    pub shared: bool,
}

pub fn write_last_crash_fix_marker(
    project_dir: &Path,
    snapshot: &Snapshot,
    plan: &ActionPlan,
    fingerprint_key: &str,
) -> Result<(), String> {
    let marker = LastCrashFixMarker {
        snapshot_id: snapshot.id.clone(),
        fingerprint_key: fingerprint_key.to_string(),
        plan_source: plan.source.clone(),
        matched_case_ids: plan.matched_case_ids.clone(),
        human_explanation: plan.human_explanation.clone(),
        actions: plan.actions.clone(),
        created_at: tuffbox_core::time_util::rfc3339_now(),
        shared: false,
    };
    let path = last_crash_fix_path(project_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&marker).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub fn auto_snapshot_crash_fix(
    manifest_path: &Path,
    plan: &ActionPlan,
    fingerprint_key: Option<&str>,
) -> Result<Snapshot, String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest path has no parent".to_string())?;
    let lockfile_path = manifest_path.with_extension("lock.json");
    let lockfile_path = if lockfile_path.exists() {
        Some(lockfile_path)
    } else {
        None
    };
    let fp = fingerprint_key.unwrap_or("unknown");
    let fp_prefix: String = fp.chars().take(24).collect();
    let name = format!("auto-before-crash-fix-{fp_prefix}");
    let reason = format!(
        "Auto snapshot before crash fix ({})",
        plan.source.as_deref().unwrap_or("manual")
    );
    let meta = SnapshotMeta {
        tags: vec!["crash_fix".into()],
        crash_fingerprint_key: fingerprint_key.map(|s| s.to_string()),
        report_id: None,
        plan_source: plan.source.clone().or_else(|| Some("manual".into())),
        matched_case_ids: plan.matched_case_ids.clone(),
    };
    let store = SnapshotStore::new(project_dir);
    let snapshot = store
        .create_with_meta(
            &name,
            &reason,
            manifest_path,
            lockfile_path.as_ref(),
            &[] as &[std::path::PathBuf],
            meta,
        )
        .map_err(|e| e.to_string())?;
    let _ = write_last_crash_fix_marker(project_dir, &snapshot, plan, fp);
    Ok(snapshot)
}

/// Heuristic Crash Assistant apply path — still tags snapshot as crash_fix.
pub fn auto_snapshot_crash_fix_heuristic(
    manifest_path: &Path,
    fingerprint_key: Option<&str>,
    summary: &str,
    report_id: Option<&str>,
) -> Result<Snapshot, String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest path has no parent".to_string())?;
    let lockfile_path = manifest_path.with_extension("lock.json");
    let lockfile_path = if lockfile_path.exists() {
        Some(lockfile_path)
    } else {
        None
    };
    let fp = fingerprint_key.unwrap_or("unknown");
    let fp_prefix: String = fp.chars().take(24).collect();
    let name = format!("auto-before-crash-fix-{fp_prefix}");
    let reason = format!("Auto snapshot before crash fix (manual): {summary}");
    let meta = SnapshotMeta {
        tags: vec!["crash_fix".into()],
        crash_fingerprint_key: fingerprint_key.map(|s| s.to_string()),
        report_id: report_id.map(|s| s.to_string()),
        plan_source: Some("manual".into()),
        matched_case_ids: Vec::new(),
    };
    let store = SnapshotStore::new(project_dir);
    let snapshot = store
        .create_with_meta(
            &name,
            &reason,
            manifest_path,
            lockfile_path.as_ref(),
            &[] as &[std::path::PathBuf],
            meta,
        )
        .map_err(|e| e.to_string())?;
    let plan = ActionPlan {
        schema_version: tuffbox_core::action_plan::ACTION_PLAN_SCHEMA_VERSION,
        human_explanation: summary.to_string(),
        confidence: 0.6,
        suspected_mods: Vec::new(),
        needs_user_review: true,
        source: Some("manual".into()),
        matched_case_ids: Vec::new(),
        actions: Vec::new(),
        additional_context: None,
    };
    let _ = write_last_crash_fix_marker(project_dir, &snapshot, &plan, fp);
    Ok(snapshot)
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_pending_action_plan(path: String) -> Result<Option<ActionPlan>, String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;
    load_pending_action_plan(&project_dir)
}

#[tauri::command(rename_all = "camelCase")]
pub fn clear_pending_network_plan(path: String) -> Result<(), String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;
    clear_pending_action_plan(&project_dir)
}

#[tauri::command(rename_all = "camelCase")]
pub fn write_pending_network_plan(path: String, plan: ActionPlan) -> Result<String, String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;
    let out = write_pending_action_plan(&project_dir, &plan)?;
    Ok(out.to_string_lossy().to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_share_prompt_after_launch(path: String) -> Result<Option<LastCrashFixMarker>, String> {
    let swarm = integrations::swarm_settings();
    if !swarm.enabled || !swarm.share_prompts_enabled {
        return Ok(None);
    }
    let project_dir = manifest_parent(&path)?;
    let marker_path = last_crash_fix_path(&project_dir);
    if !marker_path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&marker_path).map_err(|e| e.to_string())?;
    let marker: LastCrashFixMarker =
        serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if marker.shared {
        return Ok(None);
    }
    Ok(Some(marker))
}

#[tauri::command(rename_all = "camelCase")]
pub fn dismiss_share_prompt(path: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let marker_path = last_crash_fix_path(&project_dir);
    if !marker_path.is_file() {
        return Ok(());
    }
    let raw = std::fs::read_to_string(&marker_path).map_err(|e| e.to_string())?;
    let mut marker: LastCrashFixMarker =
        serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    marker.shared = true;
    std::fs::write(
        marker_path,
        serde_json::to_vec_pretty(&marker).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn publish_experience_capsule(
    path: String,
    fingerprint_key: Option<String>,
    human_explanation: Option<String>,
    actions: Option<Vec<tuffbox_core::action_plan::LauncherAction>>,
) -> Result<serde_json::Value, String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;

    let marker_path = last_crash_fix_path(&project_dir);
    let marker: Option<LastCrashFixMarker> = std::fs::read_to_string(&marker_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());

    let fp_key = fingerprint_key
        .or_else(|| marker.as_ref().map(|m| m.fingerprint_key.clone()))
        .unwrap_or_else(|| "unknown".into());
    let solution = human_explanation
        .or_else(|| marker.as_ref().map(|m| m.human_explanation.clone()))
        .unwrap_or_else(|| "Shared crash fix".into());
    let launcher_actions = actions.unwrap_or_else(|| {
        marker
            .as_ref()
            .map(|m| m.actions.clone())
            .unwrap_or_default()
    });

    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
    let crash = std::fs::read_to_string(project_dir.join("logs").join("latest.log")).unwrap_or_default();
    // Prefer a real fingerprint from logs when available; never publish the log itself.
    let mut fingerprint = tuffbox_core::crash_kb::fingerprint_from_text(
        &crash,
        &manifest.minecraft.version,
        &loader,
    );
    if fingerprint.key.trim().is_empty() || fingerprint.key == "||||" {
        fingerprint = tuffbox_core::crash_kb::CrashFingerprint {
            exception: fp_key.clone(),
            frames: Vec::new(),
            mod_file: None,
            mixin: None,
            mc_major: fingerprint.mc_major,
            loader: loader.clone(),
            key: fp_key.clone(),
        };
    } else if !fp_key.is_empty() && fp_key != "unknown" {
        // Keep marker key if it was the one used for the fix.
        fingerprint.key = fp_key;
    }

    let case = CrashCase {
        id: format!("capsule-{}", tuffbox_core::time_util::compact_now()),
        fingerprint: fingerprint.clone(),
        symptoms: Vec::new(),
        suspected_mods: Vec::new(),
        solution: solution.clone(),
        actions: Vec::new(),
        launcher_actions: launcher_actions.clone(),
        notes: None,
        source: "authored".into(),
        success_count: 1,
        fail_count: 0,
    };
    let capsule = ExperienceCapsule::from_crash_case(&case).sanitized_for_network();
    let public = capsule.to_public_json();

    // Project-local authored export (pack author).
    let _ = tuffbox_core::crash_kb::save_authored_case(
        &project_dir,
        AuthorCaseInput {
            id: Some(case.id.clone()),
            fingerprint: fingerprint.clone(),
            solution: solution.clone(),
            symptoms: Vec::new(),
            suspected_mods: Vec::new(),
            launcher_actions: launcher_actions.clone(),
            actions: Vec::new(),
            notes: None,
        },
    );

    // Machine-wide durable library — other projects on this PC keep the fix.
    let global = integrations::global_capsule_library();
    let stored_global = global.publish(&capsule)?;

    // Phase C: prefer P2P control HTTP when healthy; hub remains bootstrap/fallback.
    // Publish to every available transport so hub stays seeded for non-P2P peers.
    let bases = crate::swarm_node::capsule_transport_bases().await;
    let mut published_remote = false;
    let mut remote_results = Vec::new();
    let mut remote_error: Option<String> = None;
    for base in &bases {
        let token = crate::swarm_node::auth_token_for_base(base);
        match tuffbox_core::crash_remote::publish_capsule_async(&base, token.as_deref(), &public)
            .await
        {
            Ok(body) => {
                published_remote = true;
                remote_results.push(json!({ "base": base, "ok": true, "body": body }));
            }
            Err(e) => {
                remote_results.push(json!({ "base": base, "ok": false, "error": e.clone() }));
                if remote_error.is_none() {
                    remote_error = Some(e);
                }
            }
        }
    }
    if published_remote {
        remote_error = None;
    }

    let _ = dismiss_share_prompt(path);
    Ok(json!({
        "published": published_remote,
        "sharedLocal": true,
        "globalPath": global.path().to_string_lossy(),
        "capsuleId": stored_global.id,
        "fingerprintKey": stored_global.fingerprint.key,
        "remote": remote_results,
        "error": remote_error,
        "privacy": { "rawLogs": false, "notesIncluded": false },
        "capsule": public,
        "hubConfigured": integrations::swarm_network_base().is_some(),
        "p2pConfigured": integrations::swarm_settings().p2p_enabled,
        "transportBases": bases,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub fn record_project_cooccurrence(path: String) -> Result<(), String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
    let ids: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    record_mod_set_cooccurrence(
        &project_dir,
        &ids,
        &manifest.minecraft.version,
        &loader,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_local_cooccurrence(path: String, limit: Option<u32>) -> Result<Vec<ModPairStat>, String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;
    Ok(top_cooccurrence_pairs(
        &project_dir,
        limit.unwrap_or(25) as usize,
    ))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_creation_trends(
    path: String,
    limit: Option<u32>,
) -> Result<serde_json::Value, String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;
    let limit = limit.unwrap_or(20) as usize;
    let local = top_cooccurrence_pairs(&project_dir, limit);
    let prompt_hint = format_cooccurrence_for_prompt(&local, limit.min(15));

    let mut network: Option<serde_json::Value> = None;
    // Co-occurrence is hub/KB only (P2P node does not expose this route yet).
    if let Some(endpoint) = integrations::swarm_network_base() {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
        let token = integrations::secret_optional("crash_kb");
        if let Ok(body) = tuffbox_core::crash_remote::fetch_cooccurrence_async(
            &endpoint,
            token.as_deref(),
            &manifest.minecraft.version,
            &loader,
            limit as u32,
        )
        .await
        {
            network = Some(body);
        }
    }

    Ok(json!({
        "localPairs": local,
        "network": network,
        "promptHint": prompt_hint,
        "strongMatchThreshold": STRONG_MATCH_THRESHOLD,
    }))
}

/// Suggest Modrinth slugs from co-occurrence partner mods not yet installed.
#[tauri::command(rename_all = "camelCase")]
pub fn suggest_mods_from_trends(path: String, limit: Option<u32>) -> Result<Vec<String>, String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let installed: std::collections::HashSet<String> = manifest
        .mods
        .iter()
        .map(|m| m.id.to_ascii_lowercase())
        .collect();
    let pairs = top_cooccurrence_pairs(&project_dir, 50);
    let mut scores: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for p in pairs {
        for id in [p.mod_a, p.mod_b] {
            let key = id.to_ascii_lowercase();
            if !installed.contains(&key) {
                *scores.entry(key).or_insert(0) += p.count;
            }
        }
    }
    let mut ranked: Vec<(String, u64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1));
    ranked.truncate(limit.unwrap_or(8) as usize);
    Ok(ranked.into_iter().map(|(id, _)| id).collect())
}

/// After analyze: if swarm on and strong match, persist pending plan.
pub fn maybe_persist_pending_from_plan(
    project_dir: &Path,
    plan: &ActionPlan,
    network_used: bool,
) -> Option<PathBuf> {
    if !integrations::swarm_enabled() || !network_used {
        return None;
    }
    let score = if !plan.matched_case_ids.is_empty() {
        plan.confidence.max(STRONG_MATCH_THRESHOLD)
    } else {
        plan.confidence
    };
    maybe_write_pending_from_score(project_dir, plan, score)
        .ok()
        .flatten()
}
