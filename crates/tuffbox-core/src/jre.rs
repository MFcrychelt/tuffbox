use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse java version")]
    InvalidVersion,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct JavaRuntime {
    pub path: String,
    pub version: String,
    pub major: u32,
}

/// Returns the minimum Java major version Mojang requires for a given
/// Minecraft release, per the version manifest's `javaVersion` field
/// (hardcoded here for the common release boundaries since we don't fetch
/// per-version manifests just for this check).
///
/// This exists because [`find_all_runtimes`]/callers previously picked
/// "the newest installed Java" unconditionally, with no regard for what
/// the target Minecraft/loader version actually needs. That silently
/// launches e.g. Forge 1.20.1 (which needs Java 17) on a Java 21 JVM,
/// which fails deep inside Forge's bootstrap launcher with a confusing
/// `InaccessibleObjectException` instead of a clear "wrong Java version"
/// message.
pub fn required_java_major(mc_version: &str) -> u32 {
    let parts: Vec<u32> = mc_version
        .split('.')
        .filter_map(|p| p.split('-').next().and_then(|p| p.parse().ok()))
        .collect();
    let minor = parts.get(1).copied().unwrap_or(0);
    let patch = parts.get(2).copied().unwrap_or(0);

    if minor >= 21 || (minor == 20 && patch >= 5) {
        21
    } else if minor >= 18 {
        17
    } else if minor == 17 {
        16
    } else {
        8
    }
}

/// Picks the best installed runtime for a required Java major version:
/// an exact match if available, otherwise the closest newer version (JVMs
/// are usually backward-compatible enough for vanilla/Fabric, though not
/// always for Forge installers), and only falls back to "newest available"
/// if nothing meets the requirement.
pub fn find_runtime_for(runtimes: &[JavaRuntime], required_major: u32) -> Option<JavaRuntime> {
    runtimes
        .iter()
        .find(|r| r.major == required_major)
        .or_else(|| {
            runtimes
                .iter()
                .filter(|r| r.major > required_major)
                .min_by_key(|r| r.major)
        })
        .or_else(|| runtimes.iter().max_by_key(|r| r.major))
        .cloned()
}

pub fn find_all_runtimes() -> Result<Vec<JavaRuntime>, JreError> {
    let mut paths = HashSet::new();

    // PATH entries.
    if let Ok(path) = std::env::var("PATH") {
        let sep = if cfg!(target_os = "windows") { ';' } else { ':' };
        for dir in path.split(sep) {
            paths.insert(PathBuf::from(dir));
        }
    }

    // JAVA_HOME.
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        paths.insert(PathBuf::from(java_home).join("bin"));
    }

    // Common install directories.
    #[cfg(target_os = "windows")]
    {
        let common = [
            r"C:\Program Files\Java",
            r"C:\Program Files (x86)\Java",
            r"C:\Program Files\Eclipse Adoptium",
            r"C:\Program Files (x86)\Eclipse Adoptium",
            r"C:\Program Files\Microsoft",
            r"C:\Program Files (x86)\Microsoft",
        ];
        for base in &common {
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    paths.insert(path.join("bin"));
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        for base in ["/usr/lib/jvm", "/Library/Java/JavaVirtualMachines"] {
            if let Ok(entries) = std::fs::read_dir(base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let bin = path.join("Contents").join("Home").join("bin");
                    if bin.exists() {
                        paths.insert(bin);
                    } else {
                        paths.insert(path.join("bin"));
                    }
                }
            }
        }
    }

    // Windows registry.
    #[cfg(target_os = "windows")]
    {
        use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
        let keys = [
            r"SOFTWARE\JavaSoft\Java Runtime Environment",
            r"SOFTWARE\JavaSoft\Java Development Kit",
            r"SOFTWARE\JavaSoft\JRE",
            r"SOFTWARE\JavaSoft\JDK",
            r"SOFTWARE\Eclipse Foundation\JDK",
            r"SOFTWARE\Eclipse Adoptium\JRE",
            r"SOFTWARE\Microsoft\JDK",
        ];
        for key in &keys {
            for flags in [KEY_READ, KEY_READ | 0x0100 /* KEY_WOW64_64KEY */, KEY_READ | 0x0200 /* KEY_WOW64_32KEY */] {
                if let Ok(jre_key) = winreg::RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(key, flags) {
                    for subkey in jre_key.enum_keys().flatten() {
                        if let Ok(sk) = jre_key.open_subkey(subkey) {
                            for value_name in ["JavaHome", "InstallationPath"] {
                                if let Ok(path) = sk.get_value::<String, _>(value_name) {
                                    paths.insert(PathBuf::from(path).join("bin"));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut runtimes: Vec<JavaRuntime> = paths
        .into_iter()
        .filter_map(|p| check_java_at_path(&p).ok())
        .collect();
    runtimes.sort_by(|a, b| b.major.cmp(&a.major));
    runtimes.dedup_by(|a, b| a.path == b.path);
    Ok(runtimes)
}

pub fn check_java_at_path(path: &Path) -> Result<JavaRuntime, JreError> {
    let bin = path.to_path_buf();
    let java_bin = if bin.file_name().map(|f| f == java_binary_name()).unwrap_or(false) {
        bin
    } else {
        bin.join(java_binary_name())
    };

    if !java_bin.exists() {
        return Err(JreError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("java binary not found at {}", java_bin.display()),
        )));
    }

    let output = Command::new(&java_bin)
        .arg("-version")
        .stderr(Stdio::piped())
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let first_line = stderr.lines().next().unwrap_or("").to_string();
    let major = parse_java_major(&first_line).ok_or(JreError::InvalidVersion)?;

    Ok(JavaRuntime {
        path: java_bin.to_string_lossy().to_string(),
        version: first_line,
        major,
    })
}

fn java_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "java.exe"
    } else {
        "java"
    }
}

fn parse_java_major(version_string: &str) -> Option<u32> {
    let start = version_string.find('"').map(|i| i + 1)?;
    let end = version_string[start..].find('"').map(|i| start + i)?;
    let version = &version_string[start..end];
    let mut parts = version.split('.');
    let first = parts.next()?.parse::<u32>().ok()?;
    if first == 1 {
        parts.next()?.parse::<u32>().ok()
    } else {
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_java_major_matches_known_mojang_boundaries() {
        // Forge 1.20.1 famously needs Java 17, not whatever is newest on
        // the system — this is the exact scenario that used to crash with
        // `InaccessibleObjectException` deep inside Forge's bootstrap
        // launcher when TuffBox picked Java 21 instead.
        assert_eq!(required_java_major("1.20.1"), 17);
        assert_eq!(required_java_major("1.16.5"), 8);
        assert_eq!(required_java_major("1.12.2"), 8);
        assert_eq!(required_java_major("1.17"), 16);
        assert_eq!(required_java_major("1.18.2"), 17);
        assert_eq!(required_java_major("1.20.4"), 17);
        assert_eq!(required_java_major("1.20.5"), 21);
        assert_eq!(required_java_major("1.21.1"), 21);
    }

    fn runtime(major: u32) -> JavaRuntime {
        JavaRuntime {
            path: format!("/fake/java{major}"),
            version: format!("{major}.0.0"),
            major,
        }
    }

    #[test]
    fn find_runtime_for_prefers_exact_match() {
        let runtimes = vec![runtime(8), runtime(17), runtime(21)];
        let picked = find_runtime_for(&runtimes, 17).unwrap();
        assert_eq!(picked.major, 17);
    }

    #[test]
    fn find_runtime_for_falls_back_to_closest_newer_when_no_exact_match() {
        // Regression test: previously the launcher always picked the
        // single newest installed JVM regardless of what was needed,
        // which breaks old Forge installers built against pre-module-system
        // Java. This verifies the *closest* compatible version is chosen
        // instead of always jumping to the newest.
        let runtimes = vec![runtime(8), runtime(11), runtime(21)];
        let picked = find_runtime_for(&runtimes, 17).unwrap();
        assert_eq!(picked.major, 21, "should pick the closest newer runtime, not skip past it");
    }

    #[test]
    fn find_runtime_for_falls_back_to_newest_when_nothing_meets_requirement() {
        let runtimes = vec![runtime(8), runtime(11)];
        let picked = find_runtime_for(&runtimes, 21).unwrap();
        assert_eq!(picked.major, 11);
    }

    #[test]
    fn find_runtime_for_empty_list_returns_none() {
        assert!(find_runtime_for(&[], 17).is_none());
    }
}
