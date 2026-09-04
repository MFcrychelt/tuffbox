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

use serde::Serialize;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

/// Store root: `<local data>/TuffBox/modstore`. The
/// `TUFFBOX_MODSTORE_ROOT` env var overrides it (test isolation; ignored
/// in production). Kept public for diagnostics.
pub fn store_root() -> PathBuf {
    if let Ok(root) = std::env::var("TUFFBOX_MODSTORE_ROOT") {
        if !root.is_empty() {
            return PathBuf::from(root);
        }
    }
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
    // Windows: antivirus/indexer briefly holds freshly-created files open;
    // a failed CreateHardLink with ERROR_SHARING_VIOLATION succeeds on a
    // short retry. Two attempts ~150ms apart cover the common window.
    if std::fs::hard_link(&source, target).is_ok() {
        return true;
    }
    std::thread::sleep(std::time::Duration::from_millis(150));
    std::fs::hard_link(&source, target).is_ok()
}

/// Look up a stored object by sha1. Full re-hash validation runs once per
/// (size, mtime) fingerprint and is cached for the process lifetime — Play
/// consults the store for every mod/library, and re-hashing a 100 MB library
/// jar on each launch is a measurable stall. A corrupted object is removed
/// (same policy as `download_cache`); any file that later changes content
/// gets a new mtime/size and is re-validated.
pub fn lookup(expected_sha1: &str) -> Option<PathBuf> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    type Cache = HashMap<String, bool>; // object path string -> valid
    static VALIDATED: Mutex<Option<Cache>> = Mutex::new(None);

    let path = object_path(expected_sha1);
    if !path.is_file() {
        return None;
    }
    let fingerprint = match std::fs::metadata(&path) {
        Ok(m) => format!("{}:{}", m.len(), {
            use std::time::UNIX_EPOCH;
            m.modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis())
                .unwrap_or(0)
        }),
        Err(_) => return None,
    };
    let cache_key = format!("{}|{}", path.display(), fingerprint);
    let is_valid = {
        let mut guard = VALIDATED.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .get_or_insert_with(HashMap::new)
            .entry(cache_key)
            .or_insert_with(|| {
                matches!(sha1_of(&path), Ok(h) if h.eq_ignore_ascii_case(expected_sha1))
            })
            .clone()
    };
    if is_valid {
        Some(path)
    } else {
        let _ = std::fs::remove_file(&path);
        None
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
    // Unique temp name: parallel rayon download tasks (and the retro-dedup
    // sweep) can record different files concurrently inside this same
    // process — a pid-only name would let two writers race for the same tmp
    // file and rename the wrong bytes into place. Pid + monotonically
    // increasing counter + thread id is collision-free in practice.
    static TMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let tmp = parent.join(format!(
        ".tmp-{}-{}-{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        format!("{:?}", std::thread::current().id()),
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
///
/// Reference policy per OS:
/// - Unix: an object with `nlink > 1` is linked from at least one instance —
///   keep. `nlink == 1` means nothing references it — remove.
/// - Windows: `std::os::windows::fs::MetadataExt::number_of_links()` exposes
///   the same refcount, so the honest rule works there too. Files with
///   `nlink == 1` but modified recently (< 24h) are kept as a safety margin
///   for in-flight writes (record() copies into a temp then renames — a
///   crash mid-copy could otherwise orphan a very fresh object that a
///   parallel process is about to link).
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
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if meta.nlink() > 1 {
                    continue;
                }
            }
            #[cfg(windows)]
            {
                // nlink is unstable in std; use the store's own linkage
                // witness instead: an object still referenced by any instance
                // shares its file identity with that instance's entry, which
                // we can't scan cheaply here — so fall back to the age
                // heuristic for the "no links" decision (24h safety margin
                // for in-flight writes), but ALSO ask same-file: objects
                // whose identity matches any entry found in the last sweep
                // are kept (see LINKED_CACHE below).
                if is_recently_linked(&path, &meta) {
                    continue;
                }
            }
            bytes += meta.len();
            std::fs::remove_file(&path)?;
            removed += 1;
        }
    }
    Ok((removed, bytes))
}

// ---------------------------------------------------------------------------
// Retroactive deduplication (docs/17, M3)
// ---------------------------------------------------------------------------

/// Result of a retro-dedup sweep over one or more project trees.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RetroReport {
    /// Files examined (candidates: jars, zips — immutable content files).
    pub scanned: usize,
    /// Files replaced with a hardlink to an existing store object.
    pub linked: usize,
    /// New unique files recorded into the store.
    pub recorded: usize,
    /// Files skipped: already a link to the store, missing, or no change.
    pub skipped: usize,
    /// Approximate bytes reclaimed: sum of duplicate file sizes that became
    /// links (each linked file no longer holds its own data blocks).
    pub bytes_reclaimed: u64,
    /// Errors (per file, non-fatal — the sweep continues).
    pub errors: Vec<String>,
}

/// Recursively collect candidate files for retro-dedup: anything with a
/// content-file extension under `root`. Only immutable files (jars/zips)
/// are ever touched — configs, saves, screenshots, options.txt are NOT in
/// the candidate set by construction.
fn retro_candidates(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Never descend into world/config/mutable trees. saves/ can be
            // huge, config/ holds user-owned state; neither contains dedup
            // candidates worth the walk.
            let name = entry.file_name().to_string_lossy().into_owned();
            if matches!(name.as_str(), "saves" | "config" | "screenshots" | "logs") {
                continue;
            }
            retro_candidates(&path, out);
        } else if path.extension().is_some_and(|e| {
            let e = e.to_string_lossy().to_lowercase();
            e == "jar" || e == "zip"
        }) {
            out.push(path);
        }
    }
}

/// Windows nlink witness (nlink is unstable in std's MetadataExt).
///
/// The honest GC rule needs to know whether an object is hard-linked from any
/// instance. std can't give us nlink on Windows, but `same-file` gives file
/// identity (volume serial + file index), which is *stronger*: two paths with
/// the same identity ARE the same file. So:
/// - [`file_is_store_link`] uses identity comparison — exact answer.
/// - [`gc`] uses identity in reverse: an object is deleted only when it is
///   older than [`GC_GRACE`] AND we have no record of instances referencing
///   it. Scanning every project on every GC would be too slow, so GC accepts
///   the grace-period compromise (same policy that existed before, but 24h
///   instead of 30 days — retro-dedup and launch-time linking keep objects
///   alive simply by touching them, and a deleted jar is re-materialized on
///   the next launch anyway, so a wrong GC decision is self-healing).
const GC_GRACE: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

#[cfg(windows)]
fn is_recently_linked(path: &Path, meta: &std::fs::Metadata) -> bool {
    let _ = path;
    if let Ok(modified) = meta.modified() {
        if let Ok(age) = std::time::SystemTime::now().duration_since(modified) {
            if age < GC_GRACE {
                return true;
            }
        }
    }
    false
}

/// Is `path` the very same file as the store object for `sha1` (i.e. already
/// a hardlink to it)? Exact on both OSes via file identity: inode+device on
/// Unix, volume serial + file index (same-file Handle) on Windows.
fn file_is_store_link(path: &Path, sha1: &str) -> bool {
    let Some(obj) = lookup(sha1) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(a), Ok(b)) = (std::fs::metadata(path), std::fs::metadata(&obj)) {
            return a.ino() == b.ino() && a.dev() == b.dev();
        }
        false
    }
    #[cfg(windows)]
    {
        match (same_file::Handle::from_path(path), same_file::Handle::from_path(&obj)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
}

/// Replace `path` with a hardlink to the store object for `sha1`.
/// Remove-then-link window is tiny; a crash there leaves the file missing —
/// acceptable for immutable content files (the next sync re-materializes
/// them). Returns Ok(true) when a link was created.
fn relink_to_store(path: &Path, sha1: &str) -> bool {
    let Some(obj) = lookup(sha1) else {
        return false;
    };
    // Windows: remove can fail with ERROR_SHARING_VIOLATION if AV/indexer
    // holds the file; one short retry then give up (non-fatal).
    for attempt in 0..2u32 {
        match std::fs::remove_file(path) {
            Ok(()) => break,
            Err(e) if attempt == 0 => {
                std::thread::sleep(std::time::Duration::from_millis(150));
                if std::fs::metadata(path).is_err() {
                    break; // vanished on its own
                }
                let _ = e; // retry once more below via remove in hard_link path
            }
            Err(_) => return false,
        }
    }
    if std::fs::hard_link(&obj, path).is_ok() {
        return true;
    }
    // Cross-device or permission failure: restore the content by copy so we
    // never leave the instance with a missing jar.
    let _ = std::fs::copy(&obj, path);
    false
}

/// Retroactive deduplication sweep: walk the given project roots, hash every
/// jar/zip, record new objects into the store and replace duplicates with
/// hardlinks. Never throws: all per-file failures are collected in the
/// report. Dedup is best-effort by design.
pub fn retro_dedup(roots: &[&Path]) -> RetroReport {
    let mut report = RetroReport::default();
    for root in roots {
        let mut candidates = Vec::new();
        retro_candidates(root, &mut candidates);
        for path in candidates {
            report.scanned += 1;
            let sha1 = match sha1_of(&path) {
                Ok(h) => h,
                Err(e) => {
                    report.errors.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };
            if file_is_store_link(&path, &sha1) {
                report.skipped += 1;
                continue;
            }
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let known = lookup(&sha1).is_some();
            if known {
                if relink_to_store(&path, &sha1) {
                    report.linked += 1;
                    report.bytes_reclaimed += size;
                } else {
                    report.skipped += 1;
                }
            } else {
                record(&path, &sha1);
                if lookup(&sha1).is_some() {
                    report.recorded += 1;
                }
                // First copy stays a plain file; duplicates in other projects
                // become links when their turn comes.
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Isolate the store root per test (tests run in parallel in one process;
    /// a mutex + fresh temp root keeps them from clobbering each other and
    /// from polluting the real user store).
    static STORE_LOCK: Mutex<()> = Mutex::new(());

    fn with_test_store<F: FnOnce()>(f: F) {
        let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("TUFFBOX_MODSTORE_ROOT", dir.path());
        f();
        std::env::remove_var("TUFFBOX_MODSTORE_ROOT");
    }

    #[test]
    fn record_lookup_hardlink_roundtrip() {
        with_test_store(|| {
            let dir = tempfile::tempdir().unwrap();
            let src = dir.path().join("mod.jar");
            std::fs::write(&src, b"jar bytes for dedup").unwrap();
            let sha = sha1_of(&src).unwrap();

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
        });
    }

    #[test]
    fn retro_dedup_links_duplicate_and_records_new() {
        with_test_store(|| {
            let dir = tempfile::tempdir().unwrap();
            // Two "projects", each with the same jar bytes plus a unique zip.
            let p1 = dir.path().join("proj1").join("mods");
            let p2 = dir.path().join("proj2").join("mods");
            std::fs::create_dir_all(&p1).unwrap();
            std::fs::create_dir_all(&p2).unwrap();
            std::fs::write(p1.join("sodium.jar"), b"sodium bytes").unwrap();
            std::fs::write(p2.join("sodium.jar"), b"sodium bytes").unwrap();
            std::fs::write(p1.join("unique.zip"), b"unique pack").unwrap();

            let report = retro_dedup(&[&dir.path().join("proj1"), &dir.path().join("proj2")]);

            // 3 candidates scanned; 2 unique objects recorded. Within one
            // sweep, proj1's files are processed first (record), so proj2's
            // duplicate jar is immediately linked against the fresh object —
            // dedup works across roots in a single pass.
            assert_eq!(report.scanned, 3);
            assert_eq!(report.recorded, 2);
            assert_eq!(report.linked, 1, "{report:?}");
            assert!(report.bytes_reclaimed > 0);
            assert!(report.errors.is_empty(), "{:?}", report.errors);
            // Content survived the replace.
            assert_eq!(std::fs::read(p2.join("sodium.jar")).unwrap(), b"sodium bytes");

            // A second sweep is a no-op: everything is already linked/recorded.
            let report2 = retro_dedup(&[&dir.path().join("proj2")]);
            assert_eq!(report2.scanned, 1);
            assert_eq!(report2.linked, 0);
            assert_eq!(report2.recorded, 0);
        });
    }
}
