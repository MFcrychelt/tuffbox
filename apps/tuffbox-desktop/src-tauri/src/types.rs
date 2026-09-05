//! Shared type definitions for Tauri command serialization.
//!
//! All structs used across multiple command modules live here.

use std::collections::{HashMap, HashSet};

// ── Project types ────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub minecraft_version: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub java_path: Option<String>,
    pub memory_mb: u32,
    pub jvm_args: Vec<String>,
    pub player_name: String,
    /// Canonical manifest file path (may differ from the path passed in).
    pub manifest_path: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaStatus {
    pub current: String,
    pub detected: String,
    pub needs_migration: bool,
    pub supported: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub side: String,
    pub memory_mb: Option<u32>,
    pub jvm_args: Vec<String>,
}

// ── Config types ─────────────────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFileSummary {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub size: u64,
    pub modified: Option<u64>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteConfigResult {
    pub snapshot_id: String,
}

// ── Mod types ────────────────────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInstallDependent {
    pub id: String,
    pub slug: String,
    pub name: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInstallPreview {
    pub project_id: String,
    pub slug: String,
    pub name: String,
    pub version: String,
    pub file_name: Option<String>,
    pub side: String,
    pub dependencies: Vec<tuffbox_core::ModDependencySpec>,
    /// Dependency targets already present in the project (slug or provider project id).
    #[serde(default)]
    pub installed_dependencies: Vec<String>,
    /// Top-N Modrinth projects that require this one (search facet; may be empty).
    #[serde(default)]
    pub dependents: Vec<ModInstallDependent>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModUserState {
    #[serde(default)]
    pub favorites: HashMap<String, bool>,
    #[serde(default)]
    pub lists: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub ratings: HashMap<String, u8>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PagedCatalog {
    pub results: Vec<serde_json::Value>,
    pub total: u32,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDownloadProgressPayload {
    pub id: String,
    pub name: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: u32,
    pub status: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdateProgressPayload {
    pub phase: String,
    pub message: String,
    pub current: usize,
    pub total: usize,
    pub percent: u32,
    pub mod_id: Option<String>,
}

// ── History & Snapshot types ─────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectChangeEntry {
    pub id: String,
    pub snapshot_id: String,
    pub operation: String,
    pub reason: String,
    pub created_at: String,
    pub path: String,
    pub category: String,
    pub kind: String,
    pub preview: String,
    pub diff: String,
    pub can_open: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub crash_fingerprint_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub plan_source: Option<String>,
    #[serde(default)]
    pub actor: String,
    #[serde(default)]
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub episode_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fix_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub log_path: Option<String>,
}

/// Crash → actions → outcome grouping for Smart History.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEpisode {
    pub id: String,
    /// open | fixed | broke | rolled_back
    pub outcome: String,
    /// ai | heuristic | kb | swarm | manual | unknown
    pub fix_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint_key: Option<String>,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    pub summary: String,
    pub action_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryListResult {
    pub entries: Vec<ProjectChangeEntry>,
    pub episodes: Vec<HistoryEpisode>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFileContent {
    pub path: String,
    pub content: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySettings {
    pub tracked: HashMap<String, bool>,
    /// When true, IdeWorkspace debounces scan_project_changes while IDE is focused.
    #[serde(default)]
    pub focused_scan: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFileDiff {
    pub path: String,
    pub from_exists: bool,
    pub to_exists: bool,
    pub text: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotChangedFile {
    pub path: String,
    pub category: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDetail {
    pub snapshot: tuffbox_core::Snapshot,
    /// Resolved human-readable action lines (meta or synthesized).
    pub actions_summary: Vec<String>,
    pub related_events: Vec<crate::pack_events::PackEvent>,
    pub plan_actions: Vec<tuffbox_core::action_plan::LauncherAction>,
    pub human_explanation: Option<String>,
    pub changed_files: Vec<SnapshotChangedFile>,
    /// True when rollback only restores manifest/lockfile (empty changed_files).
    pub manifest_only: bool,
}

// ── Release & Export types ───────────────────────────────────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSnapshotResult {
    pub snapshot: tuffbox_core::Snapshot,
    pub changelog_path: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseArtifactRecord {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub created_at: String,
    pub file_count: usize,
    pub override_count: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDraftResult {
    pub draft_path: String,
    pub metadata_path: String,
    pub artifact_count: usize,
}

// ── Launch & Stats types ─────────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct LaunchStats {
    #[serde(default)]
    pub launches: u64,
    #[serde(default)]
    pub crashes: u64,
    #[serde(default)]
    pub last_launch: Option<String>,
    #[serde(default)]
    pub total_playtime_seconds: u64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct ProjectStats {
    #[serde(default)]
    pub instances: HashMap<String, LaunchStats>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TestRunRecord {
    pub id: String,
    pub profile: String,
    pub started_at: String,
    pub status: String,
    pub log_path: String,
    pub duration_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub captured_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_proc_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_host_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_ram_gb: Option<u32>,
}

#[cfg(test)]
mod test_run_record_tests {
    use super::TestRunRecord;

    #[test]
    fn old_test_runs_json_deserializes_without_peak_fields() {
        let raw = r#"{
            "id": "run-client-1",
            "profile": "client",
            "startedAt": "1",
            "status": "pass",
            "logPath": "logs/latest.log",
            "durationSeconds": 40
        }"#;
        let run: TestRunRecord = serde_json::from_str(raw).expect("legacy json");
        assert_eq!(run.id, "run-client-1");
        assert!(run.peak_proc_mb.is_none());
        assert!(run.peak_host_mb.is_none());
        assert!(run.recommended_ram_gb.is_none());
        assert!(run.captured_paths.is_empty());
    }
}

// ── Crash types ──────────────────────────────────────────────────

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashAiFeedbackPayload {
    pub helped: bool,
    pub fingerprint_key: Option<String>,
    pub human_explanation: Option<String>,
    pub suspected_mods: Option<Vec<String>>,
    pub recommended_actions: Option<Vec<tuffbox_core::ai_explanation::AiAction>>,
    pub report_id: Option<String>,
}

pub struct CrashExitCtx {
    pub log_path: std::path::PathBuf,
    pub mc_version: String,
    pub java_version: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub game_dir: std::path::PathBuf,
}

// ── Optimization types ───────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeModOffer {
    pub slug: String,
    pub name: String,
    pub provider: String,
    pub project_id: String,
    pub version_id: Option<String>,
    pub reason: String,
    pub risk: String,
    pub already_installed: bool,
}

// ── Backup types ─────────────────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BackupIndex {
    pub backups: Vec<BackupEntry>,
    pub max_count: u32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct BackupEntry {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub manifest_snapshot: bool,
}

// ── World Editor types ───────────────────────────────────────────

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkSelection {
    pub region_x: i32,
    pub region_z: i32,
    pub indices: Vec<usize>,
}

// ── Launcher data types ──────────────────────────────────────────

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct LauncherDataState {
    #[serde(default)]
    pub pinned: HashSet<String>,
    #[serde(default)]
    pub last_opened: Option<String>,
}

/// Sidebar / Library recent instance list — survives WebView2 profile wipes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProjectEntry {
    pub path: String,
    pub info: RecentProjectInfo,
    /// Materialized home-cache: listing icon as data URL (optional).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_data_url: Option<String>,
    /// Cached human-readable instance size (e.g. "1.2 GB").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_label: Option<String>,
    /// Cached size in bytes (for invalidation / refresh decisions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Fingerprint of mods/config mtimes when size was scanned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_fingerprint: Option<String>,
    /// Cached total playtime seconds from `.tuffbox/stats.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats_playtime_seconds: Option<u64>,
    /// Cached last launch RFC3339 timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats_last_launch: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProjectInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub minecraft_version: String,
    pub loader_kind: String,
    pub loader_version: String,
    pub java_path: Option<String>,
    pub memory_mb: u32,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    pub player_name: String,
}

// ── Live debug types ─────────────────────────────────────────────

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceLiveStats {
    pub pid: u32,
    pub profile: String,
    pub started_at: u64,
    pub cpu_percent: f32,
    pub memory_mb: u64,
    pub virtual_memory_mb: u64,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveDebugStats {
    pub host_cpu_percent: f32,
    pub host_memory_used_mb: u64,
    pub host_memory_total_mb: u64,
    pub instance: Option<InstanceLiveStats>,
}

// ── Localization ─────────────────────────────────────────────────
