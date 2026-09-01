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
    /// | solar | fern | blaze | dusk | glacier | minecraft
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub potato_pc: bool,
    /// Set once the frontend has run its one-time weak-hardware check (see
    /// store.ts `detectWeakHardware`) and either left `potato_pc` alone or
    /// auto-enabled it. Prevents re-deciding on every launch / overriding a
    /// later manual choice in Settings.
    #[serde(default)]
    pub perf_auto_detected: bool,
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
    /// Where new instances / downloaded modpacks are created. Empty = ~/TuffBox/instances.
    #[serde(default)]
    pub instances_path: Option<String>,
    /// Preferred Java binary when project has no java.path.
    #[serde(default)]
    pub default_java_path: Option<String>,
    /// Extra JVM args appended globally (space-separated stored as string).
    #[serde(default)]
    pub java_custom_args: Option<String>,
    #[serde(default = "default_memory")]
    pub default_memory_mb: u32,
    /// In-app YouTube player (lite nocookie embed). `false` = preview thumbnails only → system browser.
    #[serde(default = "default_youtube_inline_player")]
    pub youtube_inline_player: bool,
    /// Show the YouTube feed strip on the home dashboard. Off by default; enable in Settings.
    #[serde(default)]
    pub show_youtube_on_home: bool,
    /// Hide IDE workflow rail (Content / Setup / …); reveal on bottom-edge hover.
    #[serde(default)]
    pub auto_hide_workflow_rail: bool,
    /// Left nav: `full` | `icons` (toggle labels) | `autoHide` (left-edge hover).
    #[serde(default = "default_sidebar_mode")]
    pub sidebar_mode: String,
    /// UI zoom percent (75–150). Applied as CSS `--ui-scale` on the app shell.
    #[serde(default = "default_ui_scale_percent")]
    pub ui_scale_percent: u32,
    /// `auto` follows screen/window size; `manual` locks `ui_scale_percent`.
    /// Empty string = unset (migrated on load: non-100% → manual, else auto).
    #[serde(default)]
    pub ui_scale_mode: String,
    /// Round corners on panels/cards/chrome everywhere (CSS `--border-radius-*`).
    #[serde(default = "default_rounded_corners")]
    pub rounded_corners: bool,
    /// Hide InstanceHome preview block on the home dashboard.
    #[serde(default)]
    pub hide_instance_home: bool,
    /// Quartz backdrop panel behind the home dashboard (home-only).
    #[serde(default = "default_home_backdrop")]
    pub home_backdrop: bool,
    /// Inject the in-game overlay bridge (YouTube player + friends/chat) on launch.
    #[serde(default = "default_ingame_overlay")]
    pub ingame_overlay: bool,
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
fn default_youtube_inline_player() -> bool {
    true
}
fn default_sidebar_mode() -> String {
    "full".into()
}
fn default_ui_scale_percent() -> u32 {
    100
}
fn default_rounded_corners() -> bool {
    true
}
fn default_home_backdrop() -> bool {
    true
}
fn default_ingame_overlay() -> bool {
    true
}

fn normalize_ui_scale_mode(settings: &mut LauncherSettings) {
    let mode = settings.ui_scale_mode.trim().to_ascii_lowercase();
    settings.ui_scale_mode = match mode.as_str() {
        "auto" | "manual" => mode,
        _ => {
            // Migration: respect an existing manual zoom choice.
            if settings.ui_scale_percent != 100 {
                "manual".into()
            } else {
                "auto".into()
            }
        }
    };
}

impl Default for LauncherSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            potato_pc: false,
            perf_auto_detected: false,
            concurrent_downloads: default_concurrent(),
            game_resolution: None,
            pre_launch_hook: None,
            post_exit_hook: None,
            wrapper_command: None,
            runtime_path: None,
            instances_path: None,
            default_java_path: None,
            java_custom_args: None,
            default_memory_mb: default_memory(),
            youtube_inline_player: default_youtube_inline_player(),
            show_youtube_on_home: false,
            auto_hide_workflow_rail: false,
            sidebar_mode: default_sidebar_mode(),
            ui_scale_percent: default_ui_scale_percent(),
            ui_scale_mode: "auto".into(),
            rounded_corners: default_rounded_corners(),
            hide_instance_home: false,
            home_backdrop: default_home_backdrop(),
            ingame_overlay: default_ingame_overlay(),
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
    let mut settings = if path.is_file() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    } else {
        LauncherSettings::default()
    };
    normalize_ui_scale_mode(&mut settings);
    apply_runtime_side_effects(&settings);
    settings
}

pub fn save_launcher_settings(settings: &LauncherSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut normalized = settings.clone();
    normalize_ui_scale_mode(&mut normalized);
    let raw = serde_json::to_string_pretty(&normalized).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| e.to_string())?;
    apply_runtime_side_effects(&normalized);
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

/// Whether the in-game overlay bridge should be injected on launch.
pub fn overlay_enabled() -> bool {
    load_launcher_settings().ingame_overlay
}

pub fn resolve_runtime_path() -> PathBuf {
    let settings = load_launcher_settings();
    if let Some(p) = settings.runtime_path.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        return PathBuf::from(p);
    }
    default_runtime_path()
}

/// Default folder for new instances / downloaded modpacks.
pub fn default_instances_path() -> PathBuf {
    dirs::home_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("instances")
}

pub fn resolve_instances_path() -> PathBuf {
    let settings = load_launcher_settings();
    if let Some(p) = settings
        .instances_path
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return PathBuf::from(p);
    }
    default_instances_path()
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

pub fn validate_instances_path(path: &str) -> Result<bool, String> {
    validate_runtime_path(path)
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

fn jvm_args_contain(args: &[String], needle: &str) -> bool {
    args.iter().any(|a| a.contains(needle))
}

/// Push `arg` unless an equivalent flag is already present (same `-XX:Name=`
/// or `-XX:±Name` prefix). Keeps user/profile overrides authoritative when
/// auto-tune appends its GC recommendations.
pub fn append_unique_jvm_arg(args: &mut Vec<String>, arg: String) {
    let key = arg
        .trim_start_matches(['-', 'X', 'P'])
        .split('=')
        .next()
        .unwrap_or(&arg)
        .trim_start_matches(['+', '-'])
        .to_string();
    if !key.is_empty() && jvm_args_contain(args, &key) {
        return;
    }
    args.push(arg);
}

/// Append launch-stability / low-end JVM flags without overriding user or
/// profile args that already set the same option.
pub fn append_stability_jvm_args(args: &mut Vec<String>, potato_pc: bool) {
    // Prefer G1 on modern JDKs; harmless if already the default.
    if !jvm_args_contain(args, "UseG1GC") {
        args.push("-XX:+UseG1GC".into());
    }
    if !jvm_args_contain(args, "MaxGCPauseMillis") {
        args.push("-XX:MaxGCPauseMillis=50".into());
    }
    if potato_pc {
        if !jvm_args_contain(args, "G1HeapRegionSize") {
            args.push("-XX:G1HeapRegionSize=16M".into());
        }
        if !jvm_args_contain(args, "ParallelGCThreads") {
            args.push("-XX:ParallelGCThreads=2".into());
        }
        if !jvm_args_contain(args, "ConcGCThreads") {
            args.push("-XX:ConcGCThreads=1".into());
        }
        if !jvm_args_contain(args, "ReservedCodeCacheSize") {
            args.push("-XX:ReservedCodeCacheSize=256m".into());
        }
        // Avoid long GC stalls that look like freezes on weak CPUs.
        if !jvm_args_contain(args, "DisableExplicitGC") {
            args.push("-XX:+DisableExplicitGC".into());
        }
    }
}

/// Resolve heap size: profile → launcher default, then clamp for potato PCs.
pub fn resolve_launch_memory_mb(
    profile_memory_mb: Option<u32>,
    settings: &LauncherSettings,
    override_mb: Option<u32>,
) -> u32 {
    if let Some(mb) = override_mb {
        return mb.max(512);
    }
    let base = profile_memory_mb
        .unwrap_or(settings.default_memory_mb)
        .max(512);
    if settings.potato_pc {
        base.min(3072)
    } else {
        base
    }
}

// ── Auto-tune (Millida tuning.rs-inspired): heap + GC profile from hardware
//    and mod count. Used when the user leaves memory on "Auto". ──────────

/// Recommended `-Xmx` for a machine with `total_ram_mb` and `mod_count`
/// loaded mods. Never exceeds 60% of physical RAM (leaves room for OS,
/// WebView, and off-heap JVM overhead) and clamps to [2048, 12288].
pub fn recommend_memory_mb(total_ram_mb: u64, mod_count: usize) -> u32 {
    if total_ram_mb == 0 {
        return 4096;
    }
    // Mod-heavy packs need more heap: 2 GB base + ~64 MB per mod, capped.
    let mod_demand: u64 = (mod_count as u64).saturating_mul(64);
    let wanted = 2048 + mod_demand;
    let ceiling = (total_ram_mb * 60 / 100).max(2048);
    let mb = wanted.min(ceiling).clamp(2048, 12288);
    // Round down to a clean 512 MB step.
    ((mb / 512) * 512) as u32
}

/// JVM GC flags for the auto-tuned profile. Returns flags the caller appends
/// after custom user args (user flags win — caller filters duplicates via
/// `jvm_args_contain` semantics, same as `append_stability_jvm_args`).
/// Heavier packs get a low-pause G1 tuning; small ones get the defaults.
pub fn recommend_gc_args(memory_mb: u32, mod_count: usize) -> Vec<String> {
    let mut args = vec![
        "-XX:+UseG1GC".into(),
        "-XX:MaxGCPauseMillis=40".into(),
        // 32–48% of heap as young gen target: smooths chunk/mod churn.
        format!(
            "-XX:G1NewSizePercent={}",
            if mod_count > 150 { 32 } else { 40 }
        ),
    ];
    if memory_mb >= 4096 {
        // Big heaps: region sizing avoids humongous allocations with
        // mod-heavy class loading.
        args.push("-XX:G1HeapRegionSize=16M".into());
    }
    args
}

/// Full auto recommendation: heap + GC args for a launch.
pub fn auto_tune_launch(total_ram_mb: u64, mod_count: usize) -> (u32, Vec<String>) {
    let memory = recommend_memory_mb(total_ram_mb, mod_count);
    let gc = recommend_gc_args(memory, mod_count);
    (memory, gc)
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_launcher_settings() -> LauncherSettings {
    load_launcher_settings()
}

/// Auto-tuned heap + GC flags for the current machine and mod count.
/// Feeds the "Auto" memory option in launcher settings.
#[tauri::command(rename_all = "camelCase")]
pub fn get_auto_tune(total_ram_mb: u64, mod_count: usize) -> serde_json::Value {
    let (memory_mb, gc_args) = auto_tune_launch(total_ram_mb, mod_count);
    serde_json::json!({ "memoryMb": memory_mb, "gcArgs": gc_args })
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
pub fn get_instances_path_info() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "current": resolve_instances_path().to_string_lossy(),
        "default": default_instances_path().to_string_lossy(),
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub fn validate_runtime_path_cmd(path: String) -> Result<bool, String> {
    validate_runtime_path(&path)
}

#[tauri::command(rename_all = "camelCase")]
pub fn validate_instances_path_cmd(path: String) -> Result<bool, String> {
    validate_instances_path(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn potato_memory_clamps_to_3gb() {
        let mut settings = LauncherSettings::default();
        settings.potato_pc = true;
        settings.default_memory_mb = 8192;
        assert_eq!(resolve_launch_memory_mb(Some(8192), &settings, None), 3072);
        assert_eq!(resolve_launch_memory_mb(None, &settings, None), 3072);
        assert_eq!(resolve_launch_memory_mb(None, &settings, Some(4096)), 4096);
    }

    #[test]
    fn stability_args_skip_existing() {
        let mut args = vec!["-XX:MaxGCPauseMillis=200".into()];
        append_stability_jvm_args(&mut args, true);
        assert_eq!(
            args.iter()
                .filter(|a| a.contains("MaxGCPauseMillis"))
                .count(),
            1
        );
        assert!(args.iter().any(|a| a.contains("UseG1GC")));
        assert!(args.iter().any(|a| a.contains("ParallelGCThreads")));
    }

    #[test]
    fn recommend_memory_scales_with_mods_and_respects_ceiling() {
        // 8 GB machine, no mods: 60% ceiling = 4.8 GB → wanted 2048 → 2048.
        assert_eq!(recommend_memory_mb(8192, 0), 2048);
        // 8 GB machine, 100 mods: wanted 8448 → ceiling 4915 → 4608 (512 step).
        assert_eq!(recommend_memory_mb(8192, 100), 4608);
        // 32 GB machine, 300 mods: wanted 21248 → clamp 12288.
        assert_eq!(recommend_memory_mb(32768, 300), 12288);
        // Tiny 2 GB machine: ceiling max(2048) → 2048.
        assert_eq!(recommend_memory_mb(2048, 50), 2048);
        // Unknown RAM: safe default.
        assert_eq!(recommend_memory_mb(0, 0), 4096);
    }

    #[test]
    fn recommend_gc_args_match_heap_and_mods() {
        let small = recommend_gc_args(2048, 10);
        assert!(small.iter().any(|a| a.contains("UseG1GC")));
        assert!(small.iter().any(|a| a.contains("G1NewSizePercent=40")));
        assert!(!small.iter().any(|a| a.contains("G1HeapRegionSize")));

        let big = recommend_gc_args(8192, 200);
        assert!(big.iter().any(|a| a.contains("G1NewSizePercent=32")));
        assert!(big.iter().any(|a| a.contains("G1HeapRegionSize=16M")));
    }

    #[test]
    fn auto_tune_pairs_memory_with_gc() {
        let (memory, gc) = auto_tune_launch(16384, 60);
        assert_eq!(memory, recommend_memory_mb(16384, 60));
        assert_eq!(gc, recommend_gc_args(memory, 60));
    }
}
