use crate::{
    change_plan::{ChangeAction, ChangePlan, ChangeRisk},
    diagnostics::{Diagnostic, DiagnosticSeverity},
    graph::{DependencyGraph, NodeId},
    manifest::{ModSpec, ProjectManifest},
    resolver::Resolver,
    snapshot::Snapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use thiserror::Error;

const MAX_REPORT_BYTES: u64 = 4 * 1024 * 1024;
const LATEST_LOG_TAIL_LINES: usize = 900;
const MAX_EVIDENCE_PER_SUSPECT: usize = 8;
pub const LATEST_COMPATIBLE_VERSION: &str = "latest-compatible";

#[derive(Debug, Error)]
pub enum CrashError {
    #[error("failed to read crash reports directory {path}: {source}")]
    ReadCrashReportsDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read crash report {path}: {source}")]
    ReadCrashReport {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid crash report path: {0}")]
    InvalidReportPath(String),
    #[error("crash report is too large for inline analysis: {size} bytes")]
    ReportTooLarge { size: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReportSummary {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CrashSignalKind {
    SuspectedMods,
    ModFile,
    CausedBy,
    Mixin,
    Exception,
    OpenGl,
    Performance,
    ResourceWarning,
    Entrypoint,
    LoaderMismatch,
    MissingDependency,
    ModVersionMismatch,
    MinecraftVersionMismatch,
    LoaderVersionMismatch,
    WrongLoader,
    OutOfMemory,
    Watchdog,
    PortConflict,
    EulaNotAccepted,
    CorruptJar,
    DuplicateMod,
    JavaVersion,
    TickingEntity,
    SideMismatch,
    ServerState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashSignal {
    pub source: String,
    pub line_number: usize,
    pub kind: CrashSignalKind,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuspectEvidence {
    pub source: String,
    pub line_number: usize,
    pub kind: CrashSignalKind,
    pub text: String,
}

/// How strongly a suspect is implicated in the crash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum BlameRole {
    #[default]
    Related,
    Secondary,
    Primary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuspectedMod {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub file_name: Option<String>,
    pub known_in_manifest: bool,
    pub confidence: u8,
    pub evidence: Vec<SuspectEvidence>,
    /// Authors from manifest / jar metadata (may be empty).
    #[serde(default)]
    pub authors: Vec<String>,
    /// `primary` / `secondary` / `related` after multi-signal ranking.
    #[serde(default)]
    pub blame_role: BlameRole,
    /// Independent attribution channels that fired for this mod.
    #[serde(default)]
    pub match_sources: Vec<String>,
}

/// A plain-language explanation of a detected crash cause plus actionable
/// remediation steps the user can apply. Returned alongside suspects so the UI
/// can render a "Fix" panel without re-deriving meaning from raw signals.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosisHint {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub detail: String,
    pub steps: Vec<String>,
    /// Mod ids this hint is tied to (may be empty for system-level issues).
    pub related_mods: Vec<String>,
    /// Optional machine-actionable fix the UI can offer a button for.
    pub fix: Option<FixAction>,
    /// When several mods are implicated (e.g. multiple entrypoint/mixin
    /// suspects) this carries one fix action per known mod, so the UI can
    /// render a button for each. Falls back to `fix` when empty.
    pub fixes: Vec<FixAction>,
}

/// A fix the launcher can attempt automatically from the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixAction {
    pub kind: String,
    pub label: String,
    pub mod_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReportSection {
    pub title: String,
    pub start_line: usize,
    pub end_line: usize,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReportModEntry {
    pub id: String,
    pub name: Option<String>,
    pub version: Option<String>,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReportAnalysis {
    pub summary: CrashReportSummary,
    pub content: String,
    pub sections: Vec<CrashReportSection>,
    pub mod_entries: Vec<CrashReportModEntry>,
    pub signals: Vec<CrashSignal>,
    pub suspected_mods: Vec<SuspectedMod>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestLogAnalysis {
    pub path: PathBuf,
    pub exists: bool,
    pub tail: String,
    pub signals: Vec<CrashSignal>,
    pub suspected_mods: Vec<SuspectedMod>,
    pub hints: Vec<DiagnosisHint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashDiagnosis {
    pub reports: Vec<CrashReportSummary>,
    pub selected_report: Option<CrashReportAnalysis>,
    pub latest_log: LatestLogAnalysis,
    pub launcher_log: LatestLogAnalysis,
    pub suspected_mods: Vec<SuspectedMod>,
    pub hints: Vec<DiagnosisHint>,
    pub recent_snapshots: Vec<Snapshot>,
    pub graph_diagnostics: Vec<Diagnostic>,
    pub fix_plan: ChangePlan,
    /// `latest_log` when a newer successful `logs/latest.log` supersedes crash-reports;
    /// otherwise `crash_report` when a report is selected/auto-picked.
    #[serde(default = "default_analysis_source")]
    pub analysis_source: String,
    /// True when a crash-report exists but was ignored because latest.log is newer.
    #[serde(default)]
    pub crash_report_stale: bool,
    /// True when `logs/latest.log` looks like a successful Minecraft session
    /// (no fresh crash markers). Diagnose should not push crash-log fix plans.
    #[serde(default)]
    pub session_healthy: bool,
}

fn default_analysis_source() -> String {
    "crash_report".into()
}

#[derive(Debug, Clone)]
struct ModCandidate<'a> {
    module: &'a ModSpec,
    tokens: Vec<String>,
    file_stem: Option<String>,
}

#[derive(Debug, Clone)]
struct SuspectAccumulator {
    id: String,
    name: String,
    version: Option<String>,
    file_name: Option<String>,
    known_in_manifest: bool,
    confidence: u8,
    evidence: Vec<SuspectEvidence>,
    authors: Vec<String>,
    match_sources: Vec<String>,
}

pub fn list_crash_reports(
    project_dir: impl AsRef<Path>,
) -> Result<Vec<CrashReportSummary>, CrashError> {
    let project_dir = project_dir.as_ref();
    let reports_dir = project_dir.join("crash-reports");
    if !reports_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut reports = Vec::new();
    for entry in fs::read_dir(&reports_dir).map_err(|source| CrashError::ReadCrashReportsDir {
        path: reports_dir.clone(),
        source,
    })? {
        let entry = entry.map_err(|source| CrashError::ReadCrashReportsDir {
            path: reports_dir.clone(),
            source,
        })?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.to_lowercase().ends_with(".txt") {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|source| CrashError::ReadCrashReport {
                path: path.clone(),
                source,
            })?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        reports.push(CrashReportSummary {
            id: format!("crash-reports/{name}"),
            name,
            path,
            size: metadata.len(),
            modified,
        });
    }

    reports.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| b.name.cmp(&a.name))
    });
    Ok(reports)
}

pub fn analyze_crash_report(
    project_dir: impl AsRef<Path>,
    report_id: &str,
    manifest: &ProjectManifest,
) -> Result<CrashReportAnalysis, CrashError> {
    let project_dir = project_dir.as_ref();
    let relative = validate_report_id(report_id)?;
    let path = project_dir.join(&relative);
    let metadata = fs::metadata(&path).map_err(|source| CrashError::ReadCrashReport {
        path: path.clone(),
        source,
    })?;
    if metadata.len() > MAX_REPORT_BYTES {
        return Err(CrashError::ReportTooLarge {
            size: metadata.len(),
        });
    }
    let content = fs::read_to_string(&path).map_err(|source| CrashError::ReadCrashReport {
        path: path.clone(),
        source,
    })?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    let summary = CrashReportSummary {
        id: report_id.to_string(),
        name: Path::new(report_id)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(report_id)
            .to_string(),
        path,
        size: metadata.len(),
        modified,
    };
    let sections = parse_crash_sections(&content);
    let mod_entries = parse_crash_mod_entries(&content, &sections);
    let (signals, suspected_mods) = analyze_text_for_suspects(&content, report_id, manifest);
    Ok(CrashReportAnalysis {
        summary,
        content,
        sections,
        mod_entries,
        signals,
        suspected_mods,
    })
}

pub fn analyze_latest_log(
    project_dir: impl AsRef<Path>,
    manifest: &ProjectManifest,
) -> LatestLogAnalysis {
    analyze_log_file(
        project_dir.as_ref().join("logs").join("latest.log"),
        "logs/latest.log",
        manifest,
    )
}

pub fn analyze_launcher_log(
    project_dir: impl AsRef<Path>,
    manifest: &ProjectManifest,
) -> LatestLogAnalysis {
    let project_dir = project_dir.as_ref();
    let candidates = [
        project_dir.join("launcher.log"),
        project_dir.join("launcher_log.txt"),
        project_dir.join("logs").join("launcher.log"),
        project_dir.join("logs").join("launcher_log.txt"),
    ];
    let log_path = candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .unwrap_or_else(|| project_dir.join("logs").join("launcher.log"));
    analyze_log_file(log_path, "launcher.log", manifest)
}

fn analyze_log_file(path: PathBuf, source: &str, manifest: &ProjectManifest) -> LatestLogAnalysis {
    let exists = path.is_file();
    let tail = if exists {
        crate::process::read_log_tail(&path, LATEST_LOG_TAIL_LINES).unwrap_or_default()
    } else {
        String::new()
    };
    let (signals, suspected_mods) = analyze_text_for_suspects(&tail, source, manifest);
    let hints = build_hints(&signals, &suspected_mods);
    LatestLogAnalysis {
        path,
        exists,
        tail,
        signals,
        suspected_mods,
        hints,
    }
}

pub fn build_crash_diagnosis(
    project_dir: impl AsRef<Path>,
    manifest: &ProjectManifest,
    selected_report_id: Option<&str>,
    recent_snapshots: Vec<Snapshot>,
) -> Result<CrashDiagnosis, CrashError> {
    let project_dir = project_dir.as_ref();
    let reports = list_crash_reports(project_dir)?;
    let latest_log = analyze_latest_log(project_dir, manifest);
    let launcher_log = analyze_launcher_log(project_dir, manifest);

    // Explicit user pick always wins. Special id `__latest_log__` forces live-log
    // analysis (AI Explain / Diagnose sidebar) and never auto-selects a crash file.
    // Auto-pick the newest crash report only when latest.log does not supersede it.
    let force_latest_log = selected_report_id == Some("__latest_log__");
    let explicit = selected_report_id
        .filter(|id| !id.is_empty() && *id != "__latest_log__")
        .filter(|id| reports.iter().any(|report| report.id == *id));
    let newest = reports.first();
    let stale = newest
        .map(|r| latest_log_supersedes_crash(project_dir, Some(r.path.as_path()), &latest_log.tail))
        .unwrap_or(false);

    let selected_id = if force_latest_log {
        None
    } else if let Some(id) = explicit {
        Some(id)
    } else if stale {
        None
    } else {
        newest.map(|report| report.id.as_str())
    };

    let selected_report = selected_id
        .map(|id| analyze_crash_report(project_dir, id, manifest))
        .transpose()?;

    let analysis_source = if selected_report.is_some() {
        "crash_report".to_string()
    } else {
        "latest_log".to_string()
    };

    // Healthy live session (and user did not explicitly open an old crash):
    // suppress crash-log suspects / fix plans so Diagnose doesn't nag about
    // a crash that was already fixed and successfully relaunched.
    let session_healthy =
        explicit.is_none() && log_indicates_healthy_session(&latest_log.tail);

    let mut suspect_sets = Vec::new();
    let mut combined_signals = Vec::new();
    if !session_healthy {
        if let Some(report) = &selected_report {
            suspect_sets.push(report.suspected_mods.clone());
            combined_signals.extend(report.signals.clone());
        }
        suspect_sets.push(latest_log.suspected_mods.clone());
        suspect_sets.push(launcher_log.suspected_mods.clone());
        combined_signals.extend(latest_log.signals.clone());
        combined_signals.extend(launcher_log.signals.clone());
    }
    let suspected_mods = merge_suspected_mods(suspect_sets.into_iter().flatten());
    let suspected_mods = if session_healthy {
        suspected_mods
    } else {
        enrich_diagnosis_suspects(
            project_dir,
            manifest,
            &selected_report,
            &latest_log,
            suspected_mods,
        )
    };

    let graph = DependencyGraph::from_manifest(manifest);
    let graph_diagnostics = Resolver::analyze_project(manifest, &graph);
    let fix_plan = if session_healthy {
        ChangePlan {
            summary: "Minecraft launched successfully — no crash-log fixes needed. Remaining items below are dependency-graph checks only.".to_string(),
            risk: ChangeRisk::Low,
            actions: Vec::new(),
            requires_snapshot: false,
        }
    } else {
        create_crash_fix_plan(
            &graph,
            &graph_diagnostics,
            &suspected_mods,
            &combined_signals,
        )
    };

    let mut hints = if session_healthy {
        Vec::new()
    } else {
        build_hints(&combined_signals, &suspected_mods)
    };
    if session_healthy {
        hints.push(DiagnosisHint {
            id: "session-healthy".into(),
            title: "Build launched successfully".into(),
            severity: "info".into(),
            detail: "latest.log shows a healthy Minecraft session with no fresh crash markers. \
                Historical crash-reports are kept for reference but are not used for fix suggestions."
                .into(),
            steps: vec![
                "Play normally — no crash-log actions are required.".into(),
                "Open a crash-report below only if you want to revisit an old failure.".into(),
                "Use Live logs while the game is running to watch the current session.".into(),
            ],
            related_mods: Vec::new(),
            fix: None,
            fixes: Vec::new(),
        });
    }

    Ok(CrashDiagnosis {
        reports,
        selected_report,
        latest_log,
        launcher_log,
        suspected_mods,
        hints,
        recent_snapshots,
        graph_diagnostics,
        fix_plan,
        analysis_source,
        crash_report_stale: stale && explicit.is_none(),
        session_healthy,
    })
}

/// True when `logs/latest.log` is newer than `crash_report_path` and looks like a
/// post-crash successful (or at least non-crashed) session — so Diagnose should
/// not keep recommending fixes from the old crash-report.
pub fn latest_log_supersedes_crash(
    project_dir: &Path,
    crash_report_path: Option<&Path>,
    latest_log_tail: &str,
) -> bool {
    let latest_path = project_dir.join("logs").join("latest.log");
    let Some(latest_mtime) = file_mtime_secs(&latest_path) else {
        return false;
    };
    let Some(crash_path) = crash_report_path else {
        return false;
    };
    let Some(crash_mtime) = file_mtime_secs(crash_path) else {
        return false;
    };
    if latest_mtime <= crash_mtime {
        return false;
    }
    // Newer log without a fresh crash dump → treat crash-report as historical.
    !log_has_fresh_crash_markers(latest_log_tail)
        || log_indicates_successful_session(latest_log_tail)
}

fn file_mtime_secs(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

fn log_indicates_successful_session(log: &str) -> bool {
    let l = log.to_ascii_lowercase();
    // Vanilla / Fabric / Quilt / NeoForge “game reached playable state” markers.
    l.contains("sound engine started")
        || (l.contains("done (") && (l.contains("for help") || l.contains("!")))
        || l.contains("joining world")
        || l.contains("logged in with entity id")
        || l.contains("openal initialized")
        || l.contains("narrator library")
        || l.contains("completely loaded in")
        || l.contains("[chat]")
        || (l.contains("reloading resource manager")
            && (l.contains("sound engine") || l.contains("openal") || l.contains("done (")))
}

/// Session is healthy when the live log shows a successful boot and no fresh crash dump.
pub fn log_indicates_healthy_session(log: &str) -> bool {
    !log.trim().is_empty()
        && log_indicates_successful_session(log)
        && !log_has_fresh_crash_markers(log)
}

fn log_has_fresh_crash_markers(log: &str) -> bool {
    let l = log.to_ascii_lowercase();
    l.contains("---- minecraft crash report ----")
        || l.contains("#@!@# game crashed")
        || l.contains("game crashed!")
        || l.contains("crash report saved to:")
}

pub fn parse_crash_sections(text: &str) -> Vec<CrashReportSection> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut starts: Vec<(usize, String)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("-- ") && trimmed.ends_with(" --") {
            starts.push((index + 1, trimmed.trim_matches('-').trim().to_string()));
        } else if let Some(title) = forge_section_title(trimmed) {
            starts.push((index + 1, title.to_string()));
        }
    }
    starts.sort_by_key(|(line, _)| *line);
    starts.dedup_by_key(|(line, _)| *line);

    let mut sections = Vec::new();
    for (idx, (start_line, title)) in starts.iter().enumerate() {
        let end_line = starts
            .get(idx + 1)
            .map(|(next, _)| next.saturating_sub(1))
            .unwrap_or(lines.len());
        let preview = lines
            .iter()
            .skip(*start_line)
            .take(end_line.saturating_sub(*start_line).min(10))
            .map(|line| line.trim_end())
            .filter(|line| !line.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(CrashReportSection {
            title: title.clone(),
            start_line: *start_line,
            end_line,
            preview,
        });
    }
    sections
}

fn forge_section_title(line: &str) -> Option<&'static str> {
    let heading = line.strip_suffix(':').unwrap_or(line).trim();
    if heading.eq_ignore_ascii_case("Forge Mod List") {
        Some("Forge Mod List")
    } else if heading.eq_ignore_ascii_case("FML Mod Loading") {
        Some("FML Mod Loading")
    } else if heading.eq_ignore_ascii_case("NeoForge Mod List") {
        Some("NeoForge Mod List")
    } else if heading.eq_ignore_ascii_case("Memory") {
        Some("Memory")
    } else if heading.eq_ignore_ascii_case("JVM Flags") {
        Some("JVM Flags")
    } else if heading.eq_ignore_ascii_case("CPU") {
        Some("CPU")
    } else if heading.eq_ignore_ascii_case("Processor") {
        Some("Processor")
    } else {
        None
    }
}

/// Parse Forge/NeoForge crash report mod table format.
/// Forge crash reports often have one of two table formats:
///   1. "| ID | Name | Version |" pipe-separated tables
///   2. "Mod List:" followed by indented name-version pairs
fn parse_forge_crash_mods(text: &str) -> Vec<CrashReportModEntry> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    // Pattern 1: pipe table
    let mut in_pipe_table = false;
    for (_line_no, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') && trimmed.contains(" ID ") {
            in_pipe_table = true;
            continue;
        }
        if in_pipe_table {
            if !trimmed.starts_with('|') || trimmed.len() < 5 {
                if !entries.is_empty() {
                    break;
                }
                in_pipe_table = false;
                continue;
            }
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim())
                .collect();
            if cells.len() >= 2 && !cells[0].is_empty() && !cells[0].contains('-') {
                entries.push(CrashReportModEntry {
                    id: cells[0].to_string(),
                    name: cells
                        .get(1)
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string()),
                    version: cells
                        .get(2)
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string()),
                    raw: trimmed.to_string(),
                });
                if entries.len() >= 200 {
                    break;
                }
            }
        }
    }

    // Pattern 2: "Mod List:" followed by list
    if entries.is_empty() {
        let mut in_mod_list = false;
        for line in lines {
            let trimmed = line.trim();
            if trimmed.eq_ignore_ascii_case("Mod List:") || trimmed.starts_with("Mod List:") {
                in_mod_list = true;
                continue;
            }
            if in_mod_list {
                if trimmed.is_empty() || trimmed.starts_with("--") || trimmed.starts_with("[") {
                    if !entries.is_empty() {
                        break;
                    }
                    in_mod_list = false;
                    continue;
                }
                // Format: "- modid" or "modid (version)"
                let stripped = trimmed.trim_start_matches('-').trim();
                if let Some((name, ver)) = stripped.rsplit_once('(') {
                    let ver = ver.trim_end_matches(')').trim();
                    entries.push(CrashReportModEntry {
                        id: name.trim().to_string(),
                        name: None,
                        version: Some(ver.to_string()),
                        raw: trimmed.to_string(),
                    });
                } else {
                    entries.push(CrashReportModEntry {
                        id: stripped.to_string(),
                        name: None,
                        version: None,
                        raw: trimmed.to_string(),
                    });
                }
                if entries.len() >= 200 {
                    break;
                }
            }
        }
    }

    entries
}

pub fn parse_crash_mod_entries(
    text: &str,
    sections: &[CrashReportSection],
) -> Vec<CrashReportModEntry> {
    // Try Forge/NeoForge table format first
    let forge_entries = parse_forge_crash_mods(text);
    if !forge_entries.is_empty() {
        return forge_entries;
    }
    // Fallback: vanilla crash report -- Mods -- section
    let lines = text.lines().collect::<Vec<_>>();
    let Some(section) = sections
        .iter()
        .find(|section| section.title.eq_ignore_ascii_case("mods"))
    else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for line in lines
        .iter()
        .skip(section.start_line)
        .take(section.end_line.saturating_sub(section.start_line))
    {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Mod") || trimmed.starts_with('-') {
            continue;
        }
        // Handle pipe-format lines inside the vanilla -- Mods -- section
        // (some hybrid reports mix formats)
        if trimmed.starts_with('|') {
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim())
                .collect();
            if cells.len() >= 2 && !cells[0].is_empty() && !cells[0].contains('-') {
                entries.push(CrashReportModEntry {
                    id: cells[0].to_string(),
                    name: cells
                        .get(1)
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string()),
                    version: cells
                        .get(2)
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string()),
                    raw: trimmed.to_string(),
                });
            }
            continue;
        }
        let normalized =
            trimmed.trim_matches(|c: char| c == '\t' || c == '|' || c == '[' || c == ']');
        let parts = normalized.split_whitespace().collect::<Vec<_>>();
        if parts.is_empty() {
            continue;
        }
        let id = parts[0]
            .trim_matches(|c: char| c == ':' || c == '|')
            .to_string();
        if id.len() < 2 || id.contains("----") {
            continue;
        }
        let version = parts
            .iter()
            .rev()
            .find(|part| part.chars().any(|c| c.is_ascii_digit()))
            .map(|part| part.trim_matches('|').to_string());
        let name = if parts.len() > 2 {
            Some(
                parts[1..parts.len().saturating_sub(1)]
                    .join(" ")
                    .trim()
                    .to_string(),
            )
            .filter(|s| !s.is_empty())
        } else {
            None
        };
        entries.push(CrashReportModEntry {
            id,
            name,
            version,
            raw: trimmed.to_string(),
        });
    }
    entries.truncate(300);
    entries
}

pub fn analyze_text_for_suspects(
    text: &str,
    source: &str,
    manifest: &ProjectManifest,
) -> (Vec<CrashSignal>, Vec<SuspectedMod>) {
    let candidates = build_candidates(manifest);
    let mut signals = Vec::new();
    let mut suspects: BTreeMap<String, SuspectAccumulator> = BTreeMap::new();

    for (index, line) in text.lines().enumerate() {
        let line_number = index + 1;
        let Some(kind) = classify_signal_line(line) else {
            continue;
        };

        let signal = CrashSignal {
            source: source.to_string(),
            line_number,
            kind,
            text: line.trim().to_string(),
        };
        signals.push(signal);

        if matches!(
            kind,
            CrashSignalKind::Entrypoint
                | CrashSignalKind::LoaderMismatch
                | CrashSignalKind::CausedBy
        ) {
            for mod_id in extract_quoted_mod_ids(line) {
                if let Some(candidate) = candidates
                    .iter()
                    .find(|candidate| candidate.tokens.iter().any(|t| t == &mod_id))
                {
                    add_manifest_suspect(
                        &mut suspects,
                        candidate.module,
                        evidence(source, line_number, kind, line),
                        96,
                    );
                } else if !is_noise_token(&mod_id) {
                    add_inferred_suspect(
                        &mut suspects,
                        &mod_id,
                        None,
                        evidence(source, line_number, kind, line),
                        82,
                    );
                }
            }
        }

        if !matches!(
            kind,
            CrashSignalKind::Performance | CrashSignalKind::ResourceWarning
        ) {
            for candidate in &candidates {
                if candidate_matches_line(candidate, line) {
                    add_manifest_suspect(
                        &mut suspects,
                        candidate.module,
                        evidence(source, line_number, kind, line),
                        confidence_for_kind(kind),
                    );
                }
            }

            // Stack-trace FQN / mixin package attribution: `knot//
            // net.earthcomputer.clientcommands...PlayerRandCracker` or
            // `dev.isxander.controlify.mixins...` map to the mod whose id/name
            // matches the package component, without blind substring matches.
            if matches!(
                kind,
                CrashSignalKind::Exception
                    | CrashSignalKind::Mixin
                    | CrashSignalKind::Entrypoint
                    | CrashSignalKind::CausedBy
            ) {
                for pkg in extract_java_packages(line) {
                    for candidate in &candidates {
                        if candidate_matches_java_package(candidate, &pkg) {
                            add_manifest_suspect(
                                &mut suspects,
                                candidate.module,
                                evidence(source, line_number, kind, line),
                                88,
                            );
                            if let Some(entry) =
                                suspects.get_mut(&normalize_token(&candidate.module.id))
                            {
                                push_match_source(entry, "package");
                            }
                        }
                    }
                }
            }
        }

        if matches!(kind, CrashSignalKind::ModFile) {
            for jar_name in extract_jar_names(line) {
                if let Some(candidate) = candidates
                    .iter()
                    .find(|candidate| jar_matches_candidate(&jar_name, candidate))
                {
                    add_manifest_suspect(
                        &mut suspects,
                        candidate.module,
                        evidence(source, line_number, kind, line),
                        92,
                    );
                } else {
                    let inferred = infer_id_from_jar(&jar_name);
                    if !inferred.is_empty() && !is_noise_token(&inferred) {
                        add_inferred_suspect(
                            &mut suspects,
                            &inferred,
                            Some(jar_name),
                            evidence(source, line_number, kind, line),
                            68,
                        );
                    }
                }
            }
        }

        if matches!(
            kind,
            CrashSignalKind::MissingDependency
                | CrashSignalKind::ModVersionMismatch
                | CrashSignalKind::MinecraftVersionMismatch
                | CrashSignalKind::LoaderVersionMismatch
                | CrashSignalKind::WrongLoader
        ) {
            for mod_id in extract_named_mods(line) {
                if let Some(candidate) = candidates
                    .iter()
                    .find(|candidate| candidate.tokens.iter().any(|t| t == &mod_id))
                {
                    add_manifest_suspect(
                        &mut suspects,
                        candidate.module,
                        evidence(source, line_number, kind, line),
                        confidence_for_kind(kind),
                    );
                } else if !is_noise_token(&mod_id) {
                    add_inferred_suspect(
                        &mut suspects,
                        &mod_id,
                        None,
                        evidence(source, line_number, kind, line),
                        confidence_for_kind(kind).saturating_sub(8),
                    );
                }
            }
        }

        if matches!(
            kind,
            CrashSignalKind::Mixin | CrashSignalKind::SuspectedMods
        ) {
            for token in tokenize(line) {
                if token.len() < 3 || is_noise_token(&token) {
                    continue;
                }
                if let Some(candidate) = candidates
                    .iter()
                    .find(|candidate| candidate.tokens.iter().any(|t| t == &token))
                {
                    add_manifest_suspect(
                        &mut suspects,
                        candidate.module,
                        evidence(source, line_number, kind, line),
                        confidence_for_kind(kind),
                    );
                }
            }
        }
    }

    let suspected_mods = suspects
        .into_values()
        .map(accumulator_to_suspect)
        .collect::<Vec<_>>();

    (signals, merge_suspected_mods(suspected_mods))
}

pub fn merge_suspected_mods(mods: impl IntoIterator<Item = SuspectedMod>) -> Vec<SuspectedMod> {
    let mut by_id: BTreeMap<String, SuspectAccumulator> = BTreeMap::new();
    for module in mods {
        let key = compact_token(&normalize_token(&module.id));
        let entry = by_id.entry(key).or_insert_with(|| SuspectAccumulator {
            id: module.id.clone(),
            name: module.name.clone(),
            version: module.version.clone(),
            file_name: module.file_name.clone(),
            known_in_manifest: module.known_in_manifest,
            confidence: 0,
            evidence: Vec::new(),
            authors: module.authors.clone(),
            match_sources: module.match_sources.clone(),
        });
        entry.confidence = entry.confidence.max(module.confidence);
        if module.known_in_manifest && !entry.known_in_manifest {
            entry.id = module.id.clone();
            entry.name = module.name.clone();
            entry.version = module.version.clone();
            entry.file_name = module.file_name.clone();
            if !module.authors.is_empty() {
                entry.authors = module.authors.clone();
            }
        }
        entry.known_in_manifest |= module.known_in_manifest;
        if entry.version.is_none() {
            entry.version = module.version.clone();
        }
        if entry.file_name.is_none() {
            entry.file_name = module.file_name.clone();
        }
        if entry.authors.is_empty() && !module.authors.is_empty() {
            entry.authors = module.authors.clone();
        }
        for src in &module.match_sources {
            push_match_source(entry, src);
        }
        for evidence in module.evidence {
            if entry.evidence.len() >= MAX_EVIDENCE_PER_SUSPECT {
                break;
            }
            if !entry.evidence.iter().any(|item| {
                item.source == evidence.source && item.line_number == evidence.line_number
            }) {
                push_match_source(entry, &match_source_for_kind(evidence.kind));
                entry.evidence.push(evidence);
            }
        }
        entry.confidence = entry
            .confidence
            .saturating_add((entry.evidence.len().saturating_sub(1) as u8).min(7));
    }

    let mut out = by_id
        .into_values()
        .map(accumulator_to_suspect)
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then_with(|| b.known_in_manifest.cmp(&a.known_in_manifest))
            .then_with(|| a.name.cmp(&b.name))
    });
    assign_blame_roles(&mut out);
    out
}

/// Build plain-language remediation hints from the detected signals and the
/// suspected mods they reference. Each hint carries actionable steps and an
/// optional machine-actionable `FixAction` the launcher UI can trigger.
pub fn build_hints(signals: &[CrashSignal], suspects: &[SuspectedMod]) -> Vec<DiagnosisHint> {
    let kinds: HashSet<CrashSignalKind> = signals.iter().map(|s| s.kind.clone()).collect();
    let mut hints: Vec<DiagnosisHint> = Vec::new();

    let mut push = |h: DiagnosisHint| {
        if !hints.iter().any(|existing| existing.id == h.id) {
            hints.push(h);
        }
    };

    // Top mod suspect, if any (highest confidence, known in manifest).
    let top = suspects
        .iter()
        .find(|s| s.known_in_manifest)
        .or_else(|| suspects.first());

    if kinds.contains(&CrashSignalKind::OutOfMemory) {
        push(DiagnosisHint {
            id: "out-of-memory".into(),
            title: "Not enough memory (OutOfMemoryError)".into(),
            severity: "critical".into(),
            detail: "The JVM ran out of heap memory. This is usually caused by too many \
                mods/entities/chunks for the allocated RAM, or a memory leak in a mod."
                .into(),
            steps: vec![
                "Increase the JVM heap: set memory_mb in project settings to at least 4–6 GB for heavily modded instances.".into(),
                "Lower view-distance / simulation-distance in the world or server settings.".into(),
                "Pre-generate chunks to reduce runtime world generation load.".into(),
                "If it recurs after raising RAM, a specific mod likely leaks memory — bisect mods.".into(),
            ],
            related_mods: top.map(|s| vec![s.id.clone()]).unwrap_or_default(),
            fix: Some(FixAction {
                kind: "raiseMemory".into(),
                label: "Raise allocated memory to 6 GB".into(),
                mod_id: None,
            }),
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::Watchdog) {
        push(DiagnosisHint {
            id: "watchdog".into(),
            title: "Server watchdog timeout".into(),
            severity: "critical".into(),
            detail: "A single server tick took too long, so the watchdog force-stopped the server. \
                Usually a slow mod, overloaded world, or insufficient CPU/RAM."
                .into(),
            steps: vec![
                "Reduce view-distance / simulation-distance and entity counts.".into(),
                "Allocate more RAM and ensure the JVM has enough CPU headroom.".into(),
                "Remove or update the mod responsible for the slow tick (check ticking-entity/block-entity errors).".into(),
            ],
            related_mods: top.map(|s| vec![s.id.clone()]).unwrap_or_default(),
            fix: None,
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::EulaNotAccepted) {
        push(DiagnosisHint {
            id: "eula".into(),
            title: "EULA not accepted".into(),
            severity: "critical".into(),
            detail: "The server refuses to start until you accept Mojang's EULA."
                .into(),
            steps: vec![
                "Open eula.txt in the instance folder and set eula=true.".into(),
                "Restart the server afterwards.".into(),
            ],
            related_mods: Vec::new(),
            fix: Some(FixAction {
                kind: "acceptEula".into(),
                label: "Accept EULA (set eula.txt eula=true)".into(),
                mod_id: None,
            }),
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::PortConflict) {
        push(DiagnosisHint {
            id: "port-conflict".into(),
            title: "Port already in use".into(),
            severity: "critical".into(),
            detail: "Another process (often a previous server instance that did not shut down) \
                is already holding the Minecraft port (usually 25565)."
                .into(),
            steps: vec![
                "Stop the other server / Java process, or restart the machine.".into(),
                "Or change server-port in server.properties to a free port (e.g. 25566).".into(),
                "Ensure server-ip is empty unless you must bind to a specific address.".into(),
            ],
            related_mods: Vec::new(),
            fix: Some(FixAction {
                kind: "changePort".into(),
                label: "Use port 25566 instead".into(),
                mod_id: None,
            }),
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::CorruptJar) {
        push(DiagnosisHint {
            id: "corrupt-jar".into(),
            title: "Corrupted mod jar".into(),
            severity: "critical".into(),
            detail: "A mod file is corrupt (zip END header / CEN header error) — usually from an \
                interrupted download. The failing jar name is printed in the error."
                .into(),
            steps: vec![
                "Re-download the named mod jar from its source (Modrinth/CurseForge) and replace the file.".into(),
                "Delete the corrupt file and let TuffBox re-fetch it if it is a managed mod.".into(),
                "If unsure which jar, re-download the most recently added mods first.".into(),
            ],
            related_mods: top.map(|s| vec![s.id.clone()]).unwrap_or_default(),
            fix: top
                .filter(|s| s.known_in_manifest)
                .map(|s| FixAction {
                    kind: "reinstallMod".into(),
                    label: format!("Re-download {}", s.name),
                    mod_id: Some(s.id.clone()),
                }),
                fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::DuplicateMod) {
        push(DiagnosisHint {
            id: "duplicate-mod".into(),
            title: "Duplicate mod detected".into(),
            severity: "critical".into(),
            detail: "Two copies of the same mod are present (often an old jar left after updating). \
                The loader refuses to start."
                .into(),
            steps: vec![
                "Open the mods folder and delete the older/duplicate jar of the named mod.".into(),
                "Keep only one version of each mod.".into(),
            ],
            related_mods: top.map(|s| vec![s.id.clone()]).unwrap_or_default(),
            fix: None,
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::JavaVersion) {
        push(DiagnosisHint {
            id: "java-version".into(),
            title: "Wrong Java version".into(),
            severity: "critical".into(),
            detail: "UnsupportedClassVersionError means the mod/loader was built for a newer Java \
                than the one running. Modern Minecraft needs Java 17 (1.18+) or Java 21 (1.20.5+ / NeoForge)."
                .into(),
            steps: vec![
                "Install the Java version required by your Minecraft version and point the project at it.".into(),
                "1.17–1.20.4 → Java 17; 1.20.5+ and recent NeoForge → Java 21.".into(),
                "Update the loader if it also requires a newer Java.".into(),
            ],
            related_mods: Vec::new(),
            fix: Some(FixAction {
                kind: "autoJava".into(),
                label: "Auto-select a compatible Java runtime".into(),
                mod_id: None,
            }),
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::TickingEntity) {
        push(DiagnosisHint {
            id: "ticking-entity".into(),
            title: "Crash while ticking an entity/block".into(),
            severity: "high".into(),
            detail: "A specific entity or block entity threw an exception during its tick — the \
                stack trace names the exact class, which identifies the culprit mod."
                .into(),
            steps: vec![
                "Identify the entity/block from the stack trace and remove or update that mod.".into(),
                "If a chunk is corrupted, restore it from a backup or delete the region file.".into(),
                "As a last resort, remove the most recently added mod and retest.".into(),
            ],
            related_mods: top.map(|s| vec![s.id.clone()]).unwrap_or_default(),
            fix: top
                .filter(|s| s.known_in_manifest)
                .map(|s| FixAction {
                    kind: "disableMod".into(),
                    label: format!("Disable {}", s.name),
                    mod_id: Some(s.id.clone()),
                }),
                fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::SideMismatch) {
        push(DiagnosisHint {
            id: "side-mismatch".into(),
            title: "Mod loaded on the wrong side".into(),
            severity: "high".into(),
            detail: "A mod tried to load a client-only class on a server (or vice versa). This \
                happens with client-only mods installed on a dedicated server."
                .into(),
            steps: vec![
                "Remove the client-only mod from the server's mods folder.".into(),
                "Keep server-only mods out of the client instance.".into(),
            ],
            related_mods: top.map(|s| vec![s.id.clone()]).unwrap_or_default(),
            fix: top
                .filter(|s| s.known_in_manifest)
                .map(|s| FixAction {
                    kind: "disableMod".into(),
                    label: format!("Disable {}", s.name),
                    mod_id: Some(s.id.clone()),
                }),
                fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::ServerState) {
        push(DiagnosisHint {
            id: "server-state".into(),
            title: "World/session lock after a crash".into(),
            severity: "high".into(),
            detail: "The server was killed mid-run (power loss / hard crash) and left a session \
                lock or inconsistent state file. Minecraft sometimes corrupts its own JSON on sudden shutdown."
                .into(),
            steps: vec![
                "Delete session.lock in the world folder if present.".into(),
                "Restore the world from the most recent backup.".into(),
                "Make sure the previous server process is fully stopped before restarting.".into(),
            ],
            related_mods: Vec::new(),
            fix: None,
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::MissingDependency) {
        let names: Vec<String> = suspects.iter().map(|s| s.id.clone()).collect();
        push(DiagnosisHint {
            id: "missing-dependency".into(),
            title: "Missing mod dependency".into(),
            severity: "high".into(),
            detail: "One or more mods require another mod that is not installed (or could not be \
                loaded). The loader reports it as a ModResolutionException / missing dependency."
                .into(),
            steps: vec![
                "Install the missing dependency mod for the same Minecraft + loader version.".into(),
                "If the dependency is present, it may be the wrong version — update it.".into(),
                "For JIJ (jar-in-jar) dependencies, update the parent mod.".into(),
            ],
            related_mods: names.clone(),
            fix: if names.is_empty() {
                None
            } else {
                Some(FixAction {
                    kind: "installDependency".into(),
                    label: "Try to install missing dependencies".into(),
                    mod_id: names.into_iter().next(),
                })
            },
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::ModVersionMismatch) {
        let names: Vec<String> = suspects.iter().map(|s| s.id.clone()).collect();
        push(DiagnosisHint {
            id: "version-mismatch".into(),
            title: "Mod / version conflict".into(),
            severity: "high".into(),
            detail: "Two mods conflict, or a mod is the wrong version for your setup. Common with \
                mixin conflicts or libraries at incompatible versions."
                .into(),
            steps: vec![
                "Update the conflicting mod(s) to versions compatible with your Minecraft + loader.".into(),
                "If two mods edit the same feature, keep only one or use a compatibility patch.".into(),
                "Check the mod's issue tracker for known incompatibilities.".into(),
            ],
            related_mods: names.clone(),
            fix: if names.is_empty() {
                None
            } else {
                Some(FixAction {
                    kind: "updateMod".into(),
                    label: "Update suspected mod(s)".into(),
                    mod_id: names.into_iter().next(),
                })
            },
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::MinecraftVersionMismatch) {
        push(DiagnosisHint {
            id: "minecraft-version".into(),
            title: "Wrong Minecraft version for mod".into(),
            severity: "high".into(),
            detail: "A mod requires a different Minecraft version than the one installed."
                .into(),
            steps: vec![
                "Either downgrade/upgrade Minecraft to the version the mod supports, or".into(),
                "Replace the mod with a build for your current Minecraft version.".into(),
            ],
            related_mods: top.map(|s| vec![s.id.clone()]).unwrap_or_default(),
            fix: top
                .filter(|s| s.known_in_manifest)
                .map(|s| FixAction {
                    kind: "updateMod".into(),
                    label: format!("Update {}", s.name),
                    mod_id: Some(s.id.clone()),
                }),
                fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::WrongLoader) {
        push(DiagnosisHint {
            id: "wrong-loader".into(),
            title: "Wrong mod loader".into(),
            severity: "high".into(),
            detail: "A mod is built for a different loader (e.g. Forge mod on Fabric, or vice versa)."
                .into(),
            steps: vec![
                "Install the correct loader (Fabric/Forge/NeoForge/Quilt) for the mod.".into(),
                "Or replace the mod with a port for your current loader.".into(),
            ],
            related_mods: top.map(|s| vec![s.id.clone()]).unwrap_or_default(),
            fix: None,
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::LoaderVersionMismatch) {
        push(DiagnosisHint {
            id: "loader-version".into(),
            title: "Wrong loader version".into(),
            severity: "high".into(),
            detail: "A mod requires a newer (or older) version of the mod loader than is installed."
                .into(),
            steps: vec![
                "Update the mod loader to the version the mod requires.".into(),
                "Fabric Loader, Forge, NeoForge and Quilt each have their own version line.".into(),
            ],
            related_mods: Vec::new(),
            fix: Some(FixAction {
                kind: "updateLoader".into(),
                label: "Update mod loader".into(),
                mod_id: None,
            }),
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::Entrypoint) {
        // Prefer the mod named in "provided by '…'" over every weak suspect.
        let mut related = Vec::new();
        for signal in signals {
            if signal.kind != CrashSignalKind::Entrypoint {
                continue;
            }
            for id in extract_quoted_mod_ids(&signal.text) {
                if !related.contains(&id) {
                    related.push(id);
                }
            }
        }
        for s in suspects.iter().filter(|s| s.known_in_manifest) {
            if !related.contains(&s.id) {
                related.push(s.id.clone());
            }
            if related.len() >= 3 {
                break;
            }
        }
        related.truncate(3);
        push(DiagnosisHint {
            id: "entrypoint".into(),
            title: "Mod entrypoint failed".into(),
            severity: "high".into(),
            detail: "A mod's initialization code threw while the game was starting. Often a \
                version mismatch or a missing dependency for that specific mod."
                .into(),
            steps: vec![
                "Update or remove the mod named in the error.".into(),
                "Check for a missing dependency the mod requires.".into(),
            ],
            related_mods: related.clone(),
            fix: related.first().and_then(|id| {
                suspects
                    .iter()
                    .find(|s| &s.id == id && s.known_in_manifest)
                    .map(|s| FixAction {
                        kind: "disableMod".into(),
                        label: format!("Disable {}", s.name),
                        mod_id: Some(s.id.clone()),
                    })
            }),
            fixes: vec![],
        });
    }

    if kinds.contains(&CrashSignalKind::Mixin) {
        let mut related = Vec::new();
        for s in suspects.iter().filter(|s| s.known_in_manifest) {
            related.push(s.id.clone());
            if related.len() >= 3 {
                break;
            }
        }
        push(DiagnosisHint {
            id: "mixin".into(),
            title: "Mixin injection failure".into(),
            severity: "high".into(),
            detail: "A mod failed to apply its mixin transformers. Usually caused by a wrong \
                Minecraft/loader version, two mods editing the same code, or a library mismatch."
                .into(),
            steps: vec![
                "Update the mod whose mixin failed (named in the error / stack trace).".into(),
                "If two mods conflict on the same class, keep only one or add a compat patch.".into(),
                "Verify the mod supports your exact Minecraft + loader version.".into(),
            ],
            related_mods: related,
            fix: top
                .filter(|s| s.known_in_manifest)
                .map(|s| FixAction {
                    kind: "updateMod".into(),
                    label: format!("Update {}", s.name),
                    mod_id: Some(s.id.clone()),
                }),
            fixes: vec![],
        });
    }

    // For hints that implicate several installed mods, offer a Fix button per
    // related mod (capped). Never expand to the entire mod list.
    let known_by_id: std::collections::HashMap<&str, &SuspectedMod> = suspects
        .iter()
        .filter(|s| s.known_in_manifest)
        .map(|s| (s.id.as_str(), s))
        .collect();
    for hint in hints.iter_mut() {
        if !hint.fixes.is_empty() {
            continue;
        }
        let Some(kind) = mod_fix_kind_for_hint(&hint.id) else {
            continue;
        };
        if hint.related_mods.is_empty() {
            continue;
        }
        let targets: Vec<&SuspectedMod> = hint
            .related_mods
            .iter()
            .filter_map(|id| known_by_id.get(id.as_str()).copied())
            .take(3)
            .collect();
        if targets.len() <= 1 {
            continue;
        }
        hint.fixes = targets
            .iter()
            .map(|s| FixAction {
                kind: kind.to_string(),
                label: format!("{} {}", fix_verb(kind), s.name),
                mod_id: Some(s.id.clone()),
            })
            .collect();
    }

    hints
}

/// Maps a diagnosis-hint id to the fix action kind appropriate for a
/// per-mod button (disable / update / reinstall).
fn mod_fix_kind_for_hint(hint_id: &str) -> Option<&'static str> {
    match hint_id {
        "corrupt-jar" => Some("reinstallMod"),
        "ticking-entity" | "side-mismatch" | "entrypoint" => Some("disableMod"),
        "mixin" | "version-mismatch" | "minecraft-version" => Some("updateMod"),
        _ => None,
    }
}

/// Verb shown on the per-mod fix button label.
fn fix_verb(kind: &str) -> &'static str {
    match kind {
        "reinstallMod" => "Reinstall",
        "disableMod" => "Disable",
        "updateMod" => "Update",
        _ => "Fix",
    }
}

pub fn create_crash_fix_plan(
    graph: &DependencyGraph,
    diagnostics: &[Diagnostic],
    suspected_mods: &[SuspectedMod],
    signals: &[CrashSignal],
) -> ChangePlan {
    if let Some(top) = suspected_mods.first() {
        let node_id = NodeId::module(&top.id);
        let mut actions = Vec::new();
        if top.known_in_manifest && graph.has_node(&node_id) {
            actions.push(ChangeAction::DisableMod {
                node_id: node_id.clone(),
            });
            actions.push(ChangeAction::UpdateMod {
                node_id: node_id.clone(),
                target_version: LATEST_COMPATIBLE_VERSION.to_string(),
            });
        }
        return ChangePlan {
            summary: if top.known_in_manifest {
                format!(
                    "Create a safety snapshot, then disable suspected mod {} (jar → .disabled) and rerun. If needed, update it to the latest compatible build afterward.",
                    top.name
                )
            } else {
                format!(
                    "Inspect inferred crash source `{}`. It is not mapped to a manifest mod yet, so verify local jars and latest.log before applying changes.",
                    top.name
                )
            },
            risk: ChangeRisk::Medium,
            actions,
            requires_snapshot: true,
        };
    }

    if signals
        .iter()
        .any(|signal| signal.kind == CrashSignalKind::OpenGl)
    {
        return ChangePlan {
            summary: "OpenGL render pipeline errors detected (`GL_INVALID_OPERATION`). Treat this as a graphics/rendering compatibility issue first: update GPU drivers, disable shaders, then test without render optimization or visual mods such as Sodium/Iris/Voxy/ETF/MCEF/Litematica one group at a time.".to_string(),
            risk: ChangeRisk::Medium,
            actions: Vec::new(),
            requires_snapshot: true,
        };
    }

    if signals
        .iter()
        .any(|signal| signal.kind == CrashSignalKind::Performance)
    {
        return ChangePlan {
            summary: "Performance stall detected (`Can't keep up`). Reduce render/simulation load, lower view distance, review worldgen/entity-heavy mods and rerun the Test profile.".to_string(),
            risk: ChangeRisk::Low,
            actions: Vec::new(),
            requires_snapshot: false,
        };
    }

    if let Some(plan) = Resolver::create_fix_plan(graph, diagnostics) {
        return plan;
    }

    let has_errors = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error);
    ChangePlan {
        summary: if has_errors {
            "Review graph diagnostics first, then rerun Test to produce a fresh latest.log."
                .to_string()
        } else {
            "No deterministic crash source found yet. Reproduce the crash, then analyze crash-reports/latest and logs/latest.log.".to_string()
        },
        risk: ChangeRisk::Low,
        actions: Vec::new(),
        requires_snapshot: false,
    }
}

pub fn resolve_update_target_version(target_version: &str) -> Option<&str> {
    let target_version = target_version.trim();
    if target_version.is_empty() || target_version.eq_ignore_ascii_case(LATEST_COMPATIBLE_VERSION) {
        None
    } else {
        Some(target_version)
    }
}

fn build_candidates(manifest: &ProjectManifest) -> Vec<ModCandidate<'_>> {
    manifest
        .mods
        .iter()
        .map(|module| {
            let mut tokens = HashSet::new();
            insert_token_variants(&mut tokens, &module.id);
            insert_token_variants(&mut tokens, &module.name);
            if let Some(file_name) = &module.file_name {
                insert_token_variants(&mut tokens, file_name);
            }
            if let Some(project_id) = &module.source.project_id {
                insert_token_variants(&mut tokens, project_id);
            }
            let file_stem = module
                .file_name
                .as_deref()
                .map(|file| normalize_token(file.trim_end_matches(".jar")));
            ModCandidate {
                module,
                tokens: tokens
                    .into_iter()
                    .filter(|token| !is_noise_token(token))
                    .collect(),
                file_stem,
            }
        })
        .collect()
}

fn insert_token_variants(tokens: &mut HashSet<String>, value: &str) {
    let normalized = normalize_token(value.trim_end_matches(".jar"));
    if !normalized.is_empty() {
        tokens.insert(normalized.clone());
        let compact = compact_token(&normalized);
        if compact.len() >= 4 {
            tokens.insert(compact);
        }
    }
    for token in tokenize(value) {
        if token.len() >= 3 {
            let compact = compact_token(&token);
            tokens.insert(token);
            if compact.len() >= 4 {
                tokens.insert(compact);
            }
        }
    }
}

fn classify_signal_line(line: &str) -> Option<CrashSignalKind> {
    let lower = line.to_lowercase();

    // ---- System / environment crashes (highest priority) ----
    if lower.contains("java.lang.outofmemoryerror")
        || lower.contains("out of memory")
        || lower.contains("gc overhead limit exceeded")
    {
        return Some(CrashSignalKind::OutOfMemory);
    }
    if lower.contains("watchdog")
        || lower.contains("server watchdog")
        || lower.contains("the server has stopped responding")
        || lower.contains("a single server tick took")
    {
        return Some(CrashSignalKind::Watchdog);
    }
    if lower.contains("you need to agree to the eula")
        || lower.contains("eula.txt")
        || (lower.contains("eula") && lower.contains("not accepted"))
    {
        return Some(CrashSignalKind::EulaNotAccepted);
    }
    if lower.contains("failed to bind to port")
        || lower.contains("address already in use")
        || lower.contains("bind(..) failed")
    {
        return Some(CrashSignalKind::PortConflict);
    }
    if lower.contains("zip end header not found")
        || lower.contains("invalid cen header")
        || lower.contains("zipexception")
        || lower.contains("corrupt jar")
        || lower.contains("invalid or corrupt jarfile")
        || lower.contains("error analyzing")
    {
        return Some(CrashSignalKind::CorruptJar);
    }
    if lower.contains("duplicate mod")
        || lower.contains("duplicate mods")
        || (lower.contains("found duplicate") && lower.contains("mod"))
        || lower.contains("mod already loaded")
    {
        return Some(CrashSignalKind::DuplicateMod);
    }
    if lower.contains("unsupportedclassversionerror")
        || (lower.contains("unsupported") && lower.contains("class version"))
        || lower.contains("has been compiled by a more recent version of the java runtime")
    {
        return Some(CrashSignalKind::JavaVersion);
    }
    if lower.contains("ticking entity")
        || lower.contains("ticking block entity")
        || lower.contains("exception in server tick loop")
        || lower.contains("exception ticking")
    {
        return Some(CrashSignalKind::TickingEntity);
    }
    if lower.contains("attempted to load class")
        && lower.contains("invalid side")
        || lower.contains("for invalid side")
        || (lower.contains("client class") && lower.contains("server"))
    {
        return Some(CrashSignalKind::SideMismatch);
    }
    if lower.contains("state engine was in the incorrect state")
        || lower.contains("forced into state server_stopped")
        || lower.contains("failed to check session lock")
    {
        return Some(CrashSignalKind::ServerState);
    }

    // ---- Dependency / version / loader resolution errors ----
    // These are the most actionable crash causes, so they take priority.
    let is_resolution_error = lower.contains("which is missing")
        || lower.contains("is missing!")
        || (lower.contains("missing") && lower.contains("mod"))
        || lower.contains("missing dependency")
        || lower.contains("could not be loaded")
        || lower.contains("dependency")
        || lower.contains("requires ")
        || lower.contains("incompatible mod set")
        || lower.contains("conflict")
        || lower.contains("incompatible")
        || lower.contains("modresolutionexception");
    if is_resolution_error {
        // Narrow down to a more specific kind when the text is explicit.
        let mentions_minecraft = lower.contains("minecraft");
        // A loader *name* (fabric/forge/...) in the text only counts when it is
        // clearly the subject, not e.g. the `net.fabricmc.loader` package in a
        // ModResolutionException stack line.
        let mentions_loader_kind = lower.contains("wrong loader")
            || lower.contains("not a fabric mod")
            || lower.contains("not a forge mod")
            || lower.contains("not a neoforge mod")
            || lower.contains("mod loader")
            || lower.contains("is in use")
            || lower.contains("requires the fabric")
            || lower.contains("requires the forge")
            || lower.contains("requires the neoforge")
            || lower.contains("requires the quilt");
        let mentions_loader_version = lower.contains("fabricloader")
            || lower.contains("fabric loader")
            || lower.contains("loader version")
            || lower.contains("loader 0.")
            || (lower.contains("loader") && (lower.contains("below") || lower.contains("above")));
        let mentions_version_mismatch = lower.contains("non-matching version")
            || lower.contains("wrong version")
            || lower.contains("but a ")
            || lower.contains("which is present")
            || lower.contains("incompatible")
            || lower.contains("conflict");

        // Explicit missing-dependency markers win over generic loader checks.
        if lower.contains("which is missing")
            || lower.contains("is missing!")
            || lower.contains("modresolutionexception")
            || lower.contains("missing dependency")
            || lower.contains("could not be loaded")
        {
            return Some(CrashSignalKind::MissingDependency);
        }
        if mentions_minecraft
            && (lower.contains("requires")
                || lower.contains("needs")
                || mentions_version_mismatch)
        {
            return Some(CrashSignalKind::MinecraftVersionMismatch);
        }
        // Wrong-loader (Forge mod on Fabric, etc.) takes priority over a plain
        // loader-version requirement.
        if mentions_loader_kind {
            return Some(CrashSignalKind::WrongLoader);
        }
        if mentions_loader_version {
            return Some(CrashSignalKind::LoaderVersionMismatch);
        }
        if mentions_version_mismatch {
            return Some(CrashSignalKind::ModVersionMismatch);
        }
        return Some(CrashSignalKind::MissingDependency);
    }
    if lower.contains("could not execute entrypoint")
        || (lower.contains("provided by '")
            && (lower.contains("due to errors")
                || lower.contains("could not execute")
                || lower.contains("exception")
                || lower.contains("failed to")
                || lower.contains("fatal")))
    {
        return Some(CrashSignalKind::Entrypoint);
    }
    if lower.contains("nosuchmethoderror")
        || lower.contains("nosuchfielderror")
        || lower.contains("net.neoforged.fml") && lower.contains("fabric")
    {
        return Some(CrashSignalKind::LoaderMismatch);
    }
    if lower.contains("opengl debug message")
        || lower.contains("gl_invalid_operation")
        || lower.contains("gl_invalid_")
        || lower.contains("blaze3d.opengl.gldebug")
    {
        return Some(CrashSignalKind::OpenGl);
    }
    if lower.contains("can't keep up!") || lower.contains("is the server overloaded?") {
        return Some(CrashSignalKind::Performance);
    }
    if lower.contains("invalid mod icon")
        || lower.contains("broken icon")
        || lower.contains("lingering jcef helper")
    {
        return Some(CrashSignalKind::ResourceWarning);
    }
    if lower.contains("suspected mod") || lower.contains("suspected mods") {
        return Some(CrashSignalKind::SuspectedMods);
    }
    if lower.contains("mod file")
        || lower.contains("modfile")
        || lower.ends_with(".jar") && lower.contains("/mods/")
    {
        return Some(CrashSignalKind::ModFile);
    }
    if lower.contains("caused by:") {
        return Some(CrashSignalKind::CausedBy);
    }
    // Only treat as a mixin *failure* when the line clearly reports a broken
    // transformer — not every benign "mixin" mention in the loader startup
    // log (reference-map warnings, "Force-disabling mixin", the MIXIN
    // Subsystem banner). Those are normal and must not raise a false hint.
    if lower.contains("mixin")
        && (lower.contains("fail")
            || lower.contains("error")
            || lower.contains("exception")
            || lower.contains("could not")
            || lower.contains("couldn't")
            || lower.contains("conflict")
            || lower.contains("crash")
            || lower.contains("transform")
            || lower.contains("invalid"))
    {
        return Some(CrashSignalKind::Mixin);
    }
    if lower.contains("exception")
        || lower.contains("error:")
        || lower.contains("error ")
        || lower.contains("knot//")
        || lower.contains("net.fabricmc.loader")
        || lower.contains("java.base/")
        || (lower.starts_with("at ") && lower.contains(".java:"))
    {
        return Some(CrashSignalKind::Exception);
    }
    None
}

/// Match a Java package/FQN string (e.g. `net.earthcomputer.clientcommands`)
/// against a mod candidate. A candidate matches when its token equals the
/// package or is the final component of the package (`...clientcommands`).
fn candidate_matches_java_package(candidate: &ModCandidate<'_>, pkg: &str) -> bool {
    // Keep the package dots intact for suffix matching.
    let pkg = pkg.replace('\\', "/");
    candidate.tokens.iter().any(|token| {
        if token.len() < 4 || is_noise_token(token) {
            return false;
        }
        let compact = compact_token(token);
        compact == compact_token(&pkg)
            || pkg.ends_with(&format!("-{compact}"))
            || pkg.ends_with(&format!(".{compact}"))
    })
}

fn candidate_matches_line(candidate: &ModCandidate<'_>, line: &str) -> bool {
    let normalized_line = normalize_token(line);
    let compact_line = compact_token(&normalized_line);
    // Segment match avoids short name tokens like "critters" falsely hitting
    // another mod id such as "crittersandcompanions" via substring contains().
    let line_parts: HashSet<&str> = normalized_line
        .split(['-', '.'])
        .filter(|part| part.len() >= 3)
        .collect();

    candidate.tokens.iter().any(|token| {
        if token.len() < 4 || is_noise_token(token) {
            return false;
        }
        let compact = compact_token(token);
        if normalized_line == *token || compact_line == compact || compact_line == *token {
            return true;
        }
        if line_parts.contains(token.as_str()) || line_parts.contains(compact.as_str()) {
            return true;
        }
        // Long stems / modids (file names, mixin packages) may appear mid-line.
        compact.len() >= 10 && compact_line.contains(&compact)
    })
}

/// Extract Java fully-qualified names (and their package prefixes) from a
/// stack-trace line such as `at knot//net.earthcomputer.clientcommands...
/// .PlayerRandCracker.throwItem(PlayerRandCracker.java:412)`. Each dotted
/// segment becomes a candidate token so a mod whose id/name matches a package
/// component (e.g. `clientcommands`) is correctly attributed.
fn extract_java_packages(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in line.split(|c: char| {
        c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ':' | ';')
    }) {
        // Keep dots/packages intact; only switch Fabric's `knot//` separator to
        // a dot so `net/earthcomputer` style and `knot//net.x` both normalize.
        let unified = raw.replace("knot//", "").replace(['/', '\\'], ".");
        if unified.contains('.') && unified.chars().any(|c| c.is_ascii_alphabetic()) {
            let trimmed = unified.trim_end_matches(".class");
            // Emit every dotted prefix (full FQN down to the top-level package)
            // so `net.earthcomputer.clientcommands` also yields `clientcommands`.
            let mut acc = String::new();
            for seg in trimmed.split('.') {
                if seg.is_empty() {
                    continue;
                }
                acc = if acc.is_empty() {
                    seg.to_string()
                } else {
                    format!("{acc}.{seg}")
                };
                if acc.len() >= 3 {
                    out.push(acc.clone());
                }
            }
        }
    }
    out
}

fn jar_matches_candidate(jar_name: &str, candidate: &ModCandidate<'_>) -> bool {
    let normalized = normalize_token(jar_name.trim_end_matches(".jar"));
    let compact = compact_token(&normalized);
    if candidate.file_stem.as_deref() == Some(normalized.as_str()) {
        return true;
    }
    candidate.tokens.iter().any(|token| {
        if token.len() < 4 || is_noise_token(token) {
            return false;
        }
        let token_compact = compact_token(token);
        normalized == *token
            || compact == token_compact
            || (token_compact.len() >= 10
                && (compact.contains(&token_compact) || token_compact.contains(&compact)))
    })
}

fn add_manifest_suspect(
    suspects: &mut BTreeMap<String, SuspectAccumulator>,
    module: &ModSpec,
    evidence: SuspectEvidence,
    confidence: u8,
) {
    let key = normalize_token(&module.id);
    let src = match_source_for_kind(evidence.kind);
    let entry = suspects.entry(key).or_insert_with(|| SuspectAccumulator {
        id: module.id.clone(),
        name: module.name.clone(),
        version: Some(module.version.clone()),
        file_name: module.file_name.clone(),
        known_in_manifest: true,
        confidence: 0,
        evidence: Vec::new(),
        authors: module.authors.clone(),
        match_sources: Vec::new(),
    });
    if entry.authors.is_empty() && !module.authors.is_empty() {
        entry.authors = module.authors.clone();
    }
    entry.confidence = entry.confidence.max(confidence);
    push_match_source(entry, &src);
    push_evidence(entry, evidence);
}

fn add_inferred_suspect(
    suspects: &mut BTreeMap<String, SuspectAccumulator>,
    id: &str,
    file_name: Option<String>,
    evidence: SuspectEvidence,
    confidence: u8,
) {
    let key = normalize_token(id);
    let src = match_source_for_kind(evidence.kind);
    let entry = suspects.entry(key).or_insert_with(|| SuspectAccumulator {
        id: id.to_string(),
        name: id.to_string(),
        version: None,
        file_name,
        known_in_manifest: false,
        confidence: 0,
        evidence: Vec::new(),
        authors: Vec::new(),
        match_sources: Vec::new(),
    });
    entry.confidence = entry.confidence.max(confidence);
    push_match_source(entry, &src);
    push_evidence(entry, evidence);
}

fn accumulator_to_suspect(acc: SuspectAccumulator) -> SuspectedMod {
    SuspectedMod {
        id: acc.id,
        name: acc.name,
        version: acc.version,
        file_name: acc.file_name,
        known_in_manifest: acc.known_in_manifest,
        confidence: acc.confidence.min(99),
        evidence: acc.evidence,
        authors: acc.authors,
        blame_role: BlameRole::Related,
        match_sources: acc.match_sources,
    }
}

fn match_source_for_kind(kind: CrashSignalKind) -> String {
    match kind {
        CrashSignalKind::SuspectedMods => "suspected_mods_line".into(),
        CrashSignalKind::Entrypoint => "entrypoint".into(),
        CrashSignalKind::ModFile => "mod_file".into(),
        CrashSignalKind::Mixin => "mixin".into(),
        CrashSignalKind::Exception | CrashSignalKind::CausedBy => "exception".into(),
        CrashSignalKind::MissingDependency => "missing_dependency".into(),
        CrashSignalKind::ModVersionMismatch => "version_mismatch".into(),
        CrashSignalKind::LoaderMismatch
        | CrashSignalKind::WrongLoader
        | CrashSignalKind::LoaderVersionMismatch => "loader".into(),
        _ => "signal".into(),
    }
}

fn push_match_source(entry: &mut SuspectAccumulator, source: &str) {
    if source.is_empty() {
        return;
    }
    if !entry.match_sources.iter().any(|s| s == source) {
        entry.match_sources.push(source.to_string());
    }
}

/// Independent high-value channels used to promote primary blame.
fn is_strong_match_source(source: &str) -> bool {
    matches!(
        source,
        "suspected_mods_line"
            | "entrypoint"
            | "mod_file"
            | "class_in_jar"
            | "mixin"
            | "package"
    )
}

fn assign_blame_roles(suspects: &mut [SuspectedMod]) {
    for s in suspects.iter_mut() {
        let strong = s
            .match_sources
            .iter()
            .filter(|src| is_strong_match_source(src))
            .count();
        // Multi-signal agreement → primary; single strong → secondary; else related.
        if strong >= 2 || (s.confidence >= 92 && strong >= 1) {
            s.blame_role = BlameRole::Primary;
            s.confidence = s.confidence.saturating_add(4).min(99);
        } else if strong == 1 || s.confidence >= 75 {
            s.blame_role = BlameRole::Secondary;
        } else {
            s.blame_role = BlameRole::Related;
        }
    }
    // Keep ranking: primary first, then confidence.
    suspects.sort_by(|a, b| {
        blame_rank(b.blame_role)
            .cmp(&blame_rank(a.blame_role))
            .then_with(|| b.confidence.cmp(&a.confidence))
            .then_with(|| a.name.cmp(&b.name))
    });
}

fn blame_rank(role: BlameRole) -> u8 {
    match role {
        BlameRole::Primary => 3,
        BlameRole::Secondary => 2,
        BlameRole::Related => 1,
    }
}

/// Post-merge enrichment: crash-report mod list, authors from jars, class→jar blame.
fn enrich_diagnosis_suspects(
    project_dir: &Path,
    manifest: &ProjectManifest,
    selected_report: &Option<CrashReportAnalysis>,
    latest_log: &LatestLogAnalysis,
    mut suspects: Vec<SuspectedMod>,
) -> Vec<SuspectedMod> {
    // 1) Force high confidence for Fabric "Suspected mods" / report mod entries.
    if let Some(report) = selected_report {
        for signal in report.signals.iter().filter(|s| s.kind == CrashSignalKind::SuspectedMods) {
            for token in tokenize(&signal.text) {
                if token.len() < 2 || is_noise_token(&token) {
                    continue;
                }
                if let Some(module) = manifest.mods.iter().find(|m| {
                    normalize_token(&m.id) == token
                        || compact_token(&normalize_token(&m.id)) == compact_token(&token)
                        || normalize_token(&m.name) == token
                }) {
                    boost_or_insert_suspect(
                        &mut suspects,
                        module,
                        SuspectEvidence {
                            source: signal.source.clone(),
                            line_number: signal.line_number,
                            kind: CrashSignalKind::SuspectedMods,
                            text: signal.text.clone(),
                        },
                        97,
                        "suspected_mods_line",
                    );
                } else if let Some(entry) = report.mod_entries.iter().find(|e| {
                    normalize_token(&e.id) == token
                        || e.name
                            .as_ref()
                            .map(|n| normalize_token(n) == token)
                            .unwrap_or(false)
                }) {
                    let mut inferred = SuspectedMod {
                        id: entry.id.clone(),
                        name: entry.name.clone().unwrap_or_else(|| entry.id.clone()),
                        version: entry.version.clone(),
                        file_name: None,
                        known_in_manifest: false,
                        confidence: 90,
                        evidence: vec![SuspectEvidence {
                            source: signal.source.clone(),
                            line_number: signal.line_number,
                            kind: CrashSignalKind::SuspectedMods,
                            text: signal.text.clone(),
                        }],
                        authors: Vec::new(),
                        blame_role: BlameRole::Related,
                        match_sources: vec!["suspected_mods_line".into()],
                    };
                    if let Some(module) = manifest.mods.iter().find(|m| {
                        normalize_token(&m.id) == normalize_token(&entry.id)
                    }) {
                        inferred.id = module.id.clone();
                        inferred.name = module.name.clone();
                        inferred.version = Some(module.version.clone());
                        inferred.file_name = module.file_name.clone();
                        inferred.known_in_manifest = true;
                        inferred.authors = module.authors.clone();
                        inferred.confidence = 97;
                    }
                    suspects = merge_suspected_mods(
                        suspects.into_iter().chain(std::iter::once(inferred)),
                    );
                }
            }
        }
    }

    // 2) Fill authors from manifest / jar metadata.
    let mods_dir = project_dir.join("mods");
    for s in &mut suspects {
        if !s.authors.is_empty() {
            continue;
        }
        if let Some(module) = manifest.mods.iter().find(|m| {
            normalize_token(&m.id) == normalize_token(&s.id)
                || m.file_name
                    .as_ref()
                    .zip(s.file_name.as_ref())
                    .map(|(a, b)| a.eq_ignore_ascii_case(b))
                    .unwrap_or(false)
        }) {
            if !module.authors.is_empty() {
                s.authors = module.authors.clone();
            } else if let Some(file) = module.file_name.as_ref() {
                let jar = mods_dir.join(file);
                if let Ok(meta) = crate::mod_scan::scan_mod_jar(&jar) {
                    s.authors = meta.authors;
                    if s.file_name.is_none() {
                        s.file_name = Some(file.clone());
                    }
                }
            }
        } else if let Some(file) = s.file_name.as_ref() {
            let jar = mods_dir.join(file);
            if let Ok(meta) = crate::mod_scan::scan_mod_jar(&jar) {
                s.authors = meta.authors;
            }
        }
    }

    // 3) Class → jar → modid attribution.
    let mut haystack = String::new();
    if let Some(report) = selected_report {
        haystack.push_str(&report.content);
        haystack.push('\n');
    }
    haystack.push_str(&latest_log.tail);
    let class_names = crate::crash_assistant::extract_blame_class_names(&haystack, 8);
    if mods_dir.is_dir() && !class_names.is_empty() {
        for class_name in class_names {
            let matches = crate::crash_assistant::find_class_in_mods(&class_name, &mods_dir);
            for hit in matches {
                if hit.mod_id == "?" {
                    continue;
                }
                let evidence = SuspectEvidence {
                    source: "class-finder".into(),
                    line_number: 0,
                    kind: CrashSignalKind::Exception,
                    text: format!("{} provided by {}", hit.class_name, hit.file_name.as_deref().unwrap_or(&hit.mod_id)),
                };
                if let Some(module) = manifest.mods.iter().find(|m| {
                    normalize_token(&m.id) == normalize_token(&hit.mod_id)
                        || m.file_name
                            .as_ref()
                            .zip(hit.file_name.as_ref())
                            .map(|(a, b)| a.eq_ignore_ascii_case(b))
                            .unwrap_or(false)
                }) {
                    boost_or_insert_suspect(
                        &mut suspects,
                        module,
                        evidence,
                        93,
                        "class_in_jar",
                    );
                } else {
                    let inferred = SuspectedMod {
                        id: hit.mod_id.clone(),
                        name: hit.mod_name.clone(),
                        version: None,
                        file_name: hit.file_name.clone(),
                        known_in_manifest: false,
                        confidence: 88,
                        evidence: vec![evidence],
                        authors: hit
                            .file_name
                            .as_ref()
                            .and_then(|f| crate::mod_scan::scan_mod_jar(&mods_dir.join(f)).ok())
                            .map(|m| m.authors)
                            .unwrap_or_default(),
                        blame_role: BlameRole::Related,
                        match_sources: vec!["class_in_jar".into()],
                    };
                    suspects = merge_suspected_mods(
                        suspects.into_iter().chain(std::iter::once(inferred)),
                    );
                }
            }
        }
    }

    assign_blame_roles(&mut suspects);
    suspects
}

fn boost_or_insert_suspect(
    suspects: &mut Vec<SuspectedMod>,
    module: &ModSpec,
    evidence: SuspectEvidence,
    confidence: u8,
    match_source: &str,
) {
    let key = compact_token(&normalize_token(&module.id));
    if let Some(existing) = suspects
        .iter_mut()
        .find(|s| compact_token(&normalize_token(&s.id)) == key)
    {
        existing.confidence = existing.confidence.max(confidence);
        existing.known_in_manifest = true;
        existing.id = module.id.clone();
        existing.name = module.name.clone();
        existing.version = Some(module.version.clone());
        if existing.file_name.is_none() {
            existing.file_name = module.file_name.clone();
        }
        if existing.authors.is_empty() {
            existing.authors = module.authors.clone();
        }
        if !existing.match_sources.iter().any(|s| s == match_source) {
            existing.match_sources.push(match_source.to_string());
        }
        if existing.evidence.len() < MAX_EVIDENCE_PER_SUSPECT
            && !existing.evidence.iter().any(|e| {
                e.source == evidence.source && e.line_number == evidence.line_number
            })
        {
            existing.evidence.push(evidence);
        }
    } else {
        suspects.push(SuspectedMod {
            id: module.id.clone(),
            name: module.name.clone(),
            version: Some(module.version.clone()),
            file_name: module.file_name.clone(),
            known_in_manifest: true,
            confidence,
            evidence: vec![evidence],
            authors: module.authors.clone(),
            blame_role: BlameRole::Related,
            match_sources: vec![match_source.to_string()],
        });
    }
}

fn push_evidence(entry: &mut SuspectAccumulator, evidence: SuspectEvidence) {
    if entry.evidence.len() >= MAX_EVIDENCE_PER_SUSPECT {
        return;
    }
    if !entry
        .evidence
        .iter()
        .any(|item| item.source == evidence.source && item.line_number == evidence.line_number)
    {
        entry.evidence.push(evidence);
    }
}

fn evidence(
    source: &str,
    line_number: usize,
    kind: CrashSignalKind,
    line: &str,
) -> SuspectEvidence {
    SuspectEvidence {
        source: source.to_string(),
        line_number,
        kind,
        text: line.trim().to_string(),
    }
}

fn confidence_for_kind(kind: CrashSignalKind) -> u8 {
    match kind {
        CrashSignalKind::SuspectedMods => 95,
        CrashSignalKind::ModFile => 88,
        CrashSignalKind::Entrypoint => 96,
        CrashSignalKind::MissingDependency => 92,
        CrashSignalKind::LoaderMismatch => 86,
        CrashSignalKind::ModVersionMismatch => 90,
        CrashSignalKind::MinecraftVersionMismatch => 90,
        CrashSignalKind::WrongLoader => 90,
        CrashSignalKind::LoaderVersionMismatch => 88,
        CrashSignalKind::OutOfMemory => 92,
        CrashSignalKind::Watchdog => 90,
        CrashSignalKind::PortConflict => 90,
        CrashSignalKind::EulaNotAccepted => 96,
        CrashSignalKind::CorruptJar => 94,
        CrashSignalKind::DuplicateMod => 92,
        CrashSignalKind::JavaVersion => 94,
        CrashSignalKind::TickingEntity => 84,
        CrashSignalKind::SideMismatch => 92,
        CrashSignalKind::ServerState => 80,
        CrashSignalKind::Mixin => 78,
        CrashSignalKind::CausedBy => 66,
        CrashSignalKind::OpenGl => 58,
        CrashSignalKind::Exception => 48,
        CrashSignalKind::ResourceWarning => 35,
        CrashSignalKind::Performance => 25,
    }
}

fn extract_quoted_mod_ids(line: &str) -> Vec<String> {
    let lower = line.to_lowercase();
    let mut ids = Vec::new();
    if let Some(pos) = lower.find("provided by '") {
        let start = pos + "provided by '".len();
        if let Some(end) = line[start..].find('\'') {
            let token = normalize_token(&line[start..start + end]);
            if !token.is_empty() {
                ids.push(token);
            }
        }
    }
    ids
}

/// Extract mod identifiers named explicitly inside loader resolution errors,
/// e.g. `Mod 'Client Commands' (clientcommands) requires ...` or
/// `'fabricloader' (fabricloader) 0.x` or `mod fabric-api (fabric-api)`.
fn extract_named_mods(line: &str) -> Vec<String> {
    let mut ids = Vec::new();
    // Pattern: Mod 'Display Name' (modid)  /  'modid' (modid)
    let mut rest = line;
    while let Some(pos) = rest.find('\'') {
        let after_open = &rest[pos + 1..];
        if let Some(end) = after_open.find('\'') {
            let inner = &after_open[..end];
            // Look ahead for `(modid)` immediately after the closing quote.
            let tail = &after_open[end + 1..];
            if let Some(open) = tail.find('(') {
                if let Some(close) = tail[open..].find(')') {
                    let id = normalize_token(&tail[open + 1..open + close]);
                    if !id.is_empty() {
                        ids.push(id);
                    }
                }
            }
            ids.push(normalize_token(inner));
            rest = &after_open[end + 1..];
        } else {
            break;
        }
    }
    // Pattern: `mod <id> (` without quotes.
    for cap in line.match_indices("mod ") {
        let tail = &line[cap.0 + 4..];
        if let Some(open) = tail.find('(') {
            let id = normalize_token(&tail[..open]);
            if !id.is_empty() && id.len() >= 3 {
                ids.push(id);
            }
        }
    }
    // Pattern: `modid (incompatible)` / `modid (disabled)` in resource-pack or
    // mod lists (no quotes, no `mod ` prefix).
    for cap in line.match_indices('(') {
        let before = &line[..cap.0];
        let after = &line[cap.0 + 1..];
        let close = match after.find(')') {
            Some(c) => c,
            None => continue,
        };
        let reason = &after[..close];
        if reason == "incompatible" || reason == "disabled" || reason.contains("incompatible") {
            let id = normalize_token(before.trim_end().split(',').last().unwrap_or(before).trim());
            if !id.is_empty() && id.len() >= 3 {
                ids.push(id);
            }
        }
    }
    ids.retain(|id| !is_noise_token(id) && id.len() >= 3);
    ids
}

fn extract_jar_names(line: &str) -> Vec<String> {
    let mut jars = Vec::new();
    for raw in line.split(|c: char| {
        c.is_whitespace() || matches!(c, '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';')
    }) {
        let trimmed = raw.trim_matches(|c: char| matches!(c, ':' | ',' | ';'));
        let lower = trimmed.to_lowercase();
        if let Some(idx) = lower.find(".jar") {
            let before = &trimmed[..idx + 4];
            let name = before
                .rsplit(|c| c == '/' || c == '\\')
                .next()
                .unwrap_or(before)
                .to_string();
            if !name.is_empty() && !jars.contains(&name) {
                jars.push(name);
            }
        }
    }
    jars
}

fn infer_id_from_jar(jar_name: &str) -> String {
    let stem = jar_name.trim_end_matches(".jar");
    let mut parts = Vec::new();
    for part in stem.split(|c| c == '-' || c == '_' || c == '+') {
        let part = part.trim();
        if part.is_empty() || looks_like_version_token(part) || part.starts_with("mc") {
            break;
        }
        parts.push(part);
    }
    if parts.is_empty() {
        normalize_token(stem)
    } else {
        normalize_token(&parts.join("-"))
    }
}

fn looks_like_version_token(token: &str) -> bool {
    let lower = token.to_lowercase();
    lower
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
        || matches!(
            lower.as_str(),
            "fabric" | "forge" | "neoforge" | "quilt" | "common" | "client"
        )
}

fn tokenize(value: &str) -> Vec<String> {
    value
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
        .map(normalize_token)
        .filter(|token| !token.is_empty())
        .collect()
}

fn normalize_token(value: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;
    for ch in value.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            previous_dash = false;
        } else if ch == '_' || ch == '-' || ch == '.' || ch == ' ' {
            if !previous_dash && !out.is_empty() {
                out.push('-');
                previous_dash = true;
            }
        }
    }
    out.trim_matches('-').to_string()
}

fn compact_token(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect()
}

fn is_noise_token(token: &str) -> bool {
    matches!(
        token,
        "jar"
            | "mods"
            | "mod"
            | "file"
            | "minecraft"
            | "fabric"
            | "forge"
            | "neoforge"
            | "quilt"
            | "java"
            | "exception"
            | "error"
            | "mixin"
            | "mixins"
            | "caused"
            | "client"
            | "server"
            | "common"
            | "unknown"
            | "null"
    )
}

fn validate_report_id(report_id: &str) -> Result<PathBuf, CrashError> {
    let relative = PathBuf::from(report_id);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(CrashError::InvalidReportPath(report_id.to_string()));
    }
    let normalized = report_id.replace('\\', "/");
    if !normalized.starts_with("crash-reports/") || !normalized.to_lowercase().ends_with(".txt") {
        return Err(CrashError::InvalidReportPath(report_id.to_string()));
    }
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{
        LoaderKind, LoaderSpec, MinecraftSpec, ModSource, ProjectManifest, ProjectMetadata, Side,
        SourceKind,
    };

    fn manifest() -> ProjectManifest {
        ProjectManifest {
            schema_version: "0.1.0".to_string(),
            project: ProjectMetadata {
                id: "test".to_string(),
                name: "Test".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                authors: Vec::new(),
            },
            minecraft: MinecraftSpec {
                version: "1.20.1".to_string(),
            },
            loader: LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.15.0".to_string(),
            },
            brief: None,
            java: None,
            profiles: Vec::new(),
            mods: vec![ModSpec {
                id: "sodium".to_string(),
                name: "Sodium".to_string(),
                source: ModSource {
                    kind: SourceKind::Modrinth,
                    project_id: Some("AANobbMI".to_string()),
                    file_id: None,
                    url: None,
                    path: None,
                    icon_url: None,
                    categories: Vec::new(),
                },
                version: "0.5.8".to_string(),
                file_name: Some("sodium-fabric-mc1.20.1-0.5.8.jar".to_string()),
                hashes: None,
                side: Side::Client,
                dependencies: Vec::new(),
                status: Vec::new(),
                content_type: crate::manifest::ContentType::Mod,
                authors: Vec::new(),
            option: None,
            }],
            overrides: None,
        }
    }

    #[test]
    fn detects_mod_file_suspect_from_crash_report() {
        let text = "Mod File: /instance/mods/sodium-fabric-mc1.20.1-0.5.8.jar\nCaused by: java.lang.IllegalStateException";
        let (_signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest());
        assert_eq!(suspects[0].id, "sodium");
        assert!(suspects[0].confidence >= 88);
    }

    #[test]
    fn detects_mixin_suspect_by_mod_id() {
        let text = "Mixin apply failed sodium.mixins.json:features.render.MixinWorldRenderer -> net.minecraft.WorldRenderer";
        let (_signals, suspects) = analyze_text_for_suspects(text, "logs/latest.log", &manifest());
        assert_eq!(suspects[0].id, "sodium");
    }

    #[test]
    fn resolves_compact_provided_by_id_to_installed_mod() {
        let mut manifest = manifest();
        manifest.mods.push(ModSpec {
            id: "critters-and-companions".to_string(),
            name: "Critters and Companions".to_string(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("critters-and-companions".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "2.1.0".to_string(),
            file_name: Some("crittersandcompanions-fabric-2.1.0.jar".to_string()),
            hashes: None,
            side: Side::Both,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        // A different "critters*" mod must not steal the provided-by match via
        // the shared short name token "critters".
        manifest.mods.push(ModSpec {
            id: "cosy-critters".to_string(),
            name: "Cosy Critters & Creepy Crawlies".to_string(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("cosy-critters".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "0.1.2".to_string(),
            file_name: Some("cosycritters-0.1.2+1.21.1-fabric.jar".to_string()),
            hashes: None,
            side: Side::Client,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        let text = "Could not execute entrypoint stage 'main' due to errors, provided by 'crittersandcompanions'!";

        let (signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest);

        assert_eq!(signals[0].kind, CrashSignalKind::Entrypoint);
        assert_eq!(suspects[0].id, "critters-and-companions");
        assert_eq!(suspects[0].name, "Critters and Companions");
        assert!(suspects[0].known_in_manifest);
        assert!(suspects[0].confidence >= 96);
        assert!(
            !suspects.iter().any(|s| s.id == "cosy-critters"),
            "Cosy Critters should not match crittersandcompanions via substring 'critters'"
        );
    }

    #[test]
    fn ignores_benign_provided_by_without_failure() {
        let mut manifest = manifest();
        manifest.mods.push(ModSpec {
            id: "critters-and-companions".to_string(),
            name: "Critters and Companions".to_string(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("critters-and-companions".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "2.1.0".to_string(),
            file_name: Some("crittersandcompanions-fabric-2.1.0.jar".to_string()),
            hashes: None,
            side: Side::Both,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        let text = "[Fabric] Loading 120 mods:\n\t- crittersandcompanions 2.1.0 provided by 'crittersandcompanions'\nDone.";
        let (signals, suspects) =
            analyze_text_for_suspects(text, "logs/latest.log", &manifest);
        assert!(
            !signals.iter().any(|s| s.kind == CrashSignalKind::Entrypoint),
            "benign 'provided by' must not be Entrypoint"
        );
        assert!(
            suspects.is_empty(),
            "healthy log must not suspect Critters: {:?}",
            suspects.iter().map(|s| &s.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn merge_upgrades_inferred_provider_to_manifest_mod() {
        let manifest = manifest();
        let inferred = SuspectedMod {
            id: "s-o-d-i-u-m".to_string(),
            name: "s-o-d-i-u-m".to_string(),
            version: None,
            file_name: None,
            known_in_manifest: false,
            confidence: 70,
            evidence: Vec::new(),
            authors: Vec::new(),
            blame_role: BlameRole::Related,
            match_sources: Vec::new(),
        };
        let resolved = SuspectedMod {
            id: "sodium".to_string(),
            name: manifest.mods[0].name.clone(),
            version: Some(manifest.mods[0].version.clone()),
            file_name: manifest.mods[0].file_name.clone(),
            known_in_manifest: true,
            confidence: 96,
            evidence: Vec::new(),
            authors: Vec::new(),
            blame_role: BlameRole::Related,
            match_sources: Vec::new(),
        };

        let suspects = merge_suspected_mods([inferred, resolved]);

        assert_eq!(suspects.len(), 1);
        assert_eq!(suspects[0].id, "sodium");
        assert_eq!(suspects[0].name, "Sodium");
        assert!(suspects[0].known_in_manifest);
    }

    #[test]
    fn detects_opengl_debug_as_render_signal() {
        let text = "OpenGL debug message: id=1282, source=API, type=ERROR, severity=HIGH, message='GL_INVALID_OPERATION error generated. No active program.'";
        let (signals, suspects) = analyze_text_for_suspects(text, "logs/latest.log", &manifest());
        assert_eq!(signals[0].kind, CrashSignalKind::OpenGl);
        assert!(suspects.is_empty());
    }

    #[test]
    fn detects_missing_dependency_with_named_mod() {
        let mut manifest = manifest();
        manifest.mods.push(ModSpec {
            id: "lithium".to_string(),
            name: "Lithium".to_string(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("lithium".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "0.11.0".to_string(),
            file_name: Some("lithium-fabric-0.11.0.jar".to_string()),
            hashes: None,
            side: Side::Client,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        // Real Fabric loader resolution error format.
        let text = "net.fabricmc.loader.impl.discovery.ModResolutionException: Mod 'Lithium' (lithium) requires version 1.0.0 or later of mod 'jellysquid3's sodium' (sodium), which is missing!";
        let (signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest);
        assert_eq!(signals[0].kind, CrashSignalKind::MissingDependency);
        assert!(suspects.iter().any(|s| s.id == "lithium"));
        assert!(suspects.iter().any(|s| s.id == "sodium"));
    }

    #[test]
    fn detects_wrong_minecraft_version_for_mod() {
        let mut manifest = manifest();
        manifest.mods.push(ModSpec {
            id: "iris".to_string(),
            name: "Iris".to_string(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("iris".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "1.7.0".to_string(),
            file_name: Some("iris-1.7.0.jar".to_string()),
            hashes: None,
            side: Side::Client,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        let text = "Incompatible mod set!\nMod 'Iris' (iris) requires version 1.21.4 or later of 'Minecraft' (minecraft), but a non-matching version 1.20.1 is present!";
        let (signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest);
        assert!(
            signals
                .iter()
                .any(|s| s.kind == CrashSignalKind::MinecraftVersionMismatch),
            "expected a MinecraftVersionMismatch signal, got {:?}",
            signals
        );
        assert!(suspects.iter().any(|s| s.id == "iris"));
        assert_eq!(suspects[0].confidence, 90);
    }

    #[test]
    fn detects_wrong_loader_for_mod() {
        let mut manifest = manifest();
        manifest.mods.push(ModSpec {
            id: "create".to_string(),
            name: "Create".to_string(),
            source: ModSource {
                kind: SourceKind::Curseforge,
                project_id: Some("create".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "0.5.1".to_string(),
            file_name: Some("create-1.20.1-0.5.1.jar".to_string()),
            hashes: None,
            side: Side::Both,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        let text = "Mod 'Create' (create) requires the Forge mod loader, but Fabric Loader 0.15.0 is in use!";
        let (signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest);
        assert_eq!(signals[0].kind, CrashSignalKind::WrongLoader);
        assert!(suspects.iter().any(|s| s.id == "create"));
    }

    #[test]
    fn detects_wrong_mod_version_conflict() {
        let mut manifest = manifest();
        manifest.mods.push(ModSpec {
            id: "sodium".to_string(),
            name: "Sodium".to_string(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("AANobbMI".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "0.6.0".to_string(),
            file_name: Some("sodium-fabric-0.6.0.jar".to_string()),
            hashes: None,
            side: Side::Client,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        let text = "Mod 'Reese's Sodium Options' (reeses-sodium-options) 1.8.0 conflicts with 'Sodium' (sodium) 0.6.0 (incompatible).";
        let (signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest);
        assert_eq!(signals[0].kind, CrashSignalKind::ModVersionMismatch);
        assert!(suspects.iter().any(|s| s.id == "sodium"));
    }

    #[test]
    fn detects_loader_version_mismatch() {
        let mut manifest = manifest();
        manifest
            .mods
            .push(ModSpec {
                id: "fabric-api".to_string(),
                name: "Fabric API".to_string(),
                source: ModSource {
                    kind: SourceKind::Modrinth,
                    project_id: Some("fabric-api".to_string()),
                    file_id: None,
                    url: None,
                    path: None,
                    icon_url: None,
                    categories: Vec::new(),
                },
                version: "0.92.0".to_string(),
                file_name: Some("fabric-api-0.92.0.jar".to_string()),
                hashes: None,
                side: Side::Both,
                dependencies: Vec::new(),
                status: Vec::new(),
                content_type: crate::manifest::ContentType::Mod,
                authors: Vec::new(),
            option: None,
            });
        let text = "Mod 'Fabric API' (fabric-api) requires Fabric Loader 0.16.0 or later, but 0.15.0 is present!";
        let (signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest);
        assert_eq!(signals[0].kind, CrashSignalKind::LoaderVersionMismatch);
        assert!(suspects.iter().any(|s| s.id == "fabric-api"));
    }

    #[test]
    fn detects_stacktrace_mod_by_java_package() {
        // Real crash from Fabulously Optimized: clientcommands NPE in tick task.
        let mut manifest = manifest();
        manifest.mods.push(ModSpec {
            id: "clientcommands".to_string(),
            name: "Client Commands".to_string(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("clientcommands".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "2.9.11".to_string(),
            file_name: Some("clientcommands-2.9.11.jar".to_string()),
            hashes: None,
            side: Side::Client,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        let text = "java.lang.NullPointerException: Cannot read field \"field_7512\" because \"player\" is null\n\tat knot//net.earthcomputer.clientcommands.features.PlayerRandCracker.throwItem(PlayerRandCracker.java:412)";
        let (_signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest);
        assert!(suspects.iter().any(|s| s.id == "clientcommands"));
    }

    #[test]
    fn detects_clientcommands_from_real_fo_crash() {
        // Real fragment from Fabulously Optimized 8.1.0 crash-2025-09-20:
        // the stack trace names `net.earthcomputer.clientcommands`, and the
        // resource-pack list flags `clientcommands (incompatible)`.
        let mut manifest = manifest();
        manifest.mods.push(ModSpec {
            id: "clientcommands".to_string(),
            name: "Client Commands".to_string(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("clientcommands".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "2.9.11".to_string(),
            file_name: Some("clientcommands-2.9.11.jar".to_string()),
            hashes: None,
            side: Side::Client,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        let text = "\
java.lang.NullPointerException: Cannot read field \"field_7512\" because \"player\" is null
	at knot//net.earthcomputer.clientcommands.features.PlayerRandCracker.throwItem(PlayerRandCracker.java:412)
	at knot//net.earthcomputer.clientcommands.task.ItemThrowTask.onTick(ItemThrowTask.java:60)
Mixins in Stacktrace:
	net.minecraft.class_310:
		net.earthcomputer.clientcommands.mixin.events.MinecraftMixin (mixins.clientcommands.json)
Resource Packs: vanilla, fabric, animatica, antip2w, betterconfig, clientcommands (incompatible), cloth-config";
        let (signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest);
        assert!(
            suspects.iter().any(|s| s.id == "clientcommands"),
            "clientcommands should be attributed via Java package / mixin / incompatible marker"
        );
        // The stack-trace lines must carry a high-confidence signal.
        assert!(
            signals
                .iter()
                .any(|s| s.kind == CrashSignalKind::Exception || s.kind == CrashSignalKind::Mixin)
        );
    }

    #[test]
    fn detects_incompatible_marker_in_resource_pack_list() {
        let mut manifest = manifest();
        manifest.mods.push(ModSpec {
            id: "clientcommands".to_string(),
            name: "Client Commands".to_string(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("clientcommands".to_string()),
                file_id: None,
                url: None,
                path: None,
                icon_url: None,
                categories: Vec::new(),
            },
            version: "2.9.11".to_string(),
            file_name: Some("clientcommands-2.9.11.jar".to_string()),
            hashes: None,
            side: Side::Client,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
            authors: Vec::new(),
        option: None,
        });
        let text = "Resource Packs: vanilla, fabric, animatica, antip2w, betterconfig, clientcommands (incompatible), cloth-config";
        let (signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest);
        assert!(
            suspects.iter().any(|s| s.id == "clientcommands"),
            "clientcommands should be attributed via the (incompatible) marker"
        );
        assert!(
            signals
                .iter()
                .any(|s| s.kind == CrashSignalKind::ModVersionMismatch)
        );
    }

    #[test]
    fn validates_report_path() {
        assert!(validate_report_id("crash-reports/crash.txt").is_ok());
        assert!(validate_report_id("../crash.txt").is_err());
        assert!(validate_report_id("logs/latest.log").is_err());
    }

    #[test]
    fn parses_forge_sections_without_vanilla_sections_or_duplicates() {
        let text = "\
Forge Mod List:
| ID | Name | Version |
| sodium | Sodium | 0.5.8 |
-- System Details --
Memory: 2048 MB / 4096 MB
CPU: 8x Example CPU
JVM Flags:
-Xmx4G";

        let sections = parse_crash_sections(text);
        let titles = sections
            .iter()
            .map(|section| section.title.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            titles,
            vec!["Forge Mod List", "System Details", "JVM Flags"]
        );
        assert_eq!(
            titles
                .iter()
                .filter(|title| **title == "Forge Mod List")
                .count(),
            1
        );
    }

    #[test]
    fn parses_standalone_forge_heading_without_vanilla_start() {
        let sections = parse_crash_sections("preamble\nNeoForge Mod List:\nexamplemod (1.0)");

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "NeoForge Mod List");
        assert_eq!(sections[0].start_line, 2);
        assert!(sections[0].preview.contains("examplemod"));
    }

    #[test]
    fn diagnosis_does_not_create_missing_log_files() {
        let dir = tempfile::tempdir().unwrap();

        let diagnosis = build_crash_diagnosis(dir.path(), &manifest(), None, Vec::new()).unwrap();

        assert!(!diagnosis.latest_log.exists);
        assert!(!diagnosis.launcher_log.exists);
        assert!(!dir.path().join("logs").exists());
        assert!(!diagnosis.latest_log.path.exists());
        assert!(!diagnosis.launcher_log.path.exists());
    }

    #[test]
    fn newer_healthy_latest_log_skips_stale_crash_report() {
        let dir = tempfile::tempdir().unwrap();
        let crash_dir = dir.path().join("crash-reports");
        let logs_dir = dir.path().join("logs");
        fs::create_dir_all(&crash_dir).unwrap();
        fs::create_dir_all(&logs_dir).unwrap();

        let crash_path = crash_dir.join("crash-2020-01-01_00.00.00-client.txt");
        fs::write(
            &crash_path,
            "---- Minecraft Crash Report ----\njava.lang.NullPointerException: old crash\n\tat bad.Mod.init(Mod.java:1)\n",
        )
        .unwrap();
        // Ensure distinct mtimes on filesystems with coarse resolution.
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let latest = logs_dir.join("latest.log");
        fs::write(
            &latest,
            "[Render thread/INFO]: Sound engine started\n[Render thread/INFO]: Created: 1024x512x4 minecraft:textures/atlas/blocks.png-atlas\n[Render thread/INFO]: Reloading ResourceManager: Default\n",
        )
        .unwrap();

        let diagnosis = build_crash_diagnosis(dir.path(), &manifest(), None, Vec::new()).unwrap();
        assert!(diagnosis.crash_report_stale, "expected stale flag");
        assert!(
            diagnosis.selected_report.is_none(),
            "should not auto-select old crash"
        );
        assert_eq!(diagnosis.analysis_source, "latest_log");
        assert!(
            diagnosis.session_healthy,
            "healthy live log must set session_healthy"
        );
        assert!(
            diagnosis.fix_plan.actions.is_empty(),
            "healthy session must not propose crash-log fixes"
        );
        assert!(
            diagnosis.suspected_mods.is_empty(),
            "healthy session must not keep crash suspects"
        );
        assert!(latest_log_supersedes_crash(
            dir.path(),
            Some(crash_path.as_path()),
            &diagnosis.latest_log.tail
        ));
    }

    #[test]
    fn explicit_report_id_still_loads_stale_crash() {
        let dir = tempfile::tempdir().unwrap();
        let crash_dir = dir.path().join("crash-reports");
        let logs_dir = dir.path().join("logs");
        fs::create_dir_all(&crash_dir).unwrap();
        fs::create_dir_all(&logs_dir).unwrap();

        let name = "crash-2020-01-01_00.00.00-client.txt";
        let crash_path = crash_dir.join(name);
        fs::write(
            &crash_path,
            "---- Minecraft Crash Report ----\njava.lang.NullPointerException: old crash\n",
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(
            logs_dir.join("latest.log"),
            "[Render thread/INFO]: Sound engine started\n",
        )
        .unwrap();

        let id = format!("crash-reports/{name}");
        let diagnosis =
            build_crash_diagnosis(dir.path(), &manifest(), Some(&id), Vec::new()).unwrap();
        assert!(diagnosis.selected_report.is_some());
        assert_eq!(diagnosis.analysis_source, "crash_report");
        assert!(!diagnosis.crash_report_stale);
    }

    #[test]
    fn latest_compatible_fix_target_resolves_to_automatic_selection() {
        assert_eq!(
            resolve_update_target_version(LATEST_COMPATIBLE_VERSION),
            None
        );
        assert_eq!(resolve_update_target_version("  "), None);
        assert_eq!(
            resolve_update_target_version("version-id"),
            Some("version-id")
        );

        let manifest = manifest();
        let graph = DependencyGraph::from_manifest(&manifest);
        let suspect = SuspectedMod {
            id: "sodium".to_string(),
            name: "Sodium".to_string(),
            version: Some("0.5.8".to_string()),
            file_name: manifest.mods[0].file_name.clone(),
            known_in_manifest: true,
            confidence: 96,
            evidence: Vec::new(),
            authors: Vec::new(),
            blame_role: BlameRole::Related,
            match_sources: Vec::new(),
        };
        let plan = create_crash_fix_plan(&graph, &[], &[suspect], &[]);

        let update = plan
            .actions
            .iter()
            .find_map(|action| match action {
                ChangeAction::UpdateMod { target_version, .. } => Some(target_version),
                _ => None,
            })
            .expect("expected update action");
        assert_eq!(update, LATEST_COMPATIBLE_VERSION);
        assert_eq!(resolve_update_target_version(update), None);
    }

    #[test]
    fn suspected_mods_line_and_mod_file_become_primary() {
        let text = "\
---- Minecraft Crash Report ----
Suspected Mods: sodium
Mod File: /instance/mods/sodium-fabric-mc1.20.1-0.5.8.jar
Caused by: java.lang.IllegalStateException: boom
";
        let (_signals, suspects) =
            analyze_text_for_suspects(text, "crash-reports/latest.txt", &manifest());
        let sodium = suspects.iter().find(|s| s.id == "sodium").expect("sodium");
        assert!(sodium.confidence >= 90);
        assert!(
            sodium.match_sources.iter().any(|s| s == "suspected_mods_line")
                || sodium.match_sources.iter().any(|s| s == "mod_file")
        );
        // Multi-signal → primary after merge/assign.
        assert_eq!(sodium.blame_role, BlameRole::Primary);
    }

    #[test]
    fn mod_spec_authors_serde_default() {
        let json = r#"{
            "id":"x","name":"X","source":{"type":"modrinth"},
            "version":"1","side":"both"
        }"#;
        let m: ModSpec = serde_json::from_str(json).unwrap();
        assert!(m.authors.is_empty());
    }
}

