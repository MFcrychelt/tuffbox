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
