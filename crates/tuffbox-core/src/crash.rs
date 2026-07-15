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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashDiagnosis {
    pub reports: Vec<CrashReportSummary>,
    pub selected_report: Option<CrashReportAnalysis>,
    pub latest_log: LatestLogAnalysis,
    pub launcher_log: LatestLogAnalysis,
    pub suspected_mods: Vec<SuspectedMod>,
    pub recent_snapshots: Vec<Snapshot>,
    pub graph_diagnostics: Vec<Diagnostic>,
    pub fix_plan: ChangePlan,
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
    LatestLogAnalysis {
        path,
        exists,
        tail,
        signals,
        suspected_mods,
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
    let selected_id = selected_report_id
        .filter(|id| reports.iter().any(|report| report.id == *id))
        .or_else(|| reports.first().map(|report| report.id.as_str()));
    let selected_report = selected_id
        .map(|id| analyze_crash_report(project_dir, id, manifest))
        .transpose()?;
    let latest_log = analyze_latest_log(project_dir, manifest);
    let launcher_log = analyze_launcher_log(project_dir, manifest);

    let mut suspect_sets = Vec::new();
    if let Some(report) = &selected_report {
        suspect_sets.push(report.suspected_mods.clone());
    }
    suspect_sets.push(latest_log.suspected_mods.clone());
    suspect_sets.push(launcher_log.suspected_mods.clone());
    let suspected_mods = merge_suspected_mods(suspect_sets.into_iter().flatten());

    let mut combined_signals = Vec::new();
    if let Some(report) = &selected_report {
        combined_signals.extend(report.signals.clone());
    }
    combined_signals.extend(latest_log.signals.clone());
    combined_signals.extend(launcher_log.signals.clone());

    let graph = DependencyGraph::from_manifest(manifest);
    let graph_diagnostics = Resolver::analyze_project(manifest, &graph);
    let fix_plan = create_crash_fix_plan(
        &graph,
        &graph_diagnostics,
        &suspected_mods,
        &combined_signals,
    );

    Ok(CrashDiagnosis {
        reports,
        selected_report,
        latest_log,
        launcher_log,
        suspected_mods,
        recent_snapshots,
        graph_diagnostics,
        fix_plan,
    })
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
        .map(|acc| SuspectedMod {
            id: acc.id,
            name: acc.name,
            version: acc.version,
            file_name: acc.file_name,
            known_in_manifest: acc.known_in_manifest,
            confidence: acc.confidence,
            evidence: acc.evidence,
        })
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
        });
        entry.confidence = entry.confidence.max(module.confidence);
        if module.known_in_manifest && !entry.known_in_manifest {
            entry.id = module.id.clone();
            entry.name = module.name.clone();
            entry.version = module.version.clone();
            entry.file_name = module.file_name.clone();
        }
        entry.known_in_manifest |= module.known_in_manifest;
        if entry.version.is_none() {
            entry.version = module.version.clone();
        }
        if entry.file_name.is_none() {
            entry.file_name = module.file_name.clone();
        }
        for evidence in module.evidence {
            if entry.evidence.len() >= MAX_EVIDENCE_PER_SUSPECT {
                break;
            }
            if !entry.evidence.iter().any(|item| {
                item.source == evidence.source && item.line_number == evidence.line_number
            }) {
                entry.evidence.push(evidence);
            }
        }
        entry.confidence = entry
            .confidence
            .saturating_add((entry.evidence.len().saturating_sub(1) as u8).min(7));
    }

    let mut out = by_id
        .into_values()
        .map(|acc| SuspectedMod {
            id: acc.id,
            name: acc.name,
            version: acc.version,
            file_name: acc.file_name,
            known_in_manifest: acc.known_in_manifest,
            confidence: acc.confidence.min(99),
            evidence: acc.evidence,
        })
        .collect::<Vec<_>>();
    out.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then_with(|| b.known_in_manifest.cmp(&a.known_in_manifest))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
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
    if lower.contains("could not execute entrypoint") || lower.contains("provided by '") {
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
    if lower.contains("mixin") {
        return Some(CrashSignalKind::Mixin);
    }
    if lower.contains("exception") || lower.contains("error:") || lower.contains("error ") {
        return Some(CrashSignalKind::Exception);
    }
    None
}

fn candidate_matches_line(candidate: &ModCandidate<'_>, line: &str) -> bool {
    let normalized_line = normalize_token(line);
    let compact_line = compact_token(&normalized_line);
    // Segment match avoids short name tokens like "critters" falsely hitting
    // another mod id such as "crittersandcompanions" via substring contains().
    let line_parts: HashSet<&str> = normalized_line
        .split('-')
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
    let entry = suspects.entry(key).or_insert_with(|| SuspectAccumulator {
        id: module.id.clone(),
        name: module.name.clone(),
        version: Some(module.version.clone()),
        file_name: module.file_name.clone(),
        known_in_manifest: true,
        confidence: 0,
        evidence: Vec::new(),
    });
    entry.confidence = entry.confidence.max(confidence);
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
    let entry = suspects.entry(key).or_insert_with(|| SuspectAccumulator {
        id: id.to_string(),
        name: id.to_string(),
        version: None,
        file_name,
        known_in_manifest: false,
        confidence: 0,
        evidence: Vec::new(),
    });
    entry.confidence = entry.confidence.max(confidence);
    push_evidence(entry, evidence);
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
        CrashSignalKind::Mixin => 78,
        CrashSignalKind::Entrypoint => 96,
        CrashSignalKind::LoaderMismatch => 86,
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
                },
                version: "0.5.8".to_string(),
                file_name: Some("sodium-fabric-mc1.20.1-0.5.8.jar".to_string()),
                hashes: None,
                side: Side::Client,
                dependencies: Vec::new(),
                status: Vec::new(),
                content_type: crate::manifest::ContentType::Mod,
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
            },
            version: "2.1.0".to_string(),
            file_name: Some("crittersandcompanions-fabric-2.1.0.jar".to_string()),
            hashes: None,
            side: Side::Both,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
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
            },
            version: "0.1.2".to_string(),
            file_name: Some("cosycritters-0.1.2+1.21.1-fabric.jar".to_string()),
            hashes: None,
            side: Side::Client,
            dependencies: Vec::new(),
            status: Vec::new(),
            content_type: crate::manifest::ContentType::Mod,
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
        };
        let resolved = SuspectedMod {
            id: "sodium".to_string(),
            name: manifest.mods[0].name.clone(),
            version: Some(manifest.mods[0].version.clone()),
            file_name: manifest.mods[0].file_name.clone(),
            known_in_manifest: true,
            confidence: 96,
            evidence: Vec::new(),
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
        };
        let plan = create_crash_fix_plan(&graph, &[], &[suspect], &[]);

        let ChangeAction::UpdateMod { target_version, .. } = &plan.actions[0] else {
            panic!("expected update action");
        };
        assert_eq!(target_version, LATEST_COMPATIBLE_VERSION);
        assert_eq!(resolve_update_target_version(target_version), None);
    }
}
