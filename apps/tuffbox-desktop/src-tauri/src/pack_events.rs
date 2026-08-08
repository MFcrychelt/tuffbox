//! Pack activity event log + baseline delta scanner for History / AI context.
//!
//! Storage:
//! - `.tuffbox/events.jsonl` — append-only activity journal
//! - `.tuffbox/history-baseline.json` — mtime/size map for external-edit detection

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_EVENTS: usize = 2000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackEvent {
    pub id: String,
    pub ts: String,
    pub actor: String,
    pub op: String,
    #[serde(default)]
    pub paths: Vec<String>,
    pub category: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BaselineEntry {
    pub mtime: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HistoryBaseline {
    #[serde(default)]
    pub files: HashMap<String, BaselineEntry>,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProjectChangesResult {
    pub events: Vec<PackEvent>,
    pub baseline_updated: bool,
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub jar_drift: usize,
}

fn events_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("events.jsonl")
}

fn baseline_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("history-baseline.json")
}

fn now_rfc3339() -> String {
    tuffbox_core::time_util::rfc3339_now()
}

fn now_id_suffix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn category_for_path(path: &str) -> &'static str {
    let normalized = path.replace('\\', "/").to_lowercase();
    let root = normalized.split('/').next().unwrap_or("");
    if matches!(normalized.as_str(), "options.txt" | "servers.dat") {
        return "Configs";
    }
    match root {
        "mods" => "Mods",
        "config" | "defaultconfigs" | "kubejs" | "scripts" | "overrides" => "Configs",
        "shaderpacks" | "shaders" => "Shaders",
        "resourcepacks" | "texturepacks" => "Resource Packs",
        "datapacks" | "world" | "saves" => "World/Data",
        _ => "Other",
    }
}

pub fn actor_for_operation(operation: &str) -> &'static str {
    let op = operation.to_lowercase();
    if op.contains("crash") || op.contains("action-plan") || op.contains("action_plan") || op.contains("swarm")
    {
        "ai"
    } else if op.contains("track-history") || op.contains("scan") {
        "scan"
    } else {
        "launcher"
    }
}

pub fn op_for_operation(operation: &str, paths: &[String]) -> String {
    let op = operation.to_lowercase();
    if op.contains("crash_fix") || op.contains("crash-fix") {
        return "crash_fix".into();
    }
    if op.contains("crash_resolved") || op.contains("crash-resolved") {
        return "crash_resolved".into();
    }
    if op.contains("edit-config") || op.contains("save-quest") {
        return "file_edit".into();
    }
    if op.contains("track-history") {
        return "snapshot".into();
    }
    if op.contains("rollback") {
        return "rollback".into();
    }
    if op.contains("mod") || paths.iter().any(|p| p.starts_with("mods/")) {
        return "mod_change".into();
    }
    "snapshot".into()
}

pub fn append_pack_event(project_dir: &Path, mut event: PackEvent) -> Result<(), String> {
    let dir = project_dir.join(".tuffbox");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = events_path(project_dir);
    if event.id.is_empty() {
        event.id = format!("evt-{}", now_id_suffix());
    }
    if event.ts.is_empty() {
        event.ts = now_rfc3339();
    }
    let line = serde_json::to_string(&event).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    writeln!(file, "{line}").map_err(|e| e.to_string())?;
    trim_events_file(&path)?;
    Ok(())
}

pub fn append_from_snapshot(
    project_dir: &Path,
    operation: &str,
    snapshot_id: &str,
    changed_files: &[PathBuf],
    reason: &str,
) -> Result<(), String> {
    append_from_snapshot_with_summary(project_dir, operation, snapshot_id, changed_files, reason, &[])
}

/// Like [`append_from_snapshot`], but prefers human `actions_summary` lines when
/// no changed files were copied into the snapshot (typical for add-mod).
pub fn append_from_snapshot_with_summary(
    project_dir: &Path,
    operation: &str,
    snapshot_id: &str,
    changed_files: &[PathBuf],
    reason: &str,
    actions_summary: &[String],
) -> Result<(), String> {
    let paths: Vec<String> = changed_files
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    let category = if paths.iter().any(|p| category_for_path(p) == "Mods")
        || operation.to_lowercase().contains("mod")
    {
        "Mods".into()
    } else {
        paths
            .first()
            .map(|p| category_for_path(p).to_string())
            .unwrap_or_else(|| "Other".into())
    };
    let summary = if !actions_summary.is_empty() {
        let head: Vec<&str> = actions_summary.iter().take(8).map(|s| s.as_str()).collect();
        let more = actions_summary.len().saturating_sub(head.len());
        if more > 0 {
            format!("{} (+{} more)", head.join("; "), more)
        } else {
            head.join("; ")
        }
    } else if paths.is_empty() {
        // Avoid "add-mod: Auto snapshot before add-mod" noise.
        if reason.starts_with("Auto snapshot before ") {
            format!("Safety point before {operation}")
        } else {
            format!("{operation}: {reason}")
        }
    } else if paths.len() == 1 {
        format!("{operation}: {}", paths[0])
    } else {
        format!("{operation}: {} files ({})", paths.len(), paths[0])
    };
    let mut tags = Vec::new();
    let op_l = operation.to_lowercase();
    if op_l.contains("crash_fix") {
        tags.push("crash_fix".into());
    }
    if op_l.contains("crash_resolved") {
        tags.push("crash_resolved".into());
    }
    append_pack_event(
        project_dir,
        PackEvent {
            id: format!("evt-{}-{}", snapshot_id, now_id_suffix() % 10_000),
            ts: now_rfc3339(),
            actor: actor_for_operation(operation).into(),
            op: op_for_operation(operation, &paths),
            paths,
            category,
            summary,
            snapshot_id: Some(snapshot_id.to_string()),
            tags,
            meta: Some(serde_json::json!({
                "reason": reason,
                "operation": operation,
                "actionsSummary": actions_summary,
            })),
        },
    )
}

/// Record a launcher mod operation with concrete mod names / jar paths.
pub fn record_mod_change_event(
    project_dir: &Path,
    operation: &str,
    snapshot_id: Option<&str>,
    summaries: &[String],
    paths: &[String],
) -> Result<(), String> {
    if summaries.is_empty() && paths.is_empty() {
        return Ok(());
    }
    let summary = if !summaries.is_empty() {
        let head: Vec<&str> = summaries.iter().take(12).map(|s| s.as_str()).collect();
        let more = summaries.len().saturating_sub(head.len());
        if more > 0 {
            format!("{} (+{} more)", head.join("; "), more)
        } else {
            head.join("; ")
        }
    } else {
        format!("{operation}: {} file(s)", paths.len())
    };
    let id = format!(
        "evt-mod-{}-{}",
        snapshot_id.unwrap_or("nosnap"),
        now_id_suffix() % 100_000
    );
    append_pack_event(
        project_dir,
        PackEvent {
            id,
            ts: now_rfc3339(),
            actor: "launcher".into(),
            op: "mod_change".into(),
            paths: paths.to_vec(),
            category: "Mods".into(),
            summary,
            snapshot_id: snapshot_id.map(|s| s.to_string()),
            tags: vec!["launcher".into()],
            meta: Some(serde_json::json!({
                "operation": operation,
                "actionsSummary": summaries,
            })),
        },
    )?;
    sync_baseline_paths(project_dir, paths)?;
    Ok(())
}

/// Update history baseline entries for the given relative paths (so launcher
/// installs are not later reported as external_add).
pub fn sync_baseline_paths(project_dir: &Path, relative_paths: &[String]) -> Result<(), String> {
    if relative_paths.is_empty() {
        return Ok(());
    }
    let mut baseline = load_baseline(project_dir);
    for rel in relative_paths {
        let abs = project_dir.join(rel);
        if abs.is_file() {
            if let Ok(meta) = abs.metadata() {
                baseline.files.insert(
                    rel.replace('\\', "/"),
                    BaselineEntry {
                        mtime: file_mtime_secs(&abs),
                        size: meta.len(),
                    },
                );
            }
            cache_config_content(project_dir, rel);
        } else {
            baseline.files.remove(rel);
            remove_cached_config_content(project_dir, rel);
        }
    }
    baseline.updated_at = now_rfc3339();
    save_baseline(project_dir, &baseline)
}

fn content_cache_root(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("history-content")
}

fn content_cache_path(project_dir: &Path, rel: &str) -> PathBuf {
    let safe = rel.replace('\\', "/").replace("..", "_");
    content_cache_root(project_dir).join(safe)
}

fn is_textish_config(rel: &str) -> bool {
    let path = Path::new(rel);
    crate::helpers::is_editable_config_path(path)
}

fn read_cached_config_content(project_dir: &Path, rel: &str) -> Option<String> {
    let path = content_cache_path(project_dir, rel);
    std::fs::read_to_string(path).ok()
}

fn cache_config_content(project_dir: &Path, rel: &str) {
    if !is_textish_config(rel) {
        return;
    }
    let abs = project_dir.join(rel);
    if !abs.is_file() {
        return;
    }
    let Ok(meta) = abs.metadata() else {
        return;
    };
    if meta.len() > 512 * 1024 {
        return;
    }
    let Ok(text) = std::fs::read_to_string(&abs) else {
        return;
    };
    let cache = content_cache_path(project_dir, rel);
    if let Some(parent) = cache.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(cache, text);
}

fn remove_cached_config_content(project_dir: &Path, rel: &str) {
    let _ = std::fs::remove_file(content_cache_path(project_dir, rel));
}

/// Pull changed keys / assignment lines from a unified diff for a short preview.
fn config_change_preview(diff: &str) -> String {
    let mut keys = Vec::new();
    for line in diff.lines() {
        let trimmed = if let Some(rest) = line.strip_prefix("+ ").or_else(|| line.strip_prefix("- "))
        {
            rest.trim()
        } else {
            continue;
        };
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            // TOML section
            let section = trimmed.trim_matches(|c| c == '[' || c == ']');
            if !section.is_empty() && !keys.iter().any(|k| k == section) {
                keys.push(section.to_string());
            }
            continue;
        }
        let key = trimmed
            .split_once('=')
            .or_else(|| trimmed.split_once(':'))
            .map(|(k, _)| k.trim().trim_matches('"').trim_matches('\''))
            .filter(|k| !k.is_empty() && k.len() < 80);
        if let Some(k) = key {
            if !keys.iter().any(|existing| existing == k) {
                keys.push(k.to_string());
            }
        }
        if keys.len() >= 6 {
            break;
        }
    }
    if keys.is_empty() {
        // Fall back to first changed lines.
        diff.lines()
            .filter(|l| l.starts_with("+ ") || l.starts_with("- "))
            .take(4)
            .collect::<Vec<_>>()
            .join(" · ")
    } else {
        format!("changed {}", keys.join(", "))
    }
}

fn enrich_external_file_event(
    project_dir: &Path,
    rel: &str,
    op: &str,
) -> (String, Option<serde_json::Value>) {
    if !is_textish_config(rel) {
        let summary = match op {
            "external_add" => format!("Added on disk: {rel}"),
            "external_remove" => format!("Removed from disk: {rel}"),
            _ => format!("Changed on disk: {rel}"),
        };
        return (summary, None);
    }

    let abs = project_dir.join(rel);
    let after = if abs.is_file() {
        crate::helpers::read_small_text_file(&abs).unwrap_or_default()
    } else {
        String::new()
    };
    let before = read_cached_config_content(project_dir, rel).unwrap_or_default();

    let (summary, meta) = if op == "external_add" {
        let preview = if after.is_empty() {
            format!("Added on disk: {rel}")
        } else {
            let diff = crate::helpers::unified_text_diff("", &after);
            let place = config_change_preview(&diff);
            format!("{rel}: added ({place})")
        };
        let diff = crate::helpers::unified_text_diff("", &after);
        (
            preview.clone(),
            Some(serde_json::json!({
                "diff": diff,
                "preview": preview,
            })),
        )
    } else if op == "external_remove" {
        let diff = crate::helpers::unified_text_diff(&before, "");
        let place = if before.is_empty() {
            format!("Removed from disk: {rel}")
        } else {
            format!("{rel}: removed ({})", config_change_preview(&diff))
        };
        (
            place.clone(),
            Some(serde_json::json!({
                "diff": diff,
                "preview": place,
            })),
        )
    } else {
        let diff = crate::helpers::unified_text_diff(&before, &after);
        let place = config_change_preview(&diff);
        let summary = if place.is_empty() {
            format!("Changed on disk: {rel}")
        } else {
            format!("{rel}: {place}")
        };
        (
            summary.clone(),
            Some(serde_json::json!({
                "diff": diff,
                "preview": summary,
            })),
        )
    };

    if op == "external_remove" {
        remove_cached_config_content(project_dir, rel);
    } else {
        cache_config_content(project_dir, rel);
    }

    (summary, meta)
}

fn trim_events_file(path: &Path) -> Result<(), String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() <= MAX_EVENTS {
        return Ok(());
    }
    let keep = &lines[lines.len() - MAX_EVENTS..];
    let mut out = String::new();
    for line in keep {
        out.push_str(line);
        out.push('\n');
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}

pub fn list_pack_events(project_dir: &Path, limit: Option<usize>) -> Vec<PackEvent> {
    let path = events_path(project_dir);
    let Ok(file) = std::fs::File::open(&path) else {
        return Vec::new();
    };
    let reader = std::io::BufReader::new(file);
    let mut events = Vec::new();
    for line in reader.lines().flatten() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Ok(ev) = serde_json::from_str::<PackEvent>(t) {
            events.push(ev);
        }
    }
    events.reverse(); // newest first
    if let Some(n) = limit {
        events.truncate(n);
    }
    events
}

pub fn recent_pack_change_lines(project_dir: &Path, limit: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for ev in list_pack_events(project_dir, Some(limit.saturating_mul(2))) {
        let paths = if ev.paths.is_empty() {
            String::new()
        } else {
            format!(" ({})", ev.paths.iter().take(3).cloned().collect::<Vec<_>>().join(", "))
        };
        lines.push(format!(
            "[{}] [{}/{}] {}{}",
            truncate_ts(&ev.ts),
            ev.actor,
            ev.op,
            ev.summary,
            paths
        ));
        if lines.len() >= limit {
            break;
        }
    }
    lines
}

fn truncate_ts(ts: &str) -> &str {
    // Prefer date+time without fractional seconds for prompts.
    if ts.len() >= 19 {
        &ts[..19]
    } else {
        ts
    }
}

pub fn load_baseline(project_dir: &Path) -> HistoryBaseline {
    let path = baseline_path(project_dir);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_baseline(project_dir: &Path, baseline: &HistoryBaseline) -> Result<(), String> {
    let dir = project_dir.join(".tuffbox");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(baseline).map_err(|e| e.to_string())?;
    std::fs::write(baseline_path(project_dir), json).map_err(|e| e.to_string())
}

/// Roots enabled by History settings category map.
pub fn roots_from_tracked(tracked: &HashMap<String, bool>) -> Vec<&'static str> {
    let mut roots = Vec::new();
    let on = |key: &str| tracked.get(key).copied().unwrap_or(false);
    if on("Mods") {
        roots.push("mods");
    }
    if on("Configs") {
        roots.extend([
            "config",
            "defaultconfigs",
            "kubejs",
            "scripts",
            "overrides",
            "options.txt",
            "servers.dat",
        ]);
    }
    if on("Shaders") {
        roots.extend(["shaderpacks", "shaders"]);
    }
    if on("Resource Packs") {
        roots.extend(["resourcepacks", "texturepacks"]);
    }
    if on("World/Data") {
        roots.extend(["datapacks", "saves"]);
    }
    roots
}

fn file_mtime_secs(path: &Path) -> u64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn collect_relative_files(project_dir: &Path, roots: &[&str]) -> HashMap<String, BaselineEntry> {
    let mut out = HashMap::new();
    for root in roots {
        if *root == "options.txt" || *root == "servers.dat" {
            let p = project_dir.join(root);
            if p.is_file() {
                if let Ok(meta) = p.metadata() {
                    out.insert(
                        (*root).to_string(),
                        BaselineEntry {
                            mtime: file_mtime_secs(&p),
                            size: meta.len(),
                        },
                    );
                }
            }
            continue;
        }
        let dir = project_dir.join(root);
        if !dir.is_dir() {
            continue;
        }
        walk_files(project_dir, &dir, &mut out);
    }
    out
}

fn walk_files(project_dir: &Path, dir: &Path, out: &mut HashMap<String, BaselineEntry>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_symlink() {
            continue;
        }
        if path.is_dir() {
            walk_files(project_dir, &path, out);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        // Skip huge binaries in baseline (still track jars in mods/).
        let Ok(meta) = path.metadata() else {
            continue;
        };
        let rel = path
            .strip_prefix(project_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let is_jar = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("jar"))
            .unwrap_or(false);
        if meta.len() > 64 * 1024 * 1024 && !is_jar {
            continue;
        }
        out.insert(
            rel,
            BaselineEntry {
                mtime: file_mtime_secs(&path),
                size: meta.len(),
            },
        );
    }
}

fn manifest_mod_file_names(project_dir: &Path) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    // Find *.tuffbox.json
    let Ok(rd) = std::fs::read_dir(project_dir) else {
        return names;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        let Some(name) = p.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".tuffbox.json") {
            continue;
        }
        if let Ok(manifest) = tuffbox_core::ProjectManifest::load_from_path(&p) {
            for m in manifest.mods {
                if let Some(fnm) = m.file_name {
                    names.insert(fnm);
                }
            }
        }
        break;
    }
    names
}

/// Compare disk to baseline; emit events; update baseline.
pub fn scan_project_changes(
    project_dir: &Path,
    tracked: &HashMap<String, bool>,
) -> Result<ScanProjectChangesResult, String> {
    let roots = roots_from_tracked(tracked);
    let current = collect_relative_files(project_dir, &roots);
    let mut baseline = load_baseline(project_dir);
    let had_baseline = !baseline.files.is_empty();

    let mut added = 0usize;
    let mut modified = 0usize;
    let mut removed = 0usize;
    let mut jar_drift = 0usize;
    let mut new_events = Vec::new();

    if had_baseline {
        for (rel, cur) in &current {
            match baseline.files.get(rel) {
                None => {
                    added += 1;
                    let (summary, meta) =
                        enrich_external_file_event(project_dir, rel, "external_add");
                    let ev = PackEvent {
                        id: format!("evt-add-{}", now_id_suffix()),
                        ts: now_rfc3339(),
                        actor: "scan".into(),
                        op: "external_add".into(),
                        paths: vec![rel.clone()],
                        category: category_for_path(rel).into(),
                        summary,
                        snapshot_id: None,
                        tags: vec!["external".into()],
                        meta,
                    };
                    let _ = append_pack_event(project_dir, ev.clone());
                    new_events.push(ev);
                }
                Some(prev) if prev.mtime != cur.mtime || prev.size != cur.size => {
                    modified += 1;
                    let (summary, meta) =
                        enrich_external_file_event(project_dir, rel, "external_edit");
                    let ev = PackEvent {
                        id: format!("evt-edit-{}", now_id_suffix()),
                        ts: now_rfc3339(),
                        actor: "scan".into(),
                        op: "external_edit".into(),
                        paths: vec![rel.clone()],
                        category: category_for_path(rel).into(),
                        summary,
                        snapshot_id: None,
                        tags: vec!["external".into()],
                        meta,
                    };
                    let _ = append_pack_event(project_dir, ev.clone());
                    new_events.push(ev);
                }
                _ => {}
            }
        }
        for rel in baseline.files.keys() {
            if !current.contains_key(rel) {
                removed += 1;
                let (summary, meta) =
                    enrich_external_file_event(project_dir, rel, "external_remove");
                let ev = PackEvent {
                    id: format!("evt-rm-{}", now_id_suffix()),
                    ts: now_rfc3339(),
                    actor: "scan".into(),
                    op: "external_remove".into(),
                    paths: vec![rel.clone()],
                    category: category_for_path(rel).into(),
                    summary,
                    snapshot_id: None,
                    tags: vec!["external".into()],
                    meta,
                };
                let _ = append_pack_event(project_dir, ev.clone());
                new_events.push(ev);
            }
        }
    } else {
        // Seed content cache for editable configs so the next edit yields a real diff.
        for rel in current.keys() {
            cache_config_content(project_dir, rel);
        }
    }

    // jar_drift: mods/*.jar not listed in manifest (only after baseline exists —
    // never spam every orphan jar on first seed scan).
    let manifest_jars = manifest_mod_file_names(project_dir);
    if had_baseline {
        for rel in current.keys() {
            if !rel.starts_with("mods/") {
                continue;
            }
            let name = rel.rsplit('/').next().unwrap_or(rel);
            if !name.to_lowercase().ends_with(".jar") {
                continue;
            }
            if name.to_lowercase().ends_with(".disabled") {
                continue;
            }
            if !manifest_jars.contains(name) {
                jar_drift += 1;
                let is_new = new_events.iter().any(|e| e.paths.iter().any(|p| p == rel));
                if is_new {
                    let ev = PackEvent {
                        id: format!("evt-drift-{}", now_id_suffix()),
                        ts: now_rfc3339(),
                        actor: "scan".into(),
                        op: "jar_drift".into(),
                        paths: vec![rel.clone()],
                        category: "Mods".into(),
                        summary: format!("Jar on disk not in manifest: {name}"),
                        snapshot_id: None,
                        tags: vec!["jar_drift".into(), "external".into()],
                        meta: Some(
                            serde_json::json!({ "hint": "Import to manifest or remove orphan jar" }),
                        ),
                    };
                    let _ = append_pack_event(project_dir, ev.clone());
                    new_events.push(ev);
                }
            }
        }
    } else {
        for rel in current.keys() {
            if !rel.starts_with("mods/") {
                continue;
            }
            let name = rel.rsplit('/').next().unwrap_or(rel);
            if name.to_lowercase().ends_with(".jar")
                && !name.to_lowercase().ends_with(".disabled")
                && !manifest_jars.contains(name)
            {
                jar_drift += 1;
            }
        }
    }

    baseline.files = current;
    baseline.updated_at = now_rfc3339();
    save_baseline(project_dir, &baseline)?;

    Ok(ScanProjectChangesResult {
        events: new_events,
        baseline_updated: true,
        added,
        modified,
        removed,
        jar_drift,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tuffbox-hist-{nanos}"));
        fs::create_dir_all(dir.join("config")).unwrap();
        fs::create_dir_all(dir.join("mods")).unwrap();
        dir
    }

    #[test]
    fn snapshot_summary_prefers_actions_over_auto_reason() {
        let dir = temp_project();
        append_from_snapshot_with_summary(
            &dir,
            "add-mod-with-dependencies",
            "snap-1",
            &[],
            "Auto snapshot before add-mod-with-dependencies",
            &[
                "Install Cloth Config API 15.0.140".into(),
                "Install Mod Menu 11.0.3".into(),
            ],
        )
        .unwrap();
        let events = list_pack_events(&dir, Some(10));
        assert_eq!(events.len(), 1);
        assert!(events[0].summary.contains("Cloth Config"));
        assert!(!events[0].summary.contains("Auto snapshot"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_config_edit_includes_diff_and_keys() {
        let dir = temp_project();
        let rel = "config/demo.toml";
        fs::write(dir.join(rel), "enabled = false\nmax = 1\n").unwrap();
        cache_config_content(&dir, rel);

        fs::write(dir.join(rel), "enabled = true\nmax = 1\n").unwrap();
        let (summary, meta) = enrich_external_file_event(&dir, rel, "external_edit");
        assert!(summary.contains("enabled"), "{summary}");
        let meta = meta.expect("meta");
        let diff = meta.get("diff").and_then(|v| v.as_str()).unwrap_or("");
        assert!(diff.contains("- enabled = false") || diff.contains("- enabled = false\n"));
        assert!(diff.contains("+ enabled = true") || diff.contains("+ enabled = true\n"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_baseline_avoids_false_external_add() {
        let dir = temp_project();
        let mut tracked = HashMap::new();
        tracked.insert("Mods".into(), true);
        tracked.insert("Configs".into(), true);

        // Seed baseline empty → first scan should not emit adds.
        let seed = scan_project_changes(&dir, &tracked).unwrap();
        assert_eq!(seed.added, 0);
        assert!(seed.events.is_empty());

        let jar_rel = "mods/cloth-config.jar".to_string();
        fs::write(dir.join(&jar_rel), b"fake-jar").unwrap();
        // Launcher syncs baseline after install.
        sync_baseline_paths(&dir, &[jar_rel.clone()]).unwrap();

        let after = scan_project_changes(&dir, &tracked).unwrap();
        assert_eq!(after.added, 0, "launcher jar must not appear as external_add");
        assert!(
            after
                .events
                .iter()
                .all(|e| e.op != "external_add" || !e.paths.contains(&jar_rel))
        );

        // Truly external add still detected.
        let orphan = "mods/orphan.jar".to_string();
        fs::write(dir.join(&orphan), b"orphan").unwrap();
        let external = scan_project_changes(&dir, &tracked).unwrap();
        assert_eq!(external.added, 1);
        assert!(external.events.iter().any(|e| e.op == "external_add"));

        // External remove.
        fs::remove_file(dir.join(&orphan)).unwrap();
        let removed = scan_project_changes(&dir, &tracked).unwrap();
        assert_eq!(removed.removed, 1);
        assert!(removed.events.iter().any(|e| e.op == "external_remove"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_preview_extracts_assignment_keys() {
        let preview = config_change_preview(
            "  keep = 1\n- enabled = false\n+ enabled = true\n- [section]\n+ [section]\n",
        );
        assert!(preview.contains("enabled"), "{preview}");
    }
}
