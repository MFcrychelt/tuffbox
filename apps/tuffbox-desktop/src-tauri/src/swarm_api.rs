//! TuffBox desktop commands: pending plans, capsules, co-occurrence, share prompt,
//! and post-resolution distill (AI → Confirm → publish).

use crate::integrations;
use serde_json::json;
use std::path::{Path, PathBuf};
use tuffbox_core::action_plan::ActionPlan;
use tuffbox_core::crash_kb::{AuthorCaseInput, CrashCase};
use tuffbox_core::swarm::{
    clear_pending_action_plan, format_cooccurrence_for_prompt, load_pending_action_plan,
    maybe_write_pending_from_score, merge_cooccurrence_pairs, normalize_mod_id_list,
    record_mod_set_cooccurrence, top_cooccurrence_pairs, write_pending_action_plan,
    ExperienceCapsule, ModPairStat, STRONG_MATCH_THRESHOLD,
};
use tuffbox_core::{ProjectManifest, Snapshot, SnapshotMeta, SnapshotStore};
use tauri::Emitter;

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
    /// Unix epoch seconds when the fix was applied — used to require a *newer*
    /// healthy `latest.log` before confirming resolution.
    #[serde(default)]
    pub created_at_unix: Option<u64>,
    #[serde(default)]
    pub shared: bool,
    /// Set once a successful launch / healthy diagnose confirms the fix worked.
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashResolutionRecord {
    pub id: String,
    pub fingerprint_key: String,
    pub snapshot_id: String,
    pub plan_source: Option<String>,
    pub human_explanation: String,
    pub matched_case_ids: Vec<String>,
    pub actions_summary: Vec<String>,
    pub verified_by: String,
    pub created_at: String,
    pub resolved_at: String,
}

fn resolutions_path(project_dir: &Path) -> PathBuf {
    project_dir
        .join(".tuffbox")
        .join("history")
        .join("resolutions.jsonl")
}

pub fn list_crash_resolutions(project_dir: &Path) -> Result<Vec<CrashResolutionRecord>, String> {
    let path = resolutions_path(project_dir);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(rec) = serde_json::from_str::<CrashResolutionRecord>(line) {
            out.push(rec);
        }
    }
    out.reverse(); // newest first
    Ok(out)
}

fn append_crash_resolution(project_dir: &Path, rec: &CrashResolutionRecord) -> Result<(), String> {
    let path = resolutions_path(project_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let line = serde_json::to_string(rec).map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())
}

fn marker_created_unix(marker: &LastCrashFixMarker) -> Option<u64> {
    marker
        .created_at_unix
        .or_else(|| tuffbox_core::time_util::parse_rfc3339_unix_secs(&marker.created_at))
}

fn file_mtime_secs(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Human-readable one-liner for History (how the user fixed the error).
pub fn format_launcher_action_summary(a: &tuffbox_core::action_plan::LauncherAction) -> String {
    let target = a
        .mod_id
        .as_deref()
        .or(a.project_id.as_deref())
        .or(a.path.as_deref())
        .filter(|s| !s.is_empty())
        .unwrap_or("-");
    match a.op.as_str() {
        "disable_mod" => format!("Disabled {target}"),
        "remove_mod" => format!("Removed {target}"),
        "update_mod" | "change_mod_version" => {
            if let Some(v) = a.version.as_deref().filter(|s| !s.is_empty()) {
                format!("Updated {target} → {v}")
            } else {
                format!("Updated {target}")
            }
        }
        "reinstall_mod" => format!("Reinstalled {target}"),
        "install_mod" => format!("Installed {target}"),
        "edit_config" => format!("Edited config {target}"),
        "raise_memory" => "Raised allocated memory".into(),
        "accept_eula" => "Accepted EULA".into(),
        "change_port" => "Changed server port".into(),
        "auto_java" => "Selected compatible Java".into(),
        "update_loader" => "Updated loader".into(),
        other => {
            if target == "-" {
                other.to_string()
            } else {
                format!("{other} {target}")
            }
        }
    }
}

fn pending_fix_marker_exists(project_dir: &Path) -> bool {
    let path = last_crash_fix_path(project_dir);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(marker) = serde_json::from_str::<LastCrashFixMarker>(&raw) else {
        return false;
    };
    !marker.resolved
}

/// When a crash fix was applied and the game later launches cleanly, record a
/// durable "resolved" history entry so the History tab shows the successful fix.
///
/// Requires `latest.log` to be **newer than the fix marker** and indicate a
/// healthy session — so we don't confirm from a stale pre-crash healthy log,
/// or the empty/mid-boot log right after spawn.
pub fn maybe_confirm_crash_resolution(
    manifest_path: &Path,
    verified_by: &str,
) -> Result<Option<CrashResolutionRecord>, String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest path has no parent".to_string())?;
    let marker_path = last_crash_fix_path(project_dir);
    if !marker_path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&marker_path).map_err(|e| e.to_string())?;
    let mut marker: LastCrashFixMarker =
        serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if marker.resolved {
        return Ok(None);
    }

    let latest = project_dir.join("logs").join("latest.log");
    if !latest.is_file() {
        return Ok(None);
    }
    let Some(marker_secs) = marker_created_unix(&marker) else {
        return Ok(None);
    };
    let Some(log_mtime) = file_mtime_secs(&latest) else {
        return Ok(None);
    };
    // Log must be from a session that started after the user applied the fix.
    if log_mtime <= marker_secs {
        return Ok(None);
    }

    let latest_log = tuffbox_core::process::read_log_tail(&latest, 900).unwrap_or_default();
    if !tuffbox_core::crash::log_indicates_healthy_session(&latest_log) {
        return Ok(None);
    }

    let now = tuffbox_core::time_util::rfc3339_now();
    let mut actions_summary: Vec<String> = marker
        .actions
        .iter()
        .map(format_launcher_action_summary)
        .collect();
    if actions_summary.is_empty() && !marker.human_explanation.trim().is_empty() {
        actions_summary.push(marker.human_explanation.clone());
    }

    let rec = CrashResolutionRecord {
        id: format!(
            "resolved-{}",
            tuffbox_core::time_util::compact_now()
        ),
        fingerprint_key: marker.fingerprint_key.clone(),
        snapshot_id: marker.snapshot_id.clone(),
        plan_source: marker.plan_source.clone(),
        human_explanation: marker.human_explanation.clone(),
        matched_case_ids: marker.matched_case_ids.clone(),
        actions_summary,
        verified_by: verified_by.to_string(),
        created_at: marker.created_at.clone(),
        resolved_at: now.clone(),
    };
    append_crash_resolution(project_dir, &rec)?;

    // Peer soft-verify: if this fix came from the network, confirm matched capsules.
    maybe_spawn_supabase_confirm_votes(&rec);

    // Snapshot so History / ChangeHistory surfaces a crash_resolved card.
    let lockfile_path = manifest_path.with_extension("lock.json");
    let lockfile_path = if lockfile_path.exists() {
        Some(lockfile_path)
    } else {
        None
    };
    let fp_prefix: String = rec.fingerprint_key.chars().take(24).collect();
    let how = if rec.actions_summary.is_empty() {
        rec.human_explanation.clone()
    } else {
        rec.actions_summary.join("; ")
    };
    let meta = SnapshotMeta {
        tags: vec!["crash_resolved".into(), "crash_fix".into()],
        crash_fingerprint_key: Some(rec.fingerprint_key.clone()),
        report_id: None,
        plan_source: rec.plan_source.clone().or_else(|| Some(verified_by.into())),
        matched_case_ids: rec.matched_case_ids.clone(),
    };
    let store = SnapshotStore::new(project_dir);
    let _ = store.create_with_meta(
        format!("crash-resolved-{fp_prefix}"),
        format!(
            "Resolved crash ({verified_by}): {}",
            tuffbox_core::crash_kb::truncate_at_char_boundary(&how, 180)
        ),
        manifest_path,
        lockfile_path.as_ref(),
        &[] as &[std::path::PathBuf],
        meta,
    );

    marker.resolved = true;
    marker.resolved_at = Some(now);
    std::fs::write(
        marker_path,
        serde_json::to_vec_pretty(&marker).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(Some(rec))
}

/// Auto confirm-after-success is disabled: votes require a signed-in Supabase user JWT.
fn maybe_spawn_supabase_confirm_votes(_rec: &CrashResolutionRecord) {
    // Voting is gated on Crash Votes auth (access_token). No headless device vote path.
}

/// Whether swarm share/distill UI should run after a verified resolution.
pub fn should_offer_distill() -> bool {
    let swarm = integrations::swarm_settings();
    swarm.enabled && swarm.share_prompts_enabled
}

/// Notify the UI to open the distill → Confirm dialog (no network write yet).
pub fn emit_distill_resolution(
    app: &tauri::AppHandle,
    manifest_path: &str,
    rec: &CrashResolutionRecord,
) {
    if !should_offer_distill() {
        return;
    }
    let _ = app.emit(
        "tuffbox:distill-resolution",
        json!({
            "path": manifest_path,
            "resolution": rec,
        }),
    );
}

/// Poll `latest.log` after launch until a post-fix healthy session appears
/// (or the pending marker disappears / times out).
fn spawn_crash_resolution_watcher(app: tauri::AppHandle, manifest_path: PathBuf) {
    std::thread::Builder::new()
        .name("tuffbox-crash-resolution".into())
        .spawn(move || {
            let Some(project_dir) = manifest_path.parent().map(|p| p.to_path_buf()) else {
                return;
            };
            let path_str = manifest_path.to_string_lossy().to_string();
            // ~10 minutes: Minecraft can take a while on first boot / heavy packs.
            for _ in 0..120 {
                std::thread::sleep(std::time::Duration::from_secs(5));
                if !pending_fix_marker_exists(&project_dir) {
                    return;
                }
                match maybe_confirm_crash_resolution(&manifest_path, "successful_launch") {
                    Ok(Some(rec)) => {
                        emit_distill_resolution(&app, &path_str, &rec);
                        return;
                    }
                    Ok(None) | Err(_) => {}
                }
            }
        })
        .ok();
}

#[tauri::command(rename_all = "camelCase")]
pub fn confirm_crash_resolution_after_launch(
    app: tauri::AppHandle,
    path: String,
) -> Result<Option<CrashResolutionRecord>, String> {
    let manifest_path = PathBuf::from(&path);
    // Immediate attempt (game already healthy / relaunch into existing session).
    if let Some(rec) = maybe_confirm_crash_resolution(&manifest_path, "successful_launch")? {
        emit_distill_resolution(&app, &path, &rec);
        return Ok(Some(rec));
    }
    // Otherwise watch until latest.log is rewritten with a healthy post-fix session.
    if pending_fix_marker_exists(
        manifest_path
            .parent()
            .ok_or_else(|| "manifest path has no parent".to_string())?,
    ) {
        spawn_crash_resolution_watcher(app, manifest_path);
    }
    Ok(None)
}

#[tauri::command(rename_all = "camelCase")]
pub fn confirm_crash_resolution_from_diagnose(
    app: tauri::AppHandle,
    path: String,
) -> Result<Option<CrashResolutionRecord>, String> {
    let manifest_path = PathBuf::from(&path);
    let rec = maybe_confirm_crash_resolution(&manifest_path, "diagnose_healthy")?;
    if let Some(ref r) = rec {
        emit_distill_resolution(&app, &path, r);
    }
    Ok(rec)
}

fn resolve_fingerprint_key(manifest_path: &Path) -> String {
    let Some(project_dir) = manifest_path.parent() else {
        return "unknown".into();
    };
    let Ok(manifest) = ProjectManifest::load_from_path(manifest_path) else {
        return "unknown".into();
    };
    let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
    let mut text = String::new();
    // Prefer the newest crash-report (what the user was fixing); fall back to latest.log.
    let reports_dir = project_dir.join("crash-reports");
    if let Ok(entries) = std::fs::read_dir(&reports_dir) {
        let mut newest: Option<(u64, PathBuf)> = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(mtime) = file_mtime_secs(&path) else {
                continue;
            };
            if newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
                newest = Some((mtime, path));
            }
        }
        if let Some((_, path)) = newest {
            text = std::fs::read_to_string(path).unwrap_or_default();
        }
    }
    if text.trim().is_empty() {
        text = std::fs::read_to_string(project_dir.join("logs").join("latest.log"))
            .unwrap_or_default();
    }
    let fp = tuffbox_core::crash_kb::fingerprint_from_text(
        &text,
        &manifest.minecraft.version,
        &loader,
    );
    if fp.key.trim().is_empty() {
        "unknown".into()
    } else {
        fp.key
    }
}

/// Record a pending crash-fix attempt from Diagnose one-click actions
/// (disable / raise memory / etc.) so a later healthy relaunch can confirm
/// *how* the user resolved the error.
pub fn record_user_fix_attempt(
    manifest_path: &Path,
    source: &str,
    human_explanation: &str,
    actions: Vec<tuffbox_core::action_plan::LauncherAction>,
    fingerprint_key: Option<&str>,
) -> Result<(), String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest path has no parent".to_string())?;
    let fp = fingerprint_key
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| resolve_fingerprint_key(manifest_path));
    let existing_snapshot_id = std::fs::read_to_string(last_crash_fix_path(project_dir))
        .ok()
        .and_then(|raw| serde_json::from_str::<LastCrashFixMarker>(&raw).ok())
        .filter(|m| !m.resolved)
        .map(|m| m.snapshot_id);
    let plan = ActionPlan {
        schema_version: tuffbox_core::action_plan::ACTION_PLAN_SCHEMA_VERSION,
        human_explanation: human_explanation.to_string(),
        confidence: 0.7,
        suspected_mods: Vec::new(),
        needs_user_review: false,
        source: Some(source.to_string()),
        matched_case_ids: Vec::new(),
        actions,
        additional_context: None,
    };
    let snapshot = Snapshot {
        id: existing_snapshot_id
            .unwrap_or_else(|| format!("user-fix-{}", tuffbox_core::time_util::compact_now())),
        name: format!("crash-fix-{source}"),
        created_at: tuffbox_core::time_util::rfc3339_now(),
        reason: human_explanation.to_string(),
        manifest_path: manifest_path.to_path_buf(),
        lockfile_path: None,
        changed_files: Vec::new(),
        tags: vec!["crash_fix".into()],
        crash_fingerprint_key: Some(fp.clone()),
        report_id: None,
        plan_source: Some(source.to_string()),
        matched_case_ids: Vec::new(),
    };
    write_last_crash_fix_marker(project_dir, &snapshot, &plan, &fp)
}

pub fn write_last_crash_fix_marker(
    project_dir: &Path,
    snapshot: &Snapshot,
    plan: &ActionPlan,
    fingerprint_key: &str,
) -> Result<(), String> {
    let now_unix = tuffbox_core::time_util::unix_now_secs();
    let marker = LastCrashFixMarker {
        snapshot_id: snapshot.id.clone(),
        fingerprint_key: fingerprint_key.to_string(),
        plan_source: plan.source.clone(),
        matched_case_ids: plan.matched_case_ids.clone(),
        human_explanation: plan.human_explanation.clone(),
        actions: plan.actions.clone(),
        created_at: tuffbox_core::time_util::rfc3339_now(),
        created_at_unix: Some(now_unix),
        shared: false,
        resolved: false,
        resolved_at: None,
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
    actions: Vec<tuffbox_core::action_plan::LauncherAction>,
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
        actions,
        additional_context: None,
    };
    let _ = write_last_crash_fix_marker(project_dir, &snapshot, &plan, fp);
    Ok(snapshot)
}

/// Map ChangePlan actions into LauncherAction rows for resolution history.
pub fn change_actions_to_launcher(
    actions: &[tuffbox_core::ChangeAction],
) -> Vec<tuffbox_core::action_plan::LauncherAction> {
    actions
        .iter()
        .map(|action| match action {
            tuffbox_core::ChangeAction::InstallMod {
                project_id,
                version,
            } => tuffbox_core::action_plan::LauncherAction {
                op: "install_mod".into(),
                mod_id: None,
                provider: Some("modrinth".into()),
                project_id: Some(project_id.clone()),
                version: version.clone(),
                path: None,
                patch_type: None,
                patch: None,
                reason: None,
                risk: "medium".into(),
            },
            tuffbox_core::ChangeAction::RemoveMod { node_id } => {
                tuffbox_core::action_plan::LauncherAction {
                    op: "remove_mod".into(),
                    mod_id: Some(node_id.0.trim_start_matches("mod:").to_string()),
                    provider: None,
                    project_id: None,
                    version: None,
                    path: None,
                    patch_type: None,
                    patch: None,
                    reason: None,
                    risk: "high".into(),
                }
            }
            tuffbox_core::ChangeAction::DisableMod { node_id } => {
                tuffbox_core::action_plan::LauncherAction {
                    op: "disable_mod".into(),
                    mod_id: Some(node_id.0.trim_start_matches("mod:").to_string()),
                    provider: None,
                    project_id: None,
                    version: None,
                    path: None,
                    patch_type: None,
                    patch: None,
                    reason: None,
                    risk: "medium".into(),
                }
            }
            tuffbox_core::ChangeAction::UpdateMod {
                node_id,
                target_version,
            } => tuffbox_core::action_plan::LauncherAction {
                op: "update_mod".into(),
                mod_id: Some(node_id.0.trim_start_matches("mod:").to_string()),
                provider: None,
                project_id: None,
                version: Some(target_version.clone()),
                path: None,
                patch_type: None,
                patch: None,
                reason: None,
                risk: "medium".into(),
            },
            tuffbox_core::ChangeAction::EditConfig { path, patch } => {
                tuffbox_core::action_plan::LauncherAction {
                    op: "edit_config".into(),
                    mod_id: None,
                    provider: None,
                    project_id: None,
                    version: None,
                    path: Some(path.clone()),
                    patch_type: None,
                    patch: Some(serde_json::Value::String(patch.clone())),
                    reason: None,
                    risk: "low".into(),
                }
            }
        })
        .collect()
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
pub async fn list_community_crash_capsules(
    status: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<tuffbox_core::swarm_supabase::CommunityCapsuleCard>, String> {
    integrations::require_swarm_enabled()?;
    if !integrations::swarm_supabase_configured() {
        return Err("Community Supabase backend is not available".into());
    }
    let url = integrations::swarm_supabase_url().unwrap();
    let anon = integrations::swarm_supabase_anon_key().unwrap();
    tuffbox_core::swarm_supabase::list_community_capsules_supabase(
        &url,
        &anon,
        status.as_deref(),
        limit.unwrap_or(48),
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn vote_community_crash_capsule(
    content_hash: String,
    vote: String,
    access_token: String,
) -> Result<serde_json::Value, String> {
    integrations::require_swarm_enabled()?;
    if !integrations::swarm_supabase_configured() {
        return Err("Community Supabase backend is not available".into());
    }
    if access_token.trim().is_empty() {
        return Err("login required — register and sign in to vote".into());
    }
    let url = integrations::swarm_supabase_url().unwrap();
    let anon = integrations::swarm_supabase_anon_key().unwrap();
    tuffbox_core::swarm_supabase::vote_capsule_supabase(
        &url,
        &anon,
        &content_hash,
        &vote,
        &access_token,
    )
    .await
}

#[tauri::command(rename_all = "camelCase")]
pub fn propose_community_capsule_plan(
    path: String,
    content_hash: String,
    solution: String,
    actions: Vec<tuffbox_core::action_plan::LauncherAction>,
    matched_id: Option<String>,
) -> Result<String, String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;
    let mut plan = ActionPlan {
        schema_version: tuffbox_core::action_plan::ACTION_PLAN_SCHEMA_VERSION,
        human_explanation: solution,
        confidence: 0.45,
        suspected_mods: actions
            .iter()
            .filter_map(|a| a.mod_id.clone())
            .collect(),
        needs_user_review: true,
        source: Some("swarm".into()),
        matched_case_ids: vec![matched_id.unwrap_or(content_hash)],
        actions,
        additional_context: Some(
            "Proposed from Crash Votes. Confirm in Diagnostics before apply.".into(),
        ),
    };
    let validation = tuffbox_core::action_plan::validate_action_plan(&plan);
    if !validation.ok {
        return Err(format!(
            "invalid plan: {}",
            validation.errors.join("; ")
        ));
    }
    plan.needs_user_review = true;
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
    // Soft-verify gate: only after a verified healthy session (resolved).
    if !marker.resolved {
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
            blame_mod_ids: Vec::new(),
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
    let mut capsule = ExperienceCapsule::from_crash_case(&case).sanitized_for_network();
    // Soft-sign with persistent device key (required for Supabase; also helps hub/P2P).
    let device_id = match tuffbox_core::swarm::sign_capsule_with_device_key(&mut capsule) {
        Ok(id) => Some(id),
        Err(e) => {
            // Still allow local-only share if signing fails, but remote Supabase will reject.
            eprintln!("tuffswarm: device sign failed: {e}");
            None
        }
    };
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

    let mut published_remote = false;
    let mut remote_results = Vec::new();
    let mut remote_error: Option<String> = None;

    // Preferred transport: Supabase Edge Function (signed capsules only).
    if integrations::swarm_supabase_configured() {
        let url = integrations::swarm_supabase_url().unwrap();
        let anon = integrations::swarm_supabase_anon_key().unwrap();
        match tuffbox_core::swarm_supabase::publish_capsule_supabase(&url, &anon, &capsule).await {
            Ok(body) => {
                published_remote = true;
                remote_results.push(json!({
                    "transport": "supabase",
                    "ok": true,
                    "body": body,
                }));
            }
            Err(e) => {
                remote_results.push(json!({
                    "transport": "supabase",
                    "ok": false,
                    "error": e.clone(),
                }));
                remote_error = Some(e);
            }
        }
    }

    // Phase C / hub: prefer P2P control HTTP when healthy; hub remains bootstrap/fallback.
    // Publish to every available transport so hub stays seeded for non-P2P peers.
    let bases = crate::swarm_node::capsule_transport_bases().await;
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
        "deviceId": device_id,
        "signed": stored_global.signature.is_some(),
        "remote": remote_results,
        "error": remote_error,
        "privacy": { "rawLogs": false, "notesIncluded": false },
        "capsule": public,
        "supabaseConfigured": integrations::swarm_supabase_configured(),
        "hubConfigured": integrations::swarm_network_base().is_some(),
        "p2pConfigured": integrations::swarm_settings().p2p_enabled,
        "transportBases": bases,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub fn record_project_cooccurrence(path: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let loader = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string();
    let ids: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    record_mod_set_cooccurrence(
        &project_dir,
        &ids,
        &manifest.minecraft.version,
        &loader,
    )
}

/// Local record + best-effort Supabase upload of mod co-occurrence.
/// Local write does not require swarm; network upload does.
pub async fn record_and_upload_cooccurrence(
    path: &str,
    mod_ids: &[String],
    source: &str,
) -> Result<serde_json::Value, String> {
    record_and_upload_cooccurrence_opts(path, mod_ids, source, true).await
}

pub async fn record_and_upload_cooccurrence_opts(
    path: &str,
    mod_ids: &[String],
    source: &str,
    record_local: bool,
) -> Result<serde_json::Value, String> {
    let project_dir = manifest_parent(path)?;
    let manifest = ProjectManifest::load_from_path(path).map_err(|e| e.to_string())?;
    let loader = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string();
    let mc = manifest.minecraft.version.clone();
    let ids = if mod_ids.is_empty() {
        manifest.mods.iter().map(|m| m.id.clone()).collect()
    } else {
        mod_ids.to_vec()
    };
    let ids = normalize_mod_id_list(&ids, 48);

    if record_local {
        let _ = record_mod_set_cooccurrence(&project_dir, &ids, &mc, &loader);
    }

    let mut uploaded = false;
    let mut upload_error: Option<String> = None;
    if integrations::swarm_enabled() {
        if let (Some(url), Some(key)) = (
            integrations::swarm_supabase_url(),
            integrations::swarm_supabase_anon_key(),
        ) {
            let client_key = tuffbox_core::swarm::load_or_create_device_signing_key()
                .ok()
                .map(|(_, device_id)| device_id);
            match tuffbox_core::swarm_supabase::report_cooccurrence_supabase(
                &url,
                &key,
                &ids,
                &mc,
                &loader,
                source,
                client_key.as_deref(),
            )
            .await
            {
                Ok(_) => uploaded = true,
                Err(e) => upload_error = Some(e),
            }
        }
    }

    Ok(json!({
        "local": record_local,
        "uploaded": uploaded,
        "modCount": ids.len(),
        "uploadError": upload_error,
        "mcVersion": mc,
        "loader": loader,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn report_mod_cooccurrence(
    path: String,
    mod_ids: Option<Vec<String>>,
    source: Option<String>,
) -> Result<serde_json::Value, String> {
    let ids = mod_ids.unwrap_or_default();
    let source = source.unwrap_or_else(|| "manual".into());
    record_and_upload_cooccurrence(&path, &ids, &source).await
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_local_cooccurrence(path: String, limit: Option<u32>) -> Result<Vec<ModPairStat>, String> {
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
    let project_dir = manifest_parent(&path)?;
    let limit = limit.unwrap_or(20) as usize;
    let local = top_cooccurrence_pairs(&project_dir, limit.max(40));

    let mut network_pairs: Vec<ModPairStat> = Vec::new();
    let mut network: Option<serde_json::Value> = None;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let loader = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string();
    let mc = manifest.minecraft.version.clone();

    // Prefer Supabase community stats when swarm is on.
    if integrations::swarm_enabled() {
        if let (Some(url), Some(key)) = (
            integrations::swarm_supabase_url(),
            integrations::swarm_supabase_anon_key(),
        ) {
            if let Ok(pairs) = tuffbox_core::swarm_supabase::fetch_cooccurrence_supabase(
                &url,
                &key,
                &mc,
                &loader,
                limit as u32,
            )
            .await
            {
                network = Some(json!({ "pairs": pairs, "source": "supabase" }));
                network_pairs = pairs;
            }
        }
        // Optional hub fallback.
        if network_pairs.is_empty() {
            if let Some(endpoint) = integrations::swarm_network_base() {
                let token = integrations::secret_optional("crash_kb");
                if let Ok(body) = tuffbox_core::crash_remote::fetch_cooccurrence_async(
                    &endpoint,
                    token.as_deref(),
                    &mc,
                    &loader,
                    limit as u32,
                )
                .await
                {
                    if let Some(arr) = body.get("pairs").and_then(|v| v.as_array()) {
                        network_pairs = arr
                            .iter()
                            .filter_map(|p| {
                                let a = p
                                    .get("modA")
                                    .or_else(|| p.get("mod_a"))
                                    .and_then(|v| v.as_str())?;
                                let b = p
                                    .get("modB")
                                    .or_else(|| p.get("mod_b"))
                                    .and_then(|v| v.as_str())?;
                                Some(ModPairStat {
                                    mod_a: a.to_string(),
                                    mod_b: b.to_string(),
                                    count: p.get("count").and_then(|v| v.as_u64()).unwrap_or(1),
                                })
                            })
                            .collect();
                    }
                    network = Some(body);
                }
            }
        }
    }

    let merged = merge_cooccurrence_pairs(&local, &network_pairs, limit);
    let prompt_hint = format_cooccurrence_for_prompt(&merged, limit.min(20));

    Ok(json!({
        "localPairs": local,
        "networkPairs": network_pairs,
        "mergedPairs": merged,
        "network": network,
        "promptHint": prompt_hint,
        "strongMatchThreshold": STRONG_MATCH_THRESHOLD,
        "supabaseConfigured": integrations::swarm_supabase_configured(),
    }))
}

/// Suggest Modrinth slugs from co-occurrence partner mods not yet installed.
#[tauri::command(rename_all = "camelCase")]
pub fn suggest_mods_from_trends(path: String, limit: Option<u32>) -> Result<Vec<String>, String> {
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

fn collect_crash_fix_timeline(
    project_dir: &Path,
    fingerprint_key: &str,
) -> Vec<String> {
    let mut lines = Vec::new();
    let store = SnapshotStore::new(project_dir);
    if let Ok(snaps) = store.list() {
        for snap in snaps.into_iter().filter(|s| {
            s.tags
                .iter()
                .any(|t| t == "crash_fix" || t == "crash_resolved")
                && s.crash_fingerprint_key
                    .as_deref()
                    .map(|k| k == fingerprint_key || fingerprint_key == "unknown")
                    .unwrap_or(true)
        }) {
            let tag = if snap.tags.iter().any(|t| t == "crash_resolved") {
                "RESOLVED"
            } else {
                "FIX"
            };
            lines.push(format!(
                "[{tag}] {} · {} · {}",
                snap.created_at,
                snap.plan_source.as_deref().unwrap_or("-"),
                tuffbox_core::crash_kb::truncate_at_char_boundary(&snap.reason, 200)
            ));
        }
    }
    // Newest-first from store.list — reverse for chronological prompt.
    lines.reverse();
    lines.truncate(24);
    lines
}

fn fallback_plan_from_resolution(
    rec: &CrashResolutionRecord,
    marker: Option<&LastCrashFixMarker>,
) -> ActionPlan {
    let actions = marker
        .map(|m| m.actions.clone())
        .unwrap_or_default();
    ActionPlan {
        schema_version: tuffbox_core::action_plan::ACTION_PLAN_SCHEMA_VERSION,
        human_explanation: rec.human_explanation.clone(),
        confidence: 0.65,
        suspected_mods: Vec::new(),
        needs_user_review: true,
        source: Some("distill_fallback".into()),
        matched_case_ids: rec.matched_case_ids.clone(),
        actions,
        additional_context: Some(format!(
            "Fallback plan from recorded resolution ({}); AI distill unavailable.",
            rec.verified_by
        )),
    }
}

/// Distill a verified crash resolution into a minimal ActionPlan for network share.
/// Local only — MUST NOT publish. User Confirm in UI triggers publish_experience_capsule.
#[tauri::command(rename_all = "camelCase")]
pub async fn distill_resolved_crash_plan(
    path: String,
    resolution_id: Option<String>,
) -> Result<serde_json::Value, String> {
    integrations::require_swarm_enabled()?;
    let project_dir = manifest_parent(&path)?;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let loader = format!("{:?}", manifest.loader.kind).to_lowercase();

    let resolutions = list_crash_resolutions(&project_dir)?;
    let rec = if let Some(id) = resolution_id.as_deref() {
        resolutions
            .into_iter()
            .find(|r| r.id == id)
            .ok_or_else(|| format!("resolution not found: {id}"))?
    } else {
        resolutions
            .into_iter()
            .next()
            .ok_or_else(|| "no crash resolutions recorded yet".to_string())?
    };

    let marker_path = last_crash_fix_path(&project_dir);
    let marker: Option<LastCrashFixMarker> = std::fs::read_to_string(&marker_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());

    let mut timeline = collect_crash_fix_timeline(&project_dir, &rec.fingerprint_key);
    if let Some(ref m) = marker {
        for a in &m.actions {
            timeline.push(format!(
                "[MARKER] {}",
                format_launcher_action_summary(a)
            ));
        }
    }
    for s in &rec.actions_summary {
        if !timeline.iter().any(|l| l.contains(s)) {
            timeline.push(format!("[RESOLVED] {s}"));
        }
    }

    let crash_excerpt = {
        let reports = project_dir.join("crash-reports");
        let mut text = String::new();
        if let Ok(entries) = std::fs::read_dir(&reports) {
            let mut newest: Option<(u64, PathBuf)> = None;
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_file() {
                    continue;
                }
                let Some(mtime) = file_mtime_secs(&p) else {
                    continue;
                };
                if newest.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true) {
                    newest = Some((mtime, p));
                }
            }
            if let Some((_, p)) = newest {
                text = std::fs::read_to_string(p).unwrap_or_default();
            }
        }
        if text.trim().is_empty() {
            text = std::fs::read_to_string(project_dir.join("logs").join("latest.log"))
                .unwrap_or_default();
        }
        tuffbox_core::crash_kb::scrub_privacy_sensitive(&tuffbox_core::crash_kb::smart_excerpt(
            &text, 2500,
        ))
    };

    let distill_ctx = tuffbox_core::ai_explanation::DistillContext {
        fingerprint_key: rec.fingerprint_key.clone(),
        mc_version: manifest.minecraft.version.clone(),
        loader: loader.clone(),
        crash_excerpt,
        action_timeline: timeline,
        resolved_summary: rec.human_explanation.clone(),
        verified_by: rec.verified_by.clone(),
        final_actions_summary: rec.actions_summary.clone(),
    };
    let prompt = tuffbox_core::ai_explanation::build_distill_prompt(&distill_ctx);
    let settings = integrations::get_integration_status().settings;

    let (plan, distill_source) =
        match integrations::call_ai_crash_explain(&settings.ai, &prompt).await {
            Ok(value) => {
                let raw = serde_json::to_string(&value).unwrap_or_default();
                match tuffbox_core::action_plan::parse_action_plan(&raw) {
                    Ok(mut plan) => {
                        plan.needs_user_review = true;
                        if plan.source.as_deref() != Some("distill") {
                            plan.source = Some("distill".into());
                        }
                        if plan.matched_case_ids.is_empty() {
                            plan.matched_case_ids = rec.matched_case_ids.clone();
                        }
                        (plan, "ai")
                    }
                    Err(_) => (
                        fallback_plan_from_resolution(&rec, marker.as_ref()),
                        "fallback_parse",
                    ),
                }
            }
            Err(_) => (
                fallback_plan_from_resolution(&rec, marker.as_ref()),
                "fallback_ai",
            ),
        };

    let validation = tuffbox_core::action_plan::validate_action_plan(&plan);
    Ok(json!({
        "schemaVersion": plan.schema_version,
        "humanExplanation": plan.human_explanation,
        "confidence": plan.confidence,
        "suspectedMods": plan.suspected_mods,
        "needsUserReview": plan.needs_user_review,
        "source": plan.source,
        "matchedCaseIds": plan.matched_case_ids,
        "actions": plan.actions,
        "additionalContext": plan.additional_context,
        "validation": validation,
        "distilledFrom": "user_history",
        "distillSource": distill_source,
        "resolutionId": rec.id,
        "fingerprintKey": rec.fingerprint_key,
        "verifiedBy": rec.verified_by,
        "beta": true,
    }))
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
