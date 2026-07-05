use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("snapshot directory not found: {0}")]
    NotFound(PathBuf),
    #[error("failed to read snapshot metadata: {0}")]
    ReadMetadata(#[source] std::io::Error),
    #[error("failed to parse snapshot metadata: {0}")]
    ParseMetadata(#[source] serde_json::Error),
    #[error("failed to copy file from {from} to {to}: {source}")]
    Copy {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("snapshot {id} not found")]
    SnapshotNotFound { id: String },
    #[error("failed to restore file {path}: {source}")]
    Restore {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub reason: String,
    pub manifest_path: PathBuf,
    pub lockfile_path: Option<PathBuf>,
    pub changed_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiff {
    pub added_files: Vec<PathBuf>,
    pub removed_files: Vec<PathBuf>,
    pub modified_files: Vec<PathBuf>,
}

pub struct SnapshotStore {
    project_dir: PathBuf,
    snapshots_dir: PathBuf,
}

impl SnapshotStore {
    pub fn new(project_dir: impl AsRef<Path>) -> Self {
        let project_dir = project_dir.as_ref().to_path_buf();
        let snapshots_dir = project_dir.join(".tuffbox").join("snapshots");
        Self {
            project_dir,
            snapshots_dir,
        }
    }

    pub fn ensure_snapshots_dir(&self) -> Result<(), SnapshotError> {
        fs::create_dir_all(&self.snapshots_dir).map_err(SnapshotError::ReadMetadata)
    }

    pub fn create(
        &self,
        name: impl Into<String>,
        reason: impl Into<String>,
        manifest_path: impl AsRef<Path>,
        lockfile_path: Option<impl AsRef<Path>>,
        changed_files: &[impl AsRef<Path>],
    ) -> Result<Snapshot, SnapshotError> {
        self.ensure_snapshots_dir()?;

        let name: String = name.into();
        let reason: String = reason.into();
        let id = format!("{}-{}", slugify(&name), rfc3339_now_compact());
        let snapshot_dir = self.snapshots_dir.join(&id);
        fs::create_dir_all(&snapshot_dir).map_err(SnapshotError::ReadMetadata)?;

        let manifest_src = manifest_path.as_ref();
        let manifest_dst = snapshot_dir.join("manifest.json");
        copy_file(manifest_src, &manifest_dst)?;

        let lockfile_dst = if let Some(lockfile_path) = &lockfile_path {
            let src = lockfile_path.as_ref();
            let dst = snapshot_dir.join("lockfile.json");
            copy_file(src, &dst)?;
            Some(dst)
        } else {
            None
        };

        let mut copied_changed_files = Vec::new();
        let changed_files_dir = snapshot_dir.join("changed_files");
        for relative_path in changed_files {
            let relative = relative_path.as_ref();
            let src = self.project_dir.join(relative);
            let dst = changed_files_dir.join(relative);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).map_err(SnapshotError::ReadMetadata)?;
            }
            copy_file(&src, &dst)?;
            copied_changed_files.push(relative.to_path_buf());
        }

        let snapshot = Snapshot {
            id,
            name,
            created_at: rfc3339_now(),
            reason,
            manifest_path: manifest_dst,
            lockfile_path: lockfile_dst,
            changed_files: copied_changed_files,
        };

        let meta_path = snapshot_dir.join("snapshot.json");
        let meta_json =
            serde_json::to_string_pretty(&snapshot).expect("snapshot metadata should serialize");
        fs::write(&meta_path, meta_json).map_err(SnapshotError::ReadMetadata)?;

        Ok(snapshot)
    }

    pub fn list(&self) -> Result<Vec<Snapshot>, SnapshotError> {
        if !self.snapshots_dir.exists() {
            return Ok(Vec::new());
        }

        let mut snapshots = Vec::new();
        for entry in fs::read_dir(&self.snapshots_dir).map_err(SnapshotError::ReadMetadata)? {
            let entry = entry.map_err(SnapshotError::ReadMetadata)?;
            let meta_path = entry.path().join("snapshot.json");
            if !meta_path.is_file() {
                continue;
            }
            let raw = fs::read_to_string(&meta_path).map_err(SnapshotError::ReadMetadata)?;
            let snapshot: Snapshot =
                serde_json::from_str(&raw).map_err(SnapshotError::ParseMetadata)?;
            snapshots.push(snapshot);
        }

        snapshots.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        Ok(snapshots)
    }

    pub fn get(&self, id: impl AsRef<str>) -> Result<Option<Snapshot>, SnapshotError> {
        let id = id.as_ref();
        let meta_path = self.snapshots_dir.join(id).join("snapshot.json");
        if !meta_path.is_file() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&meta_path).map_err(SnapshotError::ReadMetadata)?;
        let snapshot: Snapshot =
            serde_json::from_str(&raw).map_err(SnapshotError::ParseMetadata)?;
        Ok(Some(snapshot))
    }

    pub fn diff(
        &self,
        from_id: impl AsRef<str>,
        to_id: impl AsRef<str>,
    ) -> Result<SnapshotDiff, SnapshotError> {
        let from = self
            .get(&from_id)?
            .ok_or_else(|| SnapshotError::SnapshotNotFound {
                id: from_id.as_ref().to_string(),
            })?;
        let to = self
            .get(&to_id)?
            .ok_or_else(|| SnapshotError::SnapshotNotFound {
                id: to_id.as_ref().to_string(),
            })?;

        let from_files: std::collections::HashSet<_> = from.changed_files.iter().cloned().collect();
        let to_files: std::collections::HashSet<_> = to.changed_files.iter().cloned().collect();

        let from_changed_dir = self.snapshots_dir.join(&from.id).join("changed_files");
        let to_changed_dir = self.snapshots_dir.join(&to.id).join("changed_files");
        let mut modified_files = Vec::new();
        for relative in from_files.intersection(&to_files) {
            let from_path = from_changed_dir.join(relative);
            let to_path = to_changed_dir.join(relative);
            if files_differ(&from_path, &to_path).map_err(SnapshotError::ReadMetadata)? {
                modified_files.push(relative.clone());
            }
        }

        Ok(SnapshotDiff {
            added_files: to_files.difference(&from_files).cloned().collect(),
            removed_files: from_files.difference(&to_files).cloned().collect(),
            modified_files,
        })
    }

    pub fn rollback(&self, id: impl AsRef<str>) -> Result<Snapshot, SnapshotError> {
        let snapshot = self
            .get(&id)?
            .ok_or_else(|| SnapshotError::SnapshotNotFound {
                id: id.as_ref().to_string(),
            })?;

        let manifest_dst = find_project_manifest(&self.project_dir)
            .unwrap_or_else(|| self.project_dir.join("project.tuffbox.json"));
        copy_file(&snapshot.manifest_path, &manifest_dst)?;

        if let Some(lockfile_path) = &snapshot.lockfile_path {
            let lockfile_dst = self.project_dir.join("project.tuffbox.lock.json");
            copy_file(lockfile_path, &lockfile_dst)?;
        }

        let changed_files_dir = self.snapshots_dir.join(&snapshot.id).join("changed_files");
        for relative_path in &snapshot.changed_files {
            let src = changed_files_dir.join(relative_path);
            let dst = self.project_dir.join(relative_path);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).map_err(|source| SnapshotError::Restore {
                    path: dst.clone(),
                    source,
                })?;
            }
            copy_file(&src, &dst)?;
        }

        Ok(snapshot)
    }
}

fn find_project_manifest(project_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(project_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".tuffbox.json"))
                .unwrap_or(false)
        })
}

fn files_differ(left: &Path, right: &Path) -> std::io::Result<bool> {
    if !left.exists() || !right.exists() {
        return Ok(left.exists() != right.exists());
    }
    let left_meta = fs::metadata(left)?;
    let right_meta = fs::metadata(right)?;
    if left_meta.len() != right_meta.len() {
        return Ok(true);
    }
    Ok(fs::read(left)? != fs::read(right)?)
}

fn copy_file(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<(), SnapshotError> {
    let from = from.as_ref();
    let to = to.as_ref();
    fs::copy(from, to).map_err(|source| SnapshotError::Copy {
        from: from.to_path_buf(),
        to: to.to_path_buf(),
        source,
    })?;
    Ok(())
}

fn slugify(name: &str) -> String {
    name.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric(), "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

/// Returns the current UTC time as an RFC 3339 timestamp, e.g.
/// `2026-06-29T12:34:56Z`.
///
/// Previously this returned a hardcoded fake date (`2026-06-29T00:00:00Z`)
/// for *every* snapshot, which broke history ordering (snapshots sorted by
/// `created_at` all compared equal) and, combined with
/// [`rfc3339_now_compact`] feeding the same frozen timestamp into snapshot
/// IDs, meant two snapshots created with the same name on the same "day"
/// silently collided and overwrote each other on disk.
fn rfc3339_now() -> String {
    crate::time_util::rfc3339_now()
}

/// Returns the current UTC time formatted for use inside a snapshot ID
/// (`YYYYMMDDTHHMMSSZ`), safe to use as a path segment (no colons).
fn rfc3339_now_compact() -> String {
    crate::time_util::compact_now()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_project() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let project_dir = dir.path().to_path_buf();
        let manifest_path = project_dir.join("project.tuffbox.json");
        let mut file = fs::File::create(&manifest_path).unwrap();
        writeln!(file, "{{\"schemaVersion\":\"0.1.0\"}}").unwrap();
        (dir, project_dir)
    }

    #[test]
    fn creates_and_lists_snapshot() {
        let (_dir, project_dir) = temp_project();
        let store = SnapshotStore::new(&project_dir);
        let manifest_path = project_dir.join("project.tuffbox.json");

        let snapshot = store
            .create(
                "before-update",
                "manual",
                &manifest_path,
                None::<&Path>,
                &[] as &[&Path],
            )
            .unwrap();

        assert_eq!(snapshot.name, "before-update");
        let snapshots = store.list().unwrap();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].id, snapshot.id);
    }

    #[test]
    fn rolls_back_manifest() {
        let (_dir, project_dir) = temp_project();
        let store = SnapshotStore::new(&project_dir);
        let manifest_path = project_dir.join("project.tuffbox.json");

        let snapshot = store
            .create(
                "baseline",
                "manual",
                &manifest_path,
                None::<&Path>,
                &[] as &[&Path],
            )
            .unwrap();

        fs::write(&manifest_path, "{\"modified\":true}").unwrap();
        store.rollback(&snapshot.id).unwrap();

        let restored = fs::read_to_string(&manifest_path).unwrap();
        assert!(restored.contains("schemaVersion"));
    }

    #[test]
    fn snapshot_timestamps_are_not_hardcoded() {
        // Regression test: `rfc3339_now`/`rfc3339_now_compact` used to
        // return a frozen fake date, so every snapshot had an identical
        // `created_at` and snapshots with the same name could collide by
        // ID. Verify the timestamp actually reflects wall-clock time
        // instead of a magic constant.
        let now = rfc3339_now();
        assert_ne!(now, "2026-06-29T00:00:00Z", "timestamp looks hardcoded: {now}");
        assert!(now.ends_with('Z'));
    }
}
