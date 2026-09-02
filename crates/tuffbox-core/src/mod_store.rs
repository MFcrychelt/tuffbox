//! Global content-addressed mod store with hard links (Shard
//! Launcher-inspired dedup): `objects/<xx>/<sha1>` shared across ALL
//! instances, so the same jar installed in N profiles is stored once on disk.
//!
//! Flow in [`super::mod_files::materialize_mod_file_with_progress`]:
//! 1. the store is consulted by sha1 — a hit hard-links the object into the
//!    instance's mods folder (no download, no extra disk space);
//! 2. a fresh download is recorded into the store before the caller counts
//!    it as success.
//!
//! Hard links keep per-instance mod folders fully self-contained from the
//! game's point of view (same inode, independent directory entries), and a
//! deleted instance folder never corrupts the store object.
//!
//! The store root lives in the user's local data dir (not per-project), so
//! dedup works across instances in different locations too. Write failures
//! are non-fatal: dedup is an optimization, callers fall back to plain
//! download/copy.

use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

/// Store root: `<local data>/TuffBox/modstore`. Kept public for diagnostics.
pub fn store_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("modstore")
}

/// Object path for a sha1: `objects/<aa>/<rest>`.
fn object_path(sha1_hex: &str) -> PathBuf {
    let sha1_hex = sha1_hex.to_lowercase();
    let (prefix, rest) = sha1_hex.split_at(2);
    store_root().join("objects").join(prefix).join(rest)
}

/// SHA-1 of a file, streamed (mirrors `mc_install::sha1_file`).
fn sha1_of(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

use std::io::Read;

/// Try to materialize `target` by hard-linking a stored object whose sha1 is
/// `expected_sha1`. Returns `Ok(true)` when the link was created (caller is
/// done — no download needed). Never throws: any failure means "no hit".
pub fn try_hardlink(target: &Path, expected_sha1: &str) -> bool {
    let Some(source) = lookup(expected_sha1) else {
        return false;
    };
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if target.exists() {
        // Another parallel task may have materialized it first.
        return sha1_of(target)
            .map(|h| h.eq_ignore_ascii_case(expected_sha1))
            .unwrap_or(false);
    }
    std::fs::hard_link(&source, target).is_ok()
}

/// Look up a stored object by sha1; validates the bytes still hash correctly
/// (a corrupted object is removed, same policy as `download_cache`).
pub fn lookup(expected_sha1: &str) -> Option<PathBuf> {
    let path = object_path(expected_sha1);
    if !path.is_file() {
        return None;
    }
    match sha1_of(&path) {
        Ok(h) if h.eq_ignore_ascii_case(expected_sha1) => Some(path),
        _ => {
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

/// Record a freshly downloaded/verified file into the store. The object is
/// written atomically (temp + rename in the same dir). Non-fatal on failure.
pub fn record(file: &Path, expected_sha1: &str) {
    let obj = object_path(expected_sha1);
    if obj.is_file() {
        return; // already stored
    }
    // Trust but verify: only index files that really hash to expected.
    let actual = match sha1_of(file) {
        Ok(h) => h,
        Err(_) => return,
    };
    if !actual.eq_ignore_ascii_case(expected_sha1) {
        return;
    }
    let Some(parent) = obj.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let tmp = parent.join(format!(
        ".tmp-{}",
        std::process::id()
    ));
    if std::fs::copy(file, &tmp).is_err() {
        return;
    }
    // Another process may have recorded it in the meantime — either way the
    // rename lands a valid object.
    let _ = std::fs::rename(&tmp, &obj);
    // Prune sibling instances of the same file: if the target is a copy
    // (not a link), leave it as-is; callers that want links use try_hardlink
    // before downloading.
}

/// Delete orphaned objects no longer hard-linked from any instance.
/// Returns the number of removed files and bytes reclaimed. Only walks
/// `objects/`; hard-link refcounts are the filesystem's job (st_nlink).
pub fn gc() -> std::io::Result<(usize, u64)> {
    let objects = store_root().join("objects");
    if !objects.is_dir() {
        return Ok((0, 0));
    }
    let mut removed = 0usize;
    let mut bytes = 0u64;
    for prefix in std::fs::read_dir(&objects)?.flatten() {
        let prefix_path = prefix.path();
        if !prefix_path.is_dir() {
            continue;
        }
        for obj in std::fs::read_dir(&prefix_path)?.flatten() {
            let path = obj.path();
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            // On Unix, nlink == 1 means no instance links this object.
            // On Windows hard links also expose nlink via metadata; a plain
            // copy in some instance folder does not affect this object's
            // nlink (that's fine — GC only removes unreferenced store files).
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if meta.nlink() > 1 {
                    continue;
                }
            }
            #[cfg(windows)]
            {
                // std does not expose nlink portably; conservatively keep
                // objects touched within the last 30 days.
                if let Ok(modified) = meta.modified() {
                    if let Ok(age) = std::time::SystemTime::now().duration_since(modified) {
                        if age < std::time::Duration::from_secs(30 * 24 * 3600) {
                            continue;
                        }
                    }
                }
            }
            bytes += meta.len();
            std::fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok((removed, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_lookup_hardlink_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("mod.jar");
        std::fs::write(&src, b"jar bytes for dedup").unwrap();
        let sha = sha1_of(&src).unwrap();

        // Record into the (real) store — safe: uses user-local data dir.
        record(&src, &sha);
        assert!(lookup(&sha).is_some());

        // Hard-link into a fresh instance folder.
        let inst = dir.path().join("inst1").join("mods");
        std::fs::create_dir_all(&inst).unwrap();
        let target = inst.join("mod.jar");
        assert!(try_hardlink(&target, &sha));
        assert_eq!(std::fs::read(&target).unwrap(), b"jar bytes for dedup");

        // Idempotent: linking when target already exists with same hash → true.
        assert!(try_hardlink(&target, &sha));

        // Unknown hash → no hit.
        assert!(!try_hardlink(
            &inst.join("other.jar"),
            "0000000000000000000000000000000000000000"
        ));
    }
}
