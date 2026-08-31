//! Small filesystem helpers shared by quest / project writers.

use std::io::Write;
use std::path::Path;

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
}
