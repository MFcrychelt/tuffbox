//! Global launcher settings (GDLauncher Carbon–inspired surface; original storage).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameResolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    /// Theme id: tuffbox | tuffbox-light | carbon | inferno | aether | frost | pixelato | win95
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub potato_pc: bool,
    #[serde(default = "default_concurrent")]
    pub concurrent_downloads: u32,
    #[serde(default)]
    pub game_resolution: Option<GameResolution>,
    #[serde(default)]
    pub pre_launch_hook: Option<String>,
    #[serde(default)]
    pub post_exit_hook: Option<String>,
    #[serde(default)]
    pub wrapper_command: Option<String>,
    /// Override for shared game data (versions/libraries/assets). Empty = default.
    #[serde(default)]
    pub runtime_path: Option<String>,
    /// Preferred Java binary when project has no java.path.
    #[serde(default)]
    pub default_java_path: Option<String>,
    /// Extra JVM args appended globally (space-separated stored as string).
    #[serde(default)]
    pub java_custom_args: Option<String>,
    #[serde(default = "default_memory")]
    pub default_memory_mb: u32,
}

fn default_theme() -> String {
    "tuffbox".into()
}
fn default_concurrent() -> u32 {
    8
}
fn default_memory() -> u32 {
    4096
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            potato_pc: false,
            concurrent_downloads: default_concurrent(),
            game_resolution: None,
            pre_launch_hook: None,
            post_exit_hook: None,
            wrapper_command: None,
            runtime_path: None,
            default_java_path: None,
            java_custom_args: None,
            default_memory_mb: default_memory(),
        }
    }
}

fn settings_path() -> PathBuf {
    dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("launcher_settings.json")
}

pub fn load_launcher_settings() -> LauncherSettings {
    let path = settings_path();
    let settings = if path.is_file() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    } else {
        LauncherSettings::default()
    };
    apply_runtime_side_effects(&settings);
    settings
}

pub fn save_launcher_settings(settings: &LauncherSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    apply_runtime_side_effects(settings);
    Ok(())
}

fn apply_runtime_side_effects(settings: &LauncherSettings) {
    let n = settings.concurrent_downloads.clamp(1, 64) as usize;
    tuffbox_core::download_engine::set_configured_concurrency(n);
}

/// Default shared launcher data directory (versions / libraries / assets).
pub fn default_runtime_path() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
}

pub fn resolve_runtime_path() -> PathBuf {
    let settings = load_launcher_settings();
    if let Some(p) = settings.runtime_path.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        return PathBuf::from(p);
    }
    default_runtime_path()
}

pub fn validate_runtime_path(path: &str) -> Result<bool, String> {
    let p = Path::new(path);
    if path.trim().is_empty() {
        return Ok(false);
    }
    if p.exists() && !p.is_dir() {
        return Err("path exists but is not a directory".into());
    }
    Ok(true)
}

/// Run a hook command via the platform shell. Empty/whitespace is a no-op.
pub fn run_hook(cmd: Option<&str>, label: &str) -> Result<(), String> {
    let Some(raw) = cmd.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    #[cfg(windows)]
    let status = std::process::Command::new("cmd")
        .args(["/C", raw])
        .status()
        .map_err(|e| format!("{label} failed to start: {e}"))?;
    #[cfg(not(windows))]
    let status = std::process::Command::new("sh")
        .args(["-c", raw])
        .status()
        .map_err(|e| format!("{label} failed to start: {e}"))?;
    if !status.success() {
        return Err(format!("{label} exited with {status}"));
    }
    Ok(())
}

/// Wrap a Minecraft java `Command` with an optional wrapper binary
/// (e.g. `gamemoderun`, `prime-run`).
pub fn wrap_java_command(
    java_cmd: std::process::Command,
    wrapper: Option<&str>,
) -> std::process::Command {
    let Some(raw) = wrapper.map(str::trim).filter(|s| !s.is_empty()) else {
        return java_cmd;
    };
    let parts: Vec<&str> = raw.split_whitespace().collect();
    if parts.is_empty() {
        return java_cmd;
    }
    let mut wrapped = std::process::Command::new(parts[0]);
    for p in &parts[1..] {
        wrapped.arg(p);
    }
    wrapped.arg(java_cmd.get_program());
    for arg in java_cmd.get_args() {
        wrapped.arg(arg);
    }
    if let Some(dir) = java_cmd.get_current_dir() {
        wrapped.current_dir(dir);
    }
    for (key, val) in java_cmd.get_envs() {
        match val {
            Some(v) => {
                wrapped.env(key, v);
            }
            None => {
                wrapped.env_remove(key);
            }
        }
    }
    wrapped
}

pub fn split_custom_jvm_args(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or("")
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_launcher_settings() -> LauncherSettings {
    load_launcher_settings()
}

#[tauri::command(rename_all = "camelCase")]
pub fn save_launcher_settings_cmd(settings: LauncherSettings) -> Result<LauncherSettings, String> {
    save_launcher_settings(&settings)?;
    Ok(load_launcher_settings())
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_runtime_path_info() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "current": resolve_runtime_path().to_string_lossy(),
        "default": default_runtime_path().to_string_lossy(),
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub fn validate_runtime_path_cmd(path: String) -> Result<bool, String> {
    validate_runtime_path(&path)
}
