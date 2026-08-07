use std::{
    collections::HashSet,
    fs,
    io,
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
    #[error("java not found")]
    NotFound,
    #[error("failed to download GraalVM: {0}")]
    Download(String),
    #[error("failed to install GraalVM: {0}")]
    Install(String),
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
/// This exists because [`find_all_runtimes`] callers previously picked
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

/// Directory where TuffBox stores a managed GraalVM install.
pub fn managed_java_root() -> PathBuf {
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local).join("TuffBox").join("runtime").join("java");
    }
    if let Some(data) = dirs::data_local_dir() {
        return data.join("TuffBox").join("runtime").join("java");
    }
    std::env::temp_dir().join("tuffbox").join("runtime").join("java")
}

pub fn find_all_runtimes() -> Result<Vec<JavaRuntime>, JreError> {
    let mut paths = HashSet::new();

    // PATH entries.
    if let Ok(path) = std::env::var("PATH") {
        let sep = if cfg!(target_os = "windows") {
            ';'
        } else {
            ':'
        };
        for dir in path.split(sep) {
            paths.insert(PathBuf::from(dir));
        }
    }

    // JAVA_HOME.
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        paths.insert(PathBuf::from(java_home).join("bin"));
    }

    // TuffBox-managed GraalVM (downloaded on demand).
    let managed = managed_java_root();
    if managed.is_dir() {
        if let Ok(entries) = fs::read_dir(&managed) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name == "downloads" {
                    continue;
                }
                paths.insert(path.join("bin"));
                // macOS GraalVM layout: Contents/Home/bin
                paths.insert(path.join("Contents").join("Home").join("bin"));
            }
        }
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
            r"C:\Program Files\GraalVM",
            r"C:\Program Files\Common Files\Oracle\Java",
        ];
        for base in &common {
            if let Ok(entries) = fs::read_dir(base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    paths.insert(path.join("bin"));
                }
            }
        }
        // Portable installs like C:\graalvm-jdk-25.0.3+9.1\
        if let Ok(entries) = fs::read_dir(r"C:\") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.starts_with("graalvm") || name.starts_with("jdk") || name.starts_with("zulu")
                {
                    paths.insert(entry.path().join("bin"));
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        for base in ["/usr/lib/jvm", "/Library/Java/JavaVirtualMachines"] {
            if let Ok(entries) = fs::read_dir(base) {
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
            for flags in [
                KEY_READ,
                KEY_READ | 0x0100, /* KEY_WOW64_64KEY */
                KEY_READ | 0x0200, /* KEY_WOW64_32KEY */
            ] {
                if let Ok(jre_key) =
                    winreg::RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(key, flags)
                {
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

/// Returns any installed Java, or downloads the latest GraalVM Community JDK
/// into the TuffBox managed runtime folder when none is found.
pub fn ensure_java() -> Result<JavaRuntime, JreError> {
    ensure_java_with_log(|_| {})
}

pub fn ensure_java_with_log<F>(mut log: F) -> Result<JavaRuntime, JreError>
where
    F: FnMut(&str),
{
    let runtimes = find_all_runtimes()?;
    if let Some(rt) = runtimes.into_iter().next() {
        return Ok(rt);
    }
    log("# No Java found on this PC — downloading latest GraalVM Community JDK…");
    install_latest_graalvm(&mut log)
}

/// Like [`ensure_java`], then picks the best match for `mc_version`.
pub fn ensure_java_for_minecraft(mc_version: &str) -> Result<JavaRuntime, JreError> {
    ensure_java_for_minecraft_with_log(mc_version, |_| {})
}

pub fn ensure_java_for_minecraft_with_log<F>(
    mc_version: &str,
    mut log: F,
) -> Result<JavaRuntime, JreError>
where
    F: FnMut(&str),
{
    let mut runtimes = find_all_runtimes()?;
    if runtimes.is_empty() {
        log("# No Java found on this PC — downloading latest GraalVM Community JDK…");
        let installed = install_latest_graalvm(&mut log)?;
        runtimes.push(installed);
    }
    let required = required_java_major(mc_version);
    find_runtime_for(&runtimes, required).ok_or(JreError::NotFound)
}

pub fn check_java_at_path(path: &Path) -> Result<JavaRuntime, JreError> {
    let bin = path.to_path_buf();
    let java_bin = if bin
        .file_name()
        .map(|f| f == java_binary_name())
        .unwrap_or(false)
    {
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

    let output = {
        let mut c = Command::new(&java_bin);
        c.arg("-version").stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        c.output()?
    };
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

#[derive(Debug, serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, serde::Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

fn platform_asset_suffix() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return "windows-x64_bin.zip";
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return "linux-x64_bin.tar.gz";
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return "linux-aarch64_bin.tar.gz";
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return "macos-aarch64_bin.tar.gz";
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return "macos-x64_bin.tar.gz";
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
    )))]
    {
        "unsupported"
    }
}

fn install_latest_graalvm<F>(log: &mut F) -> Result<JavaRuntime, JreError>
where
    F: FnMut(&str),
{
    let suffix = platform_asset_suffix();
    if suffix == "unsupported" {
        return Err(JreError::Install(
            "this platform is not supported for automatic GraalVM download".into(),
        ));
    }

    let root = managed_java_root();
    fs::create_dir_all(&root).map_err(JreError::Io)?;
    let downloads = root.join("downloads");
    fs::create_dir_all(&downloads).map_err(JreError::Io)?;

    log("# Fetching latest GraalVM Community release metadata…");
    let release: GhRelease = crate::http::get_json(
        "https://api.github.com/repos/graalvm/graalvm-ce-builds/releases/latest",
    )
    .map_err(|e| JreError::Download(format!("GitHub releases API: {e}")))?;

    let asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(suffix) && !a.name.ends_with(".sha256"))
        .ok_or_else(|| {
            JreError::Download(format!(
                "no GraalVM asset matching '*{suffix}' in release {}",
                release.tag_name
            ))
        })?;

    let install_dir = root.join(&release.tag_name);
    let marker = install_dir.join(".tuffbox-java-ok");
    if marker.is_file() {
        if let Ok(rt) = find_java_under(&install_dir) {
            log(&format!(
                "# Reusing managed GraalVM {} at {}",
                release.tag_name,
                rt.path
            ));
            return Ok(rt);
        }
    }

    let archive_path = downloads.join(&asset.name);
    let sha256 = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(|s| s.trim().to_string());

    log(&format!(
        "# Downloading {} ({})…",
        asset.name, release.tag_name
    ));
    crate::download_engine::download_resumable(
        &asset.browser_download_url,
        &archive_path,
        sha256
            .as_deref()
            .map(|s| (s, crate::download_engine::ChecksumKind::Sha256)),
        None,
        Some(std::time::Duration::from_secs(300)),
    )
    .map_err(|e| JreError::Download(e.to_string()))?;

    // Fresh extract target.
    if install_dir.exists() {
        let _ = fs::remove_dir_all(&install_dir);
    }
    fs::create_dir_all(&install_dir).map_err(JreError::Io)?;

    log(&format!("# Extracting to {}…", install_dir.display()));
    extract_archive(&archive_path, &install_dir)?;

    // GraalVM zips usually contain a single top-level folder; flatten if needed.
    flatten_single_child_dir(&install_dir)?;

    let runtime = find_java_under(&install_dir).map_err(|e| {
        JreError::Install(format!(
            "extracted GraalVM but java binary not found under {}: {e}",
            install_dir.display()
        ))
    })?;

    fs::write(&marker, release.tag_name.as_bytes()).map_err(JreError::Io)?;
    log(&format!(
        "# GraalVM {} ready: {} (Java {})",
        release.tag_name, runtime.path, runtime.major
    ));
    Ok(runtime)
}

fn find_java_under(dir: &Path) -> Result<JavaRuntime, JreError> {
    let candidates = [
        dir.join("bin"),
        dir.join("Contents").join("Home").join("bin"),
    ];
    for bin in &candidates {
        if let Ok(rt) = check_java_at_path(bin) {
            return Ok(rt);
        }
    }
    // Shallow walk one extra level (zip root folder rename edge cases).
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if !p.is_dir() {
                continue;
            }
            for rel in ["bin", "Contents/Home/bin"] {
                if let Ok(rt) = check_java_at_path(&p.join(rel)) {
                    return Ok(rt);
                }
            }
        }
    }
    Err(JreError::NotFound)
}

fn flatten_single_child_dir(install_dir: &Path) -> Result<(), JreError> {
    let mut children: Vec<PathBuf> = fs::read_dir(install_dir)
        .map_err(JreError::Io)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| n != ".tuffbox-java-ok")
                .unwrap_or(true)
        })
        .collect();
    if children.len() != 1 || !children[0].is_dir() {
        return Ok(());
    }
    // If the only child already has bin/java, keep nested layout — find_java_under handles it.
    let only = children.remove(0);
    if only.join("bin").join(java_binary_name()).is_file()
        || only
            .join("Contents")
            .join("Home")
            .join("bin")
            .join(java_binary_name())
            .is_file()
    {
        return Ok(());
    }
    Ok(())
}

fn extract_archive(archive: &Path, dest: &Path) -> Result<(), JreError> {
    let name = archive.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.ends_with(".zip") {
        extract_zip(archive, dest)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        extract_tar_gz_via_system_tar(archive, dest)
    } else {
        Err(JreError::Install(format!(
            "unsupported archive format: {name}"
        )))
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), JreError> {
    let file = fs::File::open(archive).map_err(JreError::Io)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| JreError::Install(e.to_string()))?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| JreError::Install(e.to_string()))?;
        let name = entry.name().replace('\\', "/");
        if name.contains("..") || name.starts_with('/') {
            return Err(JreError::Install(format!("path traversal in zip: {name}")));
        }
        let out_path = dest.join(&name);
        if entry.is_dir() || name.ends_with('/') {
            fs::create_dir_all(&out_path).map_err(JreError::Io)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(JreError::Io)?;
        }
        let mut out = fs::File::create(&out_path).map_err(JreError::Io)?;
        io::copy(&mut entry, &mut out).map_err(JreError::Io)?;
    }
    Ok(())
}

fn extract_tar_gz_via_system_tar(archive: &Path, dest: &Path) -> Result<(), JreError> {
    // Prefer system tar on Unix (always present); avoids adding a tar crate dep.
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .status()
        .map_err(|e| JreError::Install(format!("failed to run tar: {e}")))?;
    if !status.success() {
        return Err(JreError::Install(format!(
            "tar exited with status {status}"
        )));
    }
    Ok(())
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
        assert_eq!(
            picked.major, 21,
            "should pick the closest newer runtime, not skip past it"
        );
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

    #[test]
    fn platform_suffix_is_known() {
        assert_ne!(platform_asset_suffix(), "unsupported");
    }
}
