//! Shared helper functions used across multiple command modules.

use std::io::Write;
use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use std::sync::Mutex;
use tuffbox_core::{ProjectManifest, Snapshot, SnapshotStore};

use crate::pack_events;
use crate::types::LauncherDataState;

// ── Global locks ─────────────────────────────────────────────────

/// Serializes manifest + mods-folder mutations so background `sync_mods_folder`
/// cannot overwrite an in-flight Update All / single update.
pub(crate) static MODS_IO_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// ── Path resolution ──────────────────────────────────────────────

fn find_manifest_in_project_dir(project_dir: &str) -> Result<PathBuf, String> {
    let dir = PathBuf::from(project_dir);
    let mut manifests = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".tuffbox.json"))
            .unwrap_or(false)
        {
            manifests.push(path);
        }
    }

    if manifests.is_empty() {
        return Err(format!(
            "project manifest not found in project directory: {}",
            dir.display()
        ));
    }

    if manifests.len() == 1 {
        return Ok(manifests.remove(0));
    }

    let state = load_launcher_data(&dir);
    if let Some(ref last_opened) = state.last_opened {
        let preferred = PathBuf::from(last_opened);
        if manifests.iter().any(|path| path == &preferred) {
            return Ok(preferred);
        }
    }

    let default = dir.join("project.tuffbox.json");
    if default.exists() {
        return Ok(default);
    }

    manifests.sort();
    Ok(manifests[0].clone())
}

/// Resolve a project directory or manifest path to the canonical `.tuffbox.json` file.
pub(crate) fn resolve_manifest_path(path: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(path);

    if path_buf.is_dir() {
        return find_manifest_in_project_dir(path);
    }

    if path_buf.is_file() {
        return Ok(path_buf);
    }

    if let Some(parent) = path_buf.parent() {
        if parent.is_dir() {
            if let Ok(found) = find_manifest_in_project_dir(&parent.to_string_lossy()) {
                return Ok(found);
            }
        }
    }

    Err(format!(
        "project manifest not found: {}",
        path_buf.display()
    ))
}

pub(crate) fn manifest_parent(path: &str) -> Result<PathBuf, String> {
    let resolved = resolve_manifest_path(path)?;
    resolved
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "manifest has no parent directory".to_string())
}

// ── Snapshot helpers ─────────────────────────────────────────────

pub(crate) fn auto_snapshot(manifest_path: &Path, operation: &str) -> anyhow::Result<Snapshot> {
    auto_snapshot_detailed(manifest_path, operation, &[], &[])
}

pub(crate) fn auto_snapshot_with_changed_files(
    manifest_path: &Path,
    operation: &str,
    changed_files: &[PathBuf],
) -> anyhow::Result<Snapshot> {
    auto_snapshot_detailed(manifest_path, operation, changed_files, &[])
}

pub(crate) fn auto_snapshot_detailed(
    manifest_path: &Path,
    operation: &str,
    changed_files: &[PathBuf],
    actions_summary: &[String],
) -> anyhow::Result<Snapshot> {
    let project_dir = manifest_path.parent().ok_or_else(|| {
        anyhow::anyhow!("manifest path has no parent: {}", manifest_path.display())
    })?;
    let lockfile_path = manifest_path.with_extension("lock.json");
    let lockfile_path = if lockfile_path.exists() {
        Some(lockfile_path)
    } else {
        None
    };
    let store = SnapshotStore::new(project_dir);
    let name = format!("auto-before-{operation}");
    let reason = format!("Auto snapshot before {operation}");
    let summary: Vec<String> = if actions_summary.is_empty() {
        vec![format!("Safety point before {operation}")]
    } else {
        actions_summary.to_vec()
    };
    let actor = pack_events::actor_for_operation(operation).to_string();
    let meta = tuffbox_core::SnapshotMeta {
        operation: operation.to_string(),
        actions_summary: summary,
        actor: Some(actor),
        ..Default::default()
    };
    let snapshot = store.create_with_meta(
        &name,
        &reason,
        manifest_path,
        lockfile_path.as_ref(),
        changed_files,
        meta,
    )?;
    let _ = pack_events::append_from_snapshot(
        project_dir,
        operation,
        &snapshot.id,
        changed_files,
        &reason,
    );
    Ok(snapshot)
}

pub(crate) fn save_manifest(path: &Path, manifest: &ProjectManifest) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(manifest)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("manifest path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::Builder::new()
        .prefix(".tuffbox-manifest-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    staged.write_all(json.as_bytes())?;
    staged.flush()?;
    staged.as_file().sync_all()?;
    staged
        .persist(path)
        .map_err(|error| anyhow::Error::new(error.error))?;
    Ok(())
}

// ── File utilities ───────────────────────────────────────────────

pub(crate) fn safe_project_file(project_dir: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = validate_relative_snapshot_path(relative_path)?;
    let target = project_dir.join(&relative);
    if !target.starts_with(project_dir) {
        return Err("path escapes project directory".to_string());
    }
    Ok(target)
}

pub(crate) fn validate_relative_snapshot_path(relative_path: &str) -> Result<PathBuf, String> {
    let relative = PathBuf::from(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("invalid snapshot-relative path".to_string());
    }
    Ok(relative)
}

pub(crate) fn read_small_text_file(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Ok(String::new());
    }
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 512 * 1024 {
        return Ok(format!(
            "# File is too large for inline diff: {} bytes\n",
            metadata.len()
        ));
    }
    std::fs::read_to_string(path)
        .map_err(|_| "# Binary or non-UTF8 file; inline diff unavailable.\n".to_string())
}

pub(crate) fn unified_text_diff(before: &str, after: &str) -> String {
    if before == after {
        return "No content changes.".to_string();
    }
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let mut table = vec![vec![0usize; after_lines.len() + 1]; before_lines.len() + 1];
    for i in (0..before_lines.len()).rev() {
        for j in (0..after_lines.len()).rev() {
            table[i][j] = if before_lines[i] == after_lines[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    let mut out = String::new();
    let mut i = 0;
    let mut j = 0;
    while i < before_lines.len() && j < after_lines.len() {
        if before_lines[i] == after_lines[j] {
            out.push_str("  ");
            out.push_str(before_lines[i]);
            out.push('\n');
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            out.push_str("- ");
            out.push_str(before_lines[i]);
            out.push('\n');
            i += 1;
        } else {
            out.push_str("+ ");
            out.push_str(after_lines[j]);
            out.push('\n');
            j += 1;
        }
    }
    while i < before_lines.len() {
        out.push_str("- ");
        out.push_str(before_lines[i]);
        out.push('\n');
        i += 1;
    }
    while j < after_lines.len() {
        out.push_str("+ ");
        out.push_str(after_lines[j]);
        out.push('\n');
        j += 1;
    }
    out
}

pub(crate) fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = std::fs::metadata(&p) {
                total += meta.len();
            }
        }
    }
    total
}

pub(crate) fn copy_dir_recursive(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ── Launcher data persistence ────────────────────────────────────

pub(crate) fn launcher_data_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("launcher-data.json")
}

pub(crate) fn load_launcher_data(project_dir: &Path) -> LauncherDataState {
    let path = launcher_data_path(project_dir);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_launcher_data(
    project_dir: &Path,
    state: &LauncherDataState,
) -> Result<(), String> {
    let path = launcher_data_path(project_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

// ── Stats persistence ────────────────────────────────────────────

pub(crate) fn stats_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("stats.json")
}

pub(crate) fn load_stats(project_dir: &Path) -> crate::types::ProjectStats {
    let p = stats_path(project_dir);
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn save_stats(
    project_dir: &Path,
    stats: &crate::types::ProjectStats,
) -> Result<(), String> {
    let p = stats_path(project_dir);
    if let Some(par) = p.parent() {
        std::fs::create_dir_all(par).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &p,
        serde_json::to_string_pretty(stats).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

// ── Backup helpers ───────────────────────────────────────────────

pub(crate) fn backup_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("backups")
}

pub(crate) fn load_backup_index(project_dir: &Path) -> crate::types::BackupIndex {
    let p = backup_dir(project_dir).join("index.json");
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(crate::types::BackupIndex {
            backups: vec![],
            max_count: 20,
        })
}

pub(crate) fn save_backup_index(
    project_dir: &Path,
    idx: &crate::types::BackupIndex,
) -> Result<(), String> {
    let d = backup_dir(project_dir);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::fs::write(
        d.join("index.json"),
        serde_json::to_string_pretty(idx).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

// ── Shell / string helpers ───────────────────────────────────────

pub(crate) fn shell_escape(s: &str) -> String {
    if s.chars().all(|c| c.is_alphanumeric() || "-_.:/".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub(crate) fn slugify_project_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else if c == ' ' || c == '_' || c == '-' {
                '-'
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}
