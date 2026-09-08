//! Small filesystem helpers shared by quest / project writers.

use std::io::Write;
use std::path::{Path, PathBuf};

/// Write `contents` to `path` via a temp file in the same directory, then rename.
///
/// Avoids truncated/corrupt targets if the process crashes mid-write.
pub fn atomic_write(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".tuffbox-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|e| e.to_string())?;
    tmp.write_all(contents.as_ref())
        .map_err(|e| e.to_string())?;
    tmp.flush().map_err(|e| e.to_string())?;
    tmp.persist(path)
        .map(|_| ())
        .map_err(|e| e.error.to_string())
}

/// Write `contents` to `path` atomically, keeping a timestamped backup
/// (`<name>.bak-<unix_ms>`) of the previous content — Reality
/// Launcher-style crash/update resistance. The backup lets callers restore
/// the last known-good copy if the new content turns out unreadable.
pub fn atomic_write_with_backup(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), String> {
    if path.is_file() {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let mut bak = path.as_os_str().to_owned();
        bak.push(format!(".bak-{}", ms));
        std::fs::copy(path, PathBuf::from(bak)).map_err(|e| e.to_string())?;
    }
    atomic_write(path, contents)
}

/// Restore `path` from its newest `*.bak-<ts>` sibling if the file is missing
/// or empty. Returns Ok(true) when a backup was restored, Ok(false) when the
/// live file was already fine, Err on restore failure.
pub fn restore_from_backup(path: &Path) -> Result<bool, String> {
    if path.is_file() {
        let ok = std::fs::metadata(path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
        if ok {
            return Ok(false);
        }
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut best: Option<(u128, PathBuf)> = None;
    let entries = std::fs::read_dir(parent).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(ts) = name
            .strip_prefix(&format!("{}.bak-", stem))
            .and_then(|s| s.parse::<u128>().ok())
        {
            if best.as_ref().map(|(t, _)| ts > *t).unwrap_or(true) {
                best = Some((ts, entry.path()));
            }
        }
    }
    match best {
        Some((_, bak_path)) => {
            std::fs::copy(&bak_path, path).map_err(|e| e.to_string())?;
            Ok(true)
        }
        None => Err(format!("no backup found for {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.txt");
        atomic_write(&path, b"one").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"one");
        atomic_write(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
    }

    #[test]
    fn backup_write_keeps_previous_and_restore_recovers_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conf.json");

        // First write: no backup yet.
        atomic_write_with_backup(&path, b"good").unwrap();
        assert!(restore_from_backup(&path).unwrap() == false);

        // Second write overwrites, keeping "good" as a .bak-<ts> sibling.
        std::thread::sleep(std::time::Duration::from_millis(2));
        atomic_write_with_backup(&path, b"broken").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"broken");
        let has_bak = std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with("conf.json.bak-"));
        assert!(has_bak);

        // Simulate corruption (empty file), then restore.
        std::fs::write(&path, b"").unwrap();
        assert_eq!(restore_from_backup(&path).unwrap(), true);
        assert_eq!(std::fs::read(&path).unwrap(), b"good");
    }

    #[test]
    fn restore_picks_newest_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.json");
        std::fs::write(dir.path().join("s.json.bak-100"), b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        std::fs::write(dir.path().join("s.json.bak-200"), b"new").unwrap();
        std::fs::write(&path, b"").unwrap();
        assert_eq!(restore_from_backup(&path).unwrap(), true);
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }
}
