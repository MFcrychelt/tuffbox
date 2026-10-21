//! Single validation point for paths that come from untrusted data.
//!
//! Anything the launcher did not author itself — a CurseForge `overrides/`
//! entry, a `.mrpack` file `path`, a packwiz index entry, a GitHub pack asset,
//! a downloaded file name — MUST pass through here before it is joined under an
//! instance directory. `Path::join` silently *replaces* the base for an absolute
//! argument and happily walks out of the root for `..`; these checks turn that
//! into an explicit error (see `docs/18-millida-launcher-reference.md` §3.2).
//!
//! The checks are platform-independent on purpose: an archive extracted on
//! Linux must not produce an instance that Windows cannot open later, so
//! Windows-only hazards (reserved device names, trailing dot/space) are
//! rejected everywhere.

use std::path::{Path, PathBuf};

/// Why a candidate relative path was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("path is empty")]
    Empty,
    #[error("path is absolute")]
    Absolute,
    #[error("path contains ':' (drive letter or NTFS alternate data stream)")]
    DriveOrColon,
    #[error("path contains a '.' or '..' segment")]
    Traversal,
    #[error("path contains a control character")]
    ControlChar,
    #[error("path uses a reserved Windows device name")]
    ReservedName,
    #[error("path segment ends with a dot or space")]
    TrailingDotOrSpace,
}

/// Windows reserves these device names in *every* directory, with or without
/// an extension (`CON`, `con.txt` and `NUL.` are all unusable). `COM0`/`LPT0`
/// are ordinary files, hence the 1–9 range.
fn is_windows_reserved(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or(segment);
    let upper = stem.trim_end_matches([' ', '.']).to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    upper.len() == 4
        && (upper.starts_with("COM") || upper.starts_with("LPT"))
        && matches!(upper.as_bytes()[3], b'1'..=b'9')
}

/// Normalize separators and validate a path that came from untrusted data.
///
/// On success returns the normalized (`/`-separated) relative path. `a//b` is
/// accepted and collapses; `.`/`..` segments, absolute paths, drive letters,
/// control characters, Windows-reserved names and trailing dot/space segments
/// are rejected.
pub fn validate_rel_path(name: &str) -> Result<String, PathError> {
    let normalized = name.replace('\\', "/");
    if normalized.is_empty() {
        return Err(PathError::Empty);
    }
    if normalized.starts_with('/') {
        return Err(PathError::Absolute);
    }
    if normalized.contains(':') {
        return Err(PathError::DriveOrColon);
    }
    if normalized.chars().any(char::is_control) {
        return Err(PathError::ControlChar);
    }
    for segment in normalized.split('/') {
        if segment == "." || segment == ".." {
            return Err(PathError::Traversal);
        }
        if segment.is_empty() {
            continue; // `a//b` — collapse rather than reject.
        }
        if segment.ends_with('.') || segment.ends_with(' ') {
            return Err(PathError::TrailingDotOrSpace);
        }
        if is_windows_reserved(segment) {
            return Err(PathError::ReservedName);
        }
    }
    Ok(normalized)
}

/// Convenience predicate over [`validate_rel_path`] for callers that only need
/// a boolean (e.g. "is this archive entry safe to extract?").
pub fn is_safe_rel_path(name: &str) -> bool {
    validate_rel_path(name).is_ok()
}

/// Validate `rel`, then join it under `root` one segment at a time.
///
/// Segment-wise `push` (rather than `root.join(rel)`) keeps the guarantee
/// explicit: nothing that survives [`validate_rel_path`] can change the base
/// directory.
pub fn safe_join(root: impl AsRef<Path>, rel: &str) -> Result<PathBuf, PathError> {
    let rel = validate_rel_path(rel)?;
    let mut out = root.as_ref().to_path_buf();
    for segment in rel.split('/').filter(|s| !s.is_empty()) {
        out.push(segment);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_ordinary_paths() {
        for name in [
            "mods/sodium.jar",
            "config/example.cfg",
            "a/b/c/d.txt",
            "mods/My Mod 1.2.jar",
            "конфиг/настройки.toml",
            "a//b.txt",
            "..hidden/ok.txt",
        ] {
            assert!(is_safe_rel_path(name), "should accept {name:?}");
        }
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        for name in [
            "../evil.txt",
            "a/../../evil.txt",
            "a/./b.txt",
            "/etc/passwd",
            "\\server\\share\\x",
            "C:/Windows/System32/x.dll",
            "c:evil.txt",
            "mods/x.txt:stream",
            "",
        ] {
            assert!(!is_safe_rel_path(name), "should reject {name:?}");
        }
        assert_eq!(validate_rel_path("../x"), Err(PathError::Traversal));
        assert_eq!(validate_rel_path("/x"), Err(PathError::Absolute));
        assert_eq!(validate_rel_path("C:\\x"), Err(PathError::DriveOrColon));
        assert_eq!(validate_rel_path(""), Err(PathError::Empty));
    }

    #[test]
    fn rejects_windows_hazards() {
        assert_eq!(validate_rel_path("mods/CON"), Err(PathError::ReservedName));
        assert_eq!(validate_rel_path("nul.txt"), Err(PathError::ReservedName));
        assert_eq!(validate_rel_path("a/com5.jar"), Err(PathError::ReservedName));
        assert_eq!(validate_rel_path("lpt9"), Err(PathError::ReservedName));
        // COM0/LPT0 are ordinary names.
        assert!(is_safe_rel_path("com0.txt"));
        assert!(is_safe_rel_path("console.txt"));

        assert_eq!(validate_rel_path("mods/x."), Err(PathError::TrailingDotOrSpace));
        assert_eq!(validate_rel_path("mods/x "), Err(PathError::TrailingDotOrSpace));
        assert_eq!(validate_rel_path("bad\u{7}name"), Err(PathError::ControlChar));
    }

    #[test]
    fn safe_join_never_escapes_root() {
        let root = Path::new("/tmp/instance");
        assert_eq!(
            safe_join(root, "mods/a.jar").unwrap(),
            Path::new("/tmp/instance/mods/a.jar")
        );
        assert!(safe_join(root, "../../etc/passwd").is_err());
        assert!(safe_join(root, "/etc/passwd").is_err());
        assert!(safe_join(root, "C:/x").is_err());
    }

    #[test]
    fn backslashes_are_normalized_not_trusted() {
        // A Windows-style relative path is accepted, but normalized so the
        // traversal check sees every segment.
        assert_eq!(validate_rel_path("mods\\a.jar").unwrap(), "mods/a.jar");
        assert_eq!(validate_rel_path("a\\..\\evil"), Err(PathError::Traversal));
    }
}
