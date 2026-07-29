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
    let paths: Vec<String> = changed_files
        .iter()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .collect();
    let category = paths
        .first()
        .map(|p| category_for_path(p).to_string())
        .unwrap_or_else(|| "Other".into());
    let summary = if paths.is_empty() {
        format!("{operation}: {reason}")
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
            meta: Some(serde_json::json!({ "reason": reason, "operation": operation })),
        },
    )
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
                    let ev = PackEvent {
                        id: format!("evt-add-{}", now_id_suffix()),
                        ts: now_rfc3339(),
                        actor: "scan".into(),
                        op: "external_add".into(),
                        paths: vec![rel.clone()],
                        category: category_for_path(rel).into(),
                        summary: format!("Added on disk: {rel}"),
                        snapshot_id: None,
                        tags: vec!["external".into()],
                        meta: None,
                    };
                    let _ = append_pack_event(project_dir, ev.clone());
                    new_events.push(ev);
                }
                Some(prev) if prev.mtime != cur.mtime || prev.size != cur.size => {
                    modified += 1;
                    let ev = PackEvent {
                        id: format!("evt-edit-{}", now_id_suffix()),
                        ts: now_rfc3339(),
                        actor: "scan".into(),
                        op: "external_edit".into(),
                        paths: vec![rel.clone()],
                        category: category_for_path(rel).into(),
                        summary: format!("Changed on disk: {rel}"),
                        snapshot_id: None,
                        tags: vec!["external".into()],
                        meta: None,
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
                let ev = PackEvent {
                    id: format!("evt-rm-{}", now_id_suffix()),
                    ts: now_rfc3339(),
                    actor: "scan".into(),
                    op: "external_remove".into(),
                    paths: vec![rel.clone()],
                    category: category_for_path(rel).into(),
                    summary: format!("Removed from disk: {rel}"),
                    snapshot_id: None,
                    tags: vec!["external".into()],
                    meta: None,
                };
                let _ = append_pack_event(project_dir, ev.clone());
                new_events.push(ev);
            }
        }
    }

    // jar_drift: mods/*.jar not listed in manifest
    let manifest_jars = manifest_mod_file_names(project_dir);
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
            // Only emit if newly detected vs previous baseline absence of jar_drift tag noise:
            // emit when file was added/modified this scan OR first baseline.
            let is_new = !had_baseline
                || new_events.iter().any(|e| e.paths.iter().any(|p| p == rel));
            if is_new || !had_baseline {
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
                    meta: Some(serde_json::json!({ "hint": "Import to manifest or remove orphan jar" })),
                };
                let _ = append_pack_event(project_dir, ev.clone());
                new_events.push(ev);
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
