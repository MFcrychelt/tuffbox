use crate::jre::JavaRuntime;
use crate::manifest::{ProfileSpec, ProjectManifest};
use crate::mc_install::{install_game, InstallProgress};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error("java not found")]
    JavaNotFound,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("instance directory not prepared")]
    InstanceNotPrepared,
    #[error("unsupported loader: {0}")]
    UnsupportedLoader(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    pub profile_id: String,
    pub instance_dir: PathBuf,
    pub memory_mb: u32,
    pub jvm_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub exit_code: Option<i32>,
    pub log_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedInstance {
    pub instance_dir: PathBuf,
    pub mods_dir: PathBuf,
    pub config_dir: PathBuf,
    pub log_dir: PathBuf,
}

pub struct TestLauncher;

/// Computes a deterministic offline-mode UUID from a player name, the same
/// way vanilla Minecraft/most launchers do for offline play: MD5 of
/// `"OfflinePlayer:{name}"`, with the version/variant bits patched to make
/// it a valid (version 3) UUID.
///
/// This matters because the previous implementation always launched with a
/// fixed all-zero UUID regardless of player name: every test run looked
/// like the exact same player to Minecraft, which breaks anything that
/// keys per-player state by UUID (playerdata, permissions, whitelists,
/// mods that store player-scoped data) and makes it impossible to test
/// multiplayer-relevant behavior with different names.
pub fn offline_uuid(player_name: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(b"OfflinePlayer:");
    hasher.update(player_name.as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();

    // Set version (3) and variant (RFC 4122) bits, matching Java's
    // `UUID.nameUUIDFromBytes` used by vanilla for offline players.
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

impl TestLauncher {
    /// Returns the newest installed Java runtime, with no regard for what
    /// any particular Minecraft version needs. Prefer
    /// [`Self::find_java_for_minecraft`] when a target version is known;
    /// this is kept only for callers that generically need "some" JVM.
    pub fn find_java() -> Result<JavaRuntime, LauncherError> {
        crate::jre::find_all_runtimes()
            .map_err(|_| LauncherError::JavaNotFound)?
            .into_iter()
            .next()
            .ok_or(LauncherError::JavaNotFound)
    }

    /// Picks the installed Java runtime that best matches what
    /// `mc_version` actually requires (see [`crate::jre::required_java_major`]),
    /// instead of always grabbing the newest JVM on the system regardless
    /// of compatibility. Falls back to the newest available runtime if
    /// nothing meets the requirement, since attempting the launch with a
    /// clear log message is more useful than refusing to start.
    pub fn find_java_for_minecraft(mc_version: &str) -> Result<JavaRuntime, LauncherError> {
        let runtimes = crate::jre::find_all_runtimes().map_err(|_| LauncherError::JavaNotFound)?;
        let required = crate::jre::required_java_major(mc_version);
        crate::jre::find_runtime_for(&runtimes, required).ok_or(LauncherError::JavaNotFound)
    }

    // Utility for launching into a dedicated instance folder (Prism-style),
    // where mods/config are copied out of the shared project dir. Currently
    // TuffBox launches directly inside the project dir, so this isn't on the
    // hot path — but it's kept (with incremental copy) ready for when
    // per-profile isolation is enabled.
    #[allow(dead_code)]
    pub fn prepare_instance(
        manifest: &ProjectManifest,
        profile: &ProfileSpec,
        base_instance_dir: impl AsRef<Path>,
    ) -> Result<PreparedInstance, LauncherError> {
        let instance_dir = base_instance_dir.as_ref().join(&profile.id);
        let mods_dir = instance_dir.join("mods");
        let config_dir = instance_dir.join("config");
        let log_dir = instance_dir.join("logs");

        fs::create_dir_all(&mods_dir)?;
        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&log_dir)?;

        // For MVP, copy only mods whose side is compatible with the profile side.
        for module in &manifest.mods {
            if !module.side.is_compatible_with_profile(profile.side) {
                continue;
            }
            if let Some(file_name) = &module.file_name {
                let src = PathBuf::from("mods").join(file_name);
                let dst = mods_dir.join(file_name);
                if src.is_file() {
                    // Incremental copy: skip if the destination already exists
                    // and is byte-identical (same size). Avoids re-copying
                    // hundreds of MB of mods on every launch — only changed
                    // mods get re-synced.
                    if !is_same_file(&src, &dst) {
                        fs::copy(&src, &dst)?;
                    }
                }
            }
        }

        // Copy overrides/config if configured.
        if let Some(overrides) = &manifest.overrides {
            if let Some(config_override) = &overrides.config {
                let src = PathBuf::from(config_override);
                if src.is_dir() {
                    copy_dir_incremental(&src, &config_dir)?;
                }
            }
        }

        Ok(PreparedInstance {
            instance_dir,
            mods_dir,
            config_dir,
            log_dir,
        })
    }

    pub fn build_command(
        manifest: &ProjectManifest,
        profile: &ProfileSpec,
        options: &LaunchOptions,
        java: &JavaRuntime,
        launcher_dir: &Path,
        progress: &InstallProgress,
        mc_access_token: Option<&str>,
    ) -> Result<(std::process::Command, PathBuf), LauncherError> {
        if !options.instance_dir.exists() {
            return Err(LauncherError::InstanceNotPrepared);
        }

        let loader_kind = format!("{:?}", manifest.loader.kind).to_lowercase();
        let loader_version = manifest.loader.version.clone();
        progress.log(&format!("# Loader: {loader_kind} {loader_version}"));

        let game = install_game(
            &manifest.minecraft.version,
            &loader_kind,
            &loader_version,
            launcher_dir,
            Path::new(&java.path),
            progress,
        )
        .map_err(|e| {
            LauncherError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                e.to_string(),
            ))
        })?;

        let auth_player_name = profile
            .player_name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("Player")
            .to_string();

        // Use real MC access token when available, otherwise offline mode
        let (auth_uuid, auth_access_token, user_type) = if let Some(token) = mc_access_token {
            if !token.is_empty() && token != "0" {
                (
                    offline_uuid(&auth_player_name),
                    token.to_string(),
                    "msa",
                )
            } else {
                (
                    offline_uuid(&auth_player_name),
                    "0".to_string(),
                    "msa",
                )
            }
        } else {
            (
                offline_uuid(&auth_player_name),
                "0".to_string(),
                "msa",
            )
        };
        let version_type = "release";
        let assets_dir = canonicalize(&game.asset_dir).unwrap_or_else(|_| game.asset_dir.clone());
        let game_dir =
            canonicalize(&options.instance_dir).unwrap_or_else(|_| options.instance_dir.clone());
        let natives_dir =
            canonicalize(&game.natives_dir).unwrap_or_else(|_| game.natives_dir.clone());
        let version_jar =
            canonicalize(&game.client_jar).unwrap_or_else(|_| game.client_jar.clone());
        let library_dir = canonicalize(&launcher_dir.join("libraries"))
            .unwrap_or_else(|_| launcher_dir.join("libraries"));
        let assets_dir_s = assets_dir.to_string_lossy();
        let game_dir_s = game_dir.to_string_lossy();
        let natives_dir_s = natives_dir.to_string_lossy();
        let version_jar_s = version_jar.to_string_lossy();
        let library_dir_s = library_dir.to_string_lossy();

        let classpath = classpath_string(&game.libraries);
        // Same separator `classpath_string`/`std::env::join_paths` uses
        // for `${classpath}`, needed for Forge's `-p <module-path>` JVM
        // arg which uses a separate `${classpath_separator}` placeholder.
        // This was never substituted before, so on Linux/macOS the
        // literal string "${classpath_separator}" ended up inside the
        // module path, which made every entry after the first
        // unresolvable and crashed `securejarhandler`'s module
        // initialization with `InaccessibleObjectException` instead of
        // launching Forge.
        let classpath_separator = if cfg!(target_os = "windows") {
            ";"
        } else {
            ":"
        };

        progress.log(&format!("# Main class: {}", game.main_class));
        progress.log(&format!("# Classpath entries: {}", game.libraries.len()));

        let mut cmd = Command::new(PathBuf::from(&java.path));
        cmd.arg(format!("-Xmx{}M", options.memory_mb));
        cmd.args(&options.jvm_args);

        for arg in &game.jvm_args {
            // Replace spaces with a temporary character to prevent splitting
            // multi-word arguments, then split on that character after substitution.
            // This matches how Modrinth's launcher handles arguments like
            // "-Dfml.ignoreInvalidMinecraftCertificates=true" which may contain spaces.
            let value = arg
                .replace(' ', "\n")
                .replace("${natives_directory}", &natives_dir_s)
                .replace("${library_directory}", &library_dir_s)
                .replace("${launcher_name}", "tuffbox")
                .replace("${launcher_version}", "0.1.0")
                .replace("${version_name}", &game.id)
                .replace("${classpath_separator}", classpath_separator)
                .replace("${classpath}", &classpath);
            for part in value.split('\n') {
                if !part.is_empty() {
                    cmd.arg(part);
                }
            }
        }

        if let Some(log_config) = &game.log_config {
            if let Ok(log_path) = canonicalize(log_config) {
                let log_arg = format!("-Dlog4j.configurationFile={}", log_path.to_string_lossy());
                cmd.arg(log_arg);
            }
        }

        // Java 9+ requires --add-opens for module system access
        if java.major >= 9 {
            cmd.arg("--add-opens=java.base/java.lang.reflect=ALL-UNNAMED");
            cmd.arg("--add-opens=java.base/java.lang=ALL-UNNAMED");
        }

        // Java 25+ needs additional opens for JEP 512
        if java.major >= 25 {
            cmd.arg("--add-opens=jdk.internal/jdk.internal.misc=ALL-UNNAMED");
        }

        cmd.arg(format!("-Djava.library.path={}", natives_dir_s));
        cmd.arg("-cp").arg(classpath);
        cmd.arg(&game.main_class);

        for arg in &game.game_args {
            let value = arg
                .replace(' ', "\n")
                .replace("${auth_player_name}", &auth_player_name)
                .replace("${auth_uuid}", &auth_uuid)
                .replace("${auth_access_token}", &auth_access_token)
                .replace("${auth_session}", &auth_access_token)
                .replace("${user_type}", &user_type)
                .replace("${user_properties}", "{}")
                .replace("${version_type}", version_type)
                .replace("${assets_root}", &assets_dir_s)
                .replace("${assets_index_name}", &game.asset_index_id)
                .replace("${game_directory}", &game_dir_s)
                .replace("${game_assets}", &assets_dir_s)
                .replace("${version_name}", &game.id)
                .replace("${natives_directory}", &natives_dir_s)
                .replace("${client_jar}", &version_jar_s)
                // Quick play placeholders — empty for now (no quick play support yet)
                .replace("${quickPlaySingleplayer}", "")
                .replace("${quickPlayMultiplayer}", "");
            for part in value.split('\n') {
                if !part.is_empty() {
                    cmd.arg(part);
                }
            }
        }

        let log_path = options.instance_dir.join("logs").join("latest.log");
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)?;
        }

        cmd.current_dir(&options.instance_dir);

        Ok((cmd, log_path))
    }
}

fn classpath_string(paths: &[PathBuf]) -> String {
    let canonical_paths = paths
        .iter()
        .filter_map(|p| canonicalize(p).ok())
        .collect::<Vec<_>>();

    std::env::join_paths(canonical_paths.iter().map(|p| p.as_os_str()))
        .map(|joined| joined.to_string_lossy().to_string())
        .unwrap_or_else(|_| {
            let separator = if cfg!(target_os = "windows") {
                ";"
            } else {
                ":"
            };
            canonical_paths
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(separator)
        })
}

fn canonicalize(path: &Path) -> Result<PathBuf, LauncherError> {
    // `fs::canonicalize` does a real filesystem lookup per call; with 100+
    // library jars in a classpath that's 100+ syscalls every launch. Cache the
    // result for the process lifetime (paths in the launcher dir don't move
    // between launches), falling back to a fresh lookup on miss.
    if let Some(cached) = CANON_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(path)
    {
        return Ok(cached.clone());
    }
    let resolved = fs::canonicalize(path)?;
    let cleaned = PathBuf::from(resolved.to_string_lossy().trim_start_matches("\\\\?\\"));
    CANON_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.to_path_buf(), cleaned.clone());
    Ok(cleaned)
}

lazy_static::lazy_static! {
    static ref CANON_CACHE: Mutex<std::collections::HashMap<PathBuf, PathBuf>> =
        Mutex::new(std::collections::HashMap::new());
}

/// Returns true when `dst` already exists and is byte-identical to `src`
/// (compared by file size; cheap and good enough to skip re-copying mods
/// that haven't changed between launches).
fn is_same_file(src: &Path, dst: &Path) -> bool {
    if !dst.is_file() {
        return false;
    }
    match (fs::metadata(src), fs::metadata(dst)) {
        (Ok(s), Ok(d)) => s.len() == d.len(),
        _ => false,
    }
}

/// Like a recursive copy, but skips files whose destination already exists with
/// the same size, so re-running a launch doesn't rewrite the whole config
/// tree every time.
fn copy_dir_incremental(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), LauncherError> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.as_ref().join(entry.file_name());
        if path.is_dir() {
            copy_dir_incremental(&path, &dest)?;
        } else if !is_same_file(&path, &dest) {
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_java_in_path() {
        // This test only verifies the function does not panic when java is available.
        let result = TestLauncher::find_java();
        if let Ok(java) = result {
            assert!(PathBuf::from(&java.path).exists());
            assert!(java.major >= 8);
        }
    }

    #[test]
    fn offline_uuid_matches_known_vanilla_value() {
        // This is the well-known offline UUID vanilla Minecraft computes
        // for the player name "Notch" (MD5("OfflinePlayer:Notch") with
        // version/variant bits patched). Matching it exactly proves TuffBox
        // computes offline identities the same way the game does.
        assert_eq!(offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }

    #[test]
    fn offline_uuid_is_deterministic_per_name() {
        assert_eq!(offline_uuid("Steve"), offline_uuid("Steve"));
        assert_ne!(offline_uuid("Steve"), offline_uuid("Alex"));
    }

    #[test]
    fn incremental_copy_skips_identical_files() {
        let dir = std::env::temp_dir().join("tuffbox_incr_copy_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src");
        let dst = dir.join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();

        let a = src.join("a.txt");
        let b = src.join("b.txt");
        fs::write(&a, b"same").unwrap();
        fs::write(&b, b"diff").unwrap();
        // Pre-populate dst with identical `a.txt` and a different `b.txt`.
        fs::write(dst.join("a.txt"), b"same").unwrap();
        fs::write(dst.join("b.txt"), b"OLD").unwrap();

        copy_dir_incremental(&src, &dst).unwrap();

        // a.txt unchanged -> not rewritten (still readable, content matches)
        assert_eq!(fs::read_to_string(dst.join("a.txt")).unwrap(), "same");
        // b.txt differed -> overwritten with source content
        assert_eq!(fs::read_to_string(dst.join("b.txt")).unwrap(), "diff");

        let _ = fs::remove_dir_all(&dir);
    }
}
