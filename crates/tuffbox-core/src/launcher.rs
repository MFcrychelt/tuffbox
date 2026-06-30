use crate::jre::JavaRuntime;
use crate::manifest::{ProfileSpec, ProjectManifest};
use crate::mc_install::{install_game, InstallProgress};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
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

impl TestLauncher {
    pub fn find_java() -> Result<JavaRuntime, LauncherError> {
        crate::jre::find_all_runtimes()
            .map_err(|_| LauncherError::JavaNotFound)?
            .into_iter()
            .next()
            .ok_or(LauncherError::JavaNotFound)
    }

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
                if src.is_file() {
                    fs::copy(&src, mods_dir.join(file_name))?;
                }
            }
        }

        // Copy overrides/config if configured.
        if let Some(overrides) = &manifest.overrides {
            if let Some(config_override) = &overrides.config {
                let src = PathBuf::from(config_override);
                if src.is_dir() {
                    copy_dir_all(&src, &config_dir)?;
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
        _profile: &ProfileSpec,
        options: &LaunchOptions,
        java: &JavaRuntime,
        launcher_dir: &Path,
        progress: &InstallProgress,
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
        .map_err(|e| LauncherError::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;

        let auth_player_name = "Player";
        let auth_uuid = "00000000000000000000000000000000";
        let auth_access_token = "0";
        let user_type = "msa";
        let version_type = "release";
        let assets_dir = canonicalize(&game.asset_dir).unwrap_or_else(|_| game.asset_dir.clone());
        let game_dir = canonicalize(&options.instance_dir).unwrap_or_else(|_| options.instance_dir.clone());
        let natives_dir = canonicalize(&game.natives_dir).unwrap_or_else(|_| game.natives_dir.clone());
        let version_jar = canonicalize(&game.client_jar).unwrap_or_else(|_| game.client_jar.clone());
        let assets_dir_s = assets_dir.to_string_lossy();
        let game_dir_s = game_dir.to_string_lossy();
        let natives_dir_s = natives_dir.to_string_lossy();
        let version_jar_s = version_jar.to_string_lossy();

        let classpath = game
            .libraries
            .iter()
            .filter_map(|p| canonicalize(p).ok().map(|c| c.to_string_lossy().to_string()))
            .collect::<Vec<_>>()
            .join(";");

        progress.log(&format!("# Main class: {}", game.main_class));
        progress.log(&format!("# Classpath entries: {}", game.libraries.len()));

        let mut cmd = Command::new(PathBuf::from(&java.path));
        cmd.arg(format!("-Xmx{}M", options.memory_mb));
        cmd.args(&options.jvm_args);

        for arg in &game.jvm_args {
            let value = arg
                .replace("${natives_directory}", &natives_dir_s)
                .replace("${library_directory}", &game_dir_s)
                .replace("${launcher_name}", "tuffbox")
                .replace("${launcher_version}", "0.1.0")
                .replace("${version_name}", &game.id)
                .replace("${classpath}", &classpath);
            cmd.arg(value);
        }

        if let Some(log_config) = &game.log_config {
            if let Ok(log_path) = canonicalize(log_config) {
                let log_arg = format!("-Dlog4j.configurationFile={}", log_path.to_string_lossy());
                cmd.arg(log_arg);
            }
        }

        cmd.arg(format!("-Djava.library.path={}", natives_dir_s));
        cmd.arg("-cp").arg(classpath);
        cmd.arg(&game.main_class);

        for arg in &game.game_args {
            let value = arg
                .replace("${auth_player_name}", auth_player_name)
                .replace("${auth_uuid}", auth_uuid)
                .replace("${auth_access_token}", auth_access_token)
                .replace("${user_type}", user_type)
                .replace("${version_type}", version_type)
                .replace("${assets_root}", &assets_dir_s)
                .replace("${assets_index_name}", &game.asset_index_id)
                .replace("${game_directory}", &game_dir_s)
                .replace("${version_name}", &game.id)
                .replace("${natives_directory}", &natives_dir_s)
                .replace("${client_jar}", &version_jar_s);
            cmd.arg(value);
        }

        let log_path = options.instance_dir.join("logs").join("latest.log");
        fs::create_dir_all(log_path.parent().unwrap())?;

        cmd.current_dir(&options.instance_dir);

        Ok((cmd, log_path))
    }
}

fn canonicalize(path: &Path) -> Result<PathBuf, LauncherError> {
    let path = fs::canonicalize(path)?;
    Ok(PathBuf::from(path.to_string_lossy().trim_start_matches("\\\\?\\")))
}

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), LauncherError> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.as_ref().join(entry.file_name());
        if path.is_dir() {
            copy_dir_all(&path, &dest)?;
        } else {
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
}
