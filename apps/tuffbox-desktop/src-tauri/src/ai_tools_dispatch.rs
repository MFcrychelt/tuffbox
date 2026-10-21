//! Tool dispatcher: maps a tool name from `tuffbox_core::ai_tools::tool_catalogue`
//! onto the launcher function that already implements it.
//!
//! Deliberately a thin router, not a second implementation: every arm calls the
//! same code the UI calls, so a tool can never drift from the screen it mirrors.
//! Nothing here mutates the project — the catalogue is read-only, and changes
//! stay behind `ActionPlan` + user confirmation.
//!
//! Every tool runs inside `spawn_blocking`: the loop drives this from an async
//! command, and these read files, hash jars and hit provider APIs.

use serde_json::{json, Value};
use std::future::Future;
use std::path::{Path, PathBuf};
use tuffbox_core::ai_agent::{ToolCall, ToolDispatcher};
use tuffbox_core::manifest::ProjectManifest;

/// Cap on what a single tool hands the model, so one huge file cannot eat the
/// whole context window.
const MAX_TEXT_BYTES: usize = 120_000;
/// Log lines `read_log_tail` returns when the model does not say.
const DEFAULT_LOG_LINES: usize = 200;
const MAX_LOG_LINES: usize = 2000;

/// Tools bound to one open project.
pub struct ProjectTools {
    manifest_path: PathBuf,
}

impl ProjectTools {
    pub fn new(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
        }
    }
}

impl ToolDispatcher for ProjectTools {
    fn run(&self, call: &ToolCall) -> impl Future<Output = String> + Send {
        let manifest_path = self.manifest_path.clone();
        let name = call.name.clone();
        let name_for_error = name.clone();
        let arguments = call.arguments.clone();
        async move {
            let result =
                tokio::task::spawn_blocking(move || dispatch(&manifest_path, &name, &arguments))
                    .await;
            match result {
                Ok(Ok(value)) => value.to_string(),
                // A tool failure is information for the model, not a failed turn.
                Ok(Err(error)) => json!({ "error": error }).to_string(),
                Err(join) => json!({
                    "error": format!("tool {name_for_error} panicked: {join}")
                })
                .to_string(),
            }
        }
    }
}

/// [`AgentModel`] over the existing AI transport.
///
/// Records whether the endpoint accepted the tool payload, because the loop's
/// report does not carry that and the UI has to be able to say "this model
/// cannot inspect the project" instead of looking like the model chose not to.
pub struct TransportModel {
    settings: crate::integrations::AiSettings,
    tools_available: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl TransportModel {
    pub fn new(settings: crate::integrations::AiSettings) -> Self {
        Self {
            settings,
            tools_available: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        }
    }

    pub fn tools_available(&self) -> bool {
        self.tools_available
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl tuffbox_core::ai_agent::AgentModel for TransportModel {
    fn complete(
        &self,
        system: &str,
        messages: &[Value],
        tools: &Value,
    ) -> impl Future<Output = Result<tuffbox_core::ai_agent::AgentTurn, String>> + Send {
        let settings = self.settings.clone();
        let system = system.to_string();
        let messages = messages.to_vec();
        let tools = tools.clone();
        let flag = self.tools_available.clone();
        async move {
            let completion =
                crate::integrations::call_ai_messages_raw(&settings, &system, &messages, Some(&tools))
                    .await?;
            if !completion.tools_available {
                flag.store(false, std::sync::atomic::Ordering::Relaxed);
            }
            Ok(completion.turn)
        }
    }
}

fn dispatch(manifest_path: &Path, name: &str, arguments: &str) -> Result<Value, String> {
    let args: Value = if arguments.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(arguments)
            .map_err(|e| format!("{name}: arguments are not valid JSON: {e}"))?
    };
    match name {
        "list_mods" => list_mods(manifest_path),
        "read_config_file" => read_config_file(manifest_path, &args),
        "list_worlds" => list_worlds(manifest_path),
        "java_status" => java_status(manifest_path),
        "read_log_tail" => read_log_tail(manifest_path, &args),
        "find_mod_conflicts" => find_mod_conflicts(manifest_path),
        "diagnose_build" => diagnose_build(manifest_path, &args),
        "pack_health" => pack_health(manifest_path),
        "check_mod_updates" => check_mod_updates(manifest_path, &args),
        "search_content" => search_content(manifest_path, &args),
        "project_summary" => project_summary(manifest_path),
        "list_content" => list_content(manifest_path, &args),
        "read_options_file" => read_options_file(manifest_path, &args),
        "search_local_files" => search_local_files(manifest_path, &args),
        "list_crash_reports" => list_crash_reports(manifest_path),
        "read_crash_report" => read_crash_report(manifest_path, &args),
        "list_snapshots" => list_snapshots(manifest_path),
        "recent_launches" => recent_launches(manifest_path, &args),
        "list_servers" => list_servers(manifest_path),
        "world_dimensions" => world_dimensions(manifest_path, &args),
        "disk_usage" => disk_usage(manifest_path),
        "mod_file_metadata" => mod_file_metadata(manifest_path, &args),
        "lookup_modrinth_project" => lookup_modrinth_project(&args),
        other => Err(format!("unknown tool {other}")),
    }
}

fn project_dir(manifest_path: &Path) -> Result<PathBuf, String> {
    manifest_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "project manifest has no parent directory".to_string())
}

fn load_manifest(manifest_path: &Path) -> Result<ProjectManifest, String> {
    ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())
}

fn truncate(text: String) -> String {
    if text.len() <= MAX_TEXT_BYTES {
        return text;
    }
    let mut cut = MAX_TEXT_BYTES;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\n… (truncated at {MAX_TEXT_BYTES} bytes)", &text[..cut])
}

fn list_mods(manifest_path: &Path) -> Result<Value, String> {
    let manifest = load_manifest(manifest_path)?;
    let mods: Vec<Value> = manifest
        .mods
        .iter()
        .map(|module| serde_json::to_value(module).unwrap_or(Value::Null))
        .collect();
    Ok(json!({ "count": mods.len(), "mods": mods }))
}

fn read_config_file(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let relative = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "read_config_file needs a `path`".to_string())?;
    let project = project_dir(manifest_path)?;
    // Same guard the config editor uses: instance-relative only, no traversal.
    let target = crate::helpers::safe_project_file(&project, relative)?;
    if !target.is_file() {
        return Err(format!("{relative} is not a file in this instance"));
    }
    let text = std::fs::read_to_string(&target)
        .map_err(|e| format!("could not read {relative}: {e}"))?;
    Ok(json!({ "path": relative, "content": truncate(text) }))
}

fn list_worlds(manifest_path: &Path) -> Result<Value, String> {
    let saves = project_dir(manifest_path)?.join("saves");
    if !saves.is_dir() {
        return Ok(json!({ "count": 0, "worlds": [] }));
    }
    let mut worlds = Vec::new();
    for entry in std::fs::read_dir(&saves).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        match tuffbox_core::level_dat::read_world_info(&path) {
            Ok(info) => worlds.push(json!({
                "folder": name,
                "name": info.name,
                "version": info.version_name,
                "lastPlayed": info.last_played,
                "sizeBytes": info.size_bytes,
            })),
            // A folder that is not a world (or an unreadable save) is reported
            // by name rather than dropped silently.
            Err(reason) => worlds.push(json!({ "folder": name, "unreadable": reason })),
        }
    }
    Ok(json!({ "count": worlds.len(), "worlds": worlds }))
}

fn java_status(manifest_path: &Path) -> Result<Value, String> {
    let manifest = load_manifest(manifest_path)?;
    let required = tuffbox_core::jre::required_java_major(&manifest.minecraft.version);
    let runtimes = tuffbox_core::jre::find_all_runtimes()
        .map(|list| {
            list.into_iter()
                .map(|runtime| json!({ "path": runtime.path, "major": runtime.major }))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(json!({
        "minecraft": manifest.minecraft.version,
        "requiredMajor": required,
        "installed": runtimes,
        "satisfied": runtimes.iter().any(|r| r["major"].as_u64().unwrap_or(0) as u32 >= required),
    }))
}

/// Reads the tail of `logs/latest.log` without loading a multi-hundred-MB file.
fn log_tail_text(project: &Path, lines: usize) -> Result<String, String> {
    use std::io::{Read, Seek, SeekFrom};

    let path = project.join("logs").join("latest.log");
    if !path.is_file() {
        return Err("this instance has no logs/latest.log yet".to_string());
    }
    let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    // A 2000-line tail is a few MB at most; read that much and cut at a line
    // boundary instead of seeking into a multi-line encoding hazard.
    let window = (lines as u64 * 512).clamp(64 * 1024, 4 * 1024 * 1024);
    let start = length.saturating_sub(window);
    file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut buffer = Vec::with_capacity(window as usize);
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buffer).to_string();
    let mut tail: Vec<&str> = text.lines().rev().take(lines).collect();
    tail.reverse();
    Ok(tail.join("\n"))
}

fn read_log_tail(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let lines = args
        .get("lines")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).clamp(1, MAX_LOG_LINES))
        .unwrap_or(DEFAULT_LOG_LINES);
    let project = project_dir(manifest_path)?;
    let text = log_tail_text(&project, lines)?;
    Ok(json!({ "lines": text.lines().count(), "log": truncate(text) }))
}

fn find_mod_conflicts(manifest_path: &Path) -> Result<Value, String> {
    let manifest = load_manifest(manifest_path)?;
    let graph = tuffbox_core::DependencyGraph::from_manifest(&manifest);
    let diagnostics = tuffbox_core::Resolver::analyze_project(&manifest, &graph);
    let problems: Vec<Value> = diagnostics
        .iter()
        .filter(|diagnostic| {
            let code = diagnostic.code.to_ascii_lowercase();
            code.contains("conflict")
                || code.contains("dependency")
                || code.contains("missing")
                || format!("{:?}", diagnostic.severity).eq_ignore_ascii_case("error")
        })
        .map(|diagnostic| {
            json!({
                "code": diagnostic.code,
                "severity": format!("{:?}", diagnostic.severity).to_ascii_lowercase(),
                "message": diagnostic.message,
                "mods": diagnostic
                    .related_nodes
                    .iter()
                    .map(|node| node.0.clone())
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    // The loader's own resolution errors are the most actionable source, and
    // they only exist in the log — parse them with the same helper the UI uses.
    let project = project_dir(manifest_path)?;
    let reported = log_tail_text(&project, 400)
        .map(|text| {
            let lines: Vec<&str> = text.lines().collect();
            tuffbox_core::mod_conflict::parse_conflicts_from_lines(&lines)
                .into_iter()
                .map(|conflict| serde_json::to_value(conflict).unwrap_or(Value::Null))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(json!({
        "diagnostics": problems,
        "reportedInLog": reported,
        "hint": if problems.is_empty() && reported.is_empty() {
            "No conflicts found in the graph or the current log."
        } else {
            "Conflicts listed by the dependency graph and/or reported by the loader in the log."
        },
    }))
}

fn diagnose_build(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let project = project_dir(manifest_path)?;
    let manifest = load_manifest(manifest_path)?;
    let report_id = args.get("report_id").and_then(Value::as_str);
    // Empty snapshot list: the diagnosis falls back to the crash report and log
    // tail, which is what the model needs; snapshot history is a UI concern.
    let diagnosis =
        tuffbox_core::crash::build_crash_diagnosis(&project, &manifest, report_id, Vec::new())
            .map_err(|e| e.to_string())?;
    serde_json::to_value(&diagnosis).map_err(|e| e.to_string())
}

fn pack_health(manifest_path: &Path) -> Result<Value, String> {
    crate::get_health_report_impl(manifest_path.to_string_lossy().to_string())
}

fn check_mod_updates(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let only_outdated = args
        .get("only_outdated")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let manifest = load_manifest(manifest_path)?;
    let updates = tuffbox_core::updater::check_manifest_updates(&manifest)
        .map_err(|e| e.to_string())?;
    let mut rows: Vec<Value> = updates
        .values()
        .map(|update| {
            json!({
                "mod": update.mod_id,
                "current": update.current_version,
                "latest": update.latest_version,
                "risk": format!("{:?}", update.risk).to_ascii_lowercase(),
                "pinned": update.pinned,
            })
        })
        .collect();
    if only_outdated {
        rows.retain(|row| {
            !row["pinned"].as_bool().unwrap_or(false)
                && row["current"].as_str() != row["latest"].as_str()
        });
    }
    Ok(json!({ "count": rows.len(), "updates": rows }))
}

fn search_content(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    use tuffbox_core::provider::{ContentProvider, ModrinthProvider, ProviderSearchQuery};

    let query = args
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(|| "search_content needs a `query`".to_string())?;
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("mod")
        .to_string();
    let manifest = load_manifest(manifest_path)?;
    let provider = ModrinthProvider::new();
    let page = provider
        .search(&ProviderSearchQuery {
            query: Some(query.to_string()),
            minecraft_version: Some(manifest.minecraft.version.clone()),
            loader: Some(tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string()),
            project_type: Some(kind),
            limit: Some(20),
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
    let results: Vec<Value> = page
        .results
        .into_iter()
        .map(|hit| {
            json!({
                "id": hit.id,
                "slug": hit.slug,
                "name": hit.name,
                "author": hit.author,
                "description": hit.description,
                "downloads": hit.downloads,
                "type": hit.project_type,
            })
        })
        .collect();
    Ok(json!({ "count": results.len(), "total": page.total, "results": results }))
}

fn project_summary(manifest_path: &Path) -> Result<Value, String> {
    let manifest = load_manifest(manifest_path)?;
    let project = project_dir(manifest_path)?;
    let entries = |folder: &str| -> usize {
        std::fs::read_dir(project.join(folder))
            .map(|dir| dir.flatten().count())
            .unwrap_or(0)
    };
    Ok(json!({
        "name": manifest.project.name,
        "id": manifest.project.id,
        "version": manifest.project.version,
        "minecraft": manifest.minecraft.version,
        "loader": {
            "type": format!("{:?}", manifest.loader.kind).to_ascii_lowercase(),
            "version": manifest.loader.version,
        },
        "mods": manifest.mods.len(),
        "profiles": manifest.profiles.len(),
        "resourcepacks": entries("resourcepacks"),
        "shaderpacks": entries("shaderpacks"),
        "worlds": entries("saves"),
    }))
}

fn list_content(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let folder = args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("resourcepacks");
    if folder != "resourcepacks" && folder != "shaderpacks" {
        return Err("kind must be resourcepacks or shaderpacks".to_string());
    }
    let project = project_dir(manifest_path)?;
    let packs = tuffbox_core::content_packs::list_content_packs(&project, folder)?;
    Ok(json!({ "folder": folder, "count": packs.len(), "packs": packs }))
}

fn read_options_file(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let relative = args
        .get("path")
        .and_then(Value::as_str)
        .filter(|p| !p.trim().is_empty())
        .unwrap_or("options.txt");
    let project = project_dir(manifest_path)?;
    let target = crate::helpers::safe_project_file(&project, relative)?;
    if !target.is_file() {
        return Err(format!("{relative} is not a file in this instance"));
    }
    let text =
        std::fs::read_to_string(&target).map_err(|e| format!("could not read {relative}: {e}"))?;
    let wanted: Vec<String> = args
        .get("keys")
        .and_then(Value::as_array)
        .map(|keys| {
            keys.iter()
                .filter_map(|key| key.as_str().map(str::to_ascii_lowercase))
                .collect()
        })
        .unwrap_or_default();
    let entries: Vec<Value> = parse_key_values(&text)
        .into_iter()
        .filter(|(key, _)| wanted.is_empty() || wanted.contains(&key.to_ascii_lowercase()))
        .map(|(key, value)| json!({ "key": key, "value": value }))
        .collect();
    Ok(json!({ "path": relative, "count": entries.len(), "entries": entries }))
}

/// `key=value` **or** `key:value`, whichever comes first, skipping comments.
///
/// `PropertiesFile` (the config editor's parser) is deliberately `.properties`
/// only, i.e. `=`. Minecraft's `options.txt` uses `:`, which is the most common
/// thing to ask this tool for, so both separators are accepted here rather than
/// changing the shared parser's behaviour under the editor's feet.
fn parse_key_values(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
            continue;
        }
        let split = trimmed
            .find(['=', ':'])
            .map(|index| (index, &trimmed[index..index + 1]));
        let Some((index, _)) = split else { continue };
        let key = trimmed[..index].trim();
        if key.is_empty() {
            continue;
        }
        out.push((key.to_string(), trimmed[index + 1..].trim().to_string()));
    }
    out
}

fn search_local_files(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let query = args
        .get("query")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .filter(|q| !q.is_empty())
        .ok_or_else(|| "search_local_files needs a `query`".to_string())?;
    let extension = args
        .get("extension")
        .and_then(Value::as_str)
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase());
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).clamp(1, 200))
        .unwrap_or(50);
    let project = project_dir(manifest_path)?;

    // Bounded walk: an instance can hold tens of thousands of files and the
    // model needs the one it asked about, not an inventory.
    const MAX_DIRS: usize = 4000;
    let mut hits = Vec::new();
    let mut visited = 0usize;
    let mut stack = vec![project.clone()];
    while let Some(dir) = stack.pop() {
        if visited >= MAX_DIRS || hits.len() >= limit {
            break;
        }
        visited += 1;
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Caches and archives are noise for a name lookup, and the
                // session log archives can be huge.
                if name == ".tuffbox" || name == "logs" || name == "crash-reports" {
                    continue;
                }
                stack.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.to_ascii_lowercase().contains(&query) {
                continue;
            }
            if let Some(wanted) = &extension {
                let actual = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase);
                if actual.as_ref() != Some(wanted) {
                    continue;
                }
            }
            if let Ok(relative) = path.strip_prefix(&project) {
                hits.push(relative.to_string_lossy().replace('\\', "/"));
            }
            if hits.len() >= limit {
                break;
            }
        }
    }
    Ok(json!({ "count": hits.len(), "files": hits, "scannedDirectories": visited }))
}

fn list_crash_reports(manifest_path: &Path) -> Result<Value, String> {
    let project = project_dir(manifest_path)?;
    let reports = tuffbox_core::crash::list_crash_reports(&project).map_err(|e| e.to_string())?;
    Ok(json!({ "count": reports.len(), "reports": reports }))
}

fn read_crash_report(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let project = project_dir(manifest_path)?;
    let manifest = load_manifest(manifest_path)?;
    let report_id = match args.get("report_id").and_then(Value::as_str) {
        Some(id) if !id.trim().is_empty() => id.trim().to_string(),
        _ => tuffbox_core::crash::list_crash_reports(&project)
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .map(|report| report.id)
            .ok_or_else(|| "this instance has no crash reports".to_string())?,
    };
    let analysis = tuffbox_core::crash::analyze_crash_report(&project, &report_id, &manifest)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&analysis).map_err(|e| e.to_string())
}

fn list_snapshots(manifest_path: &Path) -> Result<Value, String> {
    let project = project_dir(manifest_path)?;
    let store = tuffbox_core::SnapshotStore::new(&project);
    let snapshots = store.list().map_err(|e| e.to_string())?;
    let rows: Vec<Value> = snapshots
        .iter()
        .take(30)
        .map(|snapshot| {
            json!({
                "id": snapshot.id,
                "name": snapshot.name,
                "reason": snapshot.reason,
                "createdAt": snapshot.created_at,
                "changedFiles": snapshot.changed_files.len(),
                "sizeBytes": snapshot.size_bytes,
            })
        })
        .collect();
    Ok(json!({ "count": snapshots.len(), "snapshots": rows }))
}

fn recent_launches(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| (n as usize).clamp(1, 100))
        .unwrap_or(20);
    let project = project_dir(manifest_path)?;
    let entries = tuffbox_core::launch_history::list_launch_history(&project, limit);
    Ok(json!({ "count": entries.len(), "launches": entries }))
}

fn list_servers(manifest_path: &Path) -> Result<Value, String> {
    let project = project_dir(manifest_path)?;
    let servers = tuffbox_core::servers_dat::list_servers(&project)?;
    // Icons are base64 blobs; the model only needs where the server is.
    let rows: Vec<Value> = servers
        .iter()
        .map(|server| json!({ "name": server.name, "address": server.address }))
        .collect();
    Ok(json!({ "count": rows.len(), "servers": rows }))
}

fn world_dimensions(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    let world = args
        .get("world")
        .and_then(Value::as_str)
        .ok_or_else(|| "world_dimensions needs a `world`".to_string())?;
    if world.contains('/') || world.contains('\\') || world.contains("..") {
        return Err("world must be a plain folder name".to_string());
    }
    let dir = project_dir(manifest_path)?.join("saves").join(world);
    if !dir.is_dir() {
        return Err(format!("no world folder named {world}"));
    }
    let dimensions = tuffbox_core::region::list_world_dimensions(&dir);
    Ok(json!({ "world": world, "dimensions": dimensions }))
}

/// Recursive byte/file count. Unreadable entries are skipped rather than fatal —
/// a size report is never worth failing over one locked file.
fn directory_size(dir: &Path) -> (u64, usize) {
    let mut total = 0u64;
    let mut files = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => stack.push(path),
                Ok(kind) if kind.is_file() => {
                    files += 1;
                    total += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                }
                _ => {}
            }
        }
    }
    (total, files)
}

fn disk_usage(manifest_path: &Path) -> Result<Value, String> {
    let project = project_dir(manifest_path)?;
    let (instance_bytes, files) = directory_size(&project);
    let (mods_bytes, _) = directory_size(&project.join("mods"));
    let (worlds_bytes, _) = directory_size(&project.join("saves"));
    Ok(json!({
        "instanceBytes": instance_bytes,
        "files": files,
        "modsBytes": mods_bytes,
        "worldsBytes": worlds_bytes,
    }))
}

fn mod_file_metadata(manifest_path: &Path, args: &Value) -> Result<Value, String> {
    use std::io::Read;

    let file = args
        .get("file")
        .and_then(Value::as_str)
        .ok_or_else(|| "mod_file_metadata needs a `file`".to_string())?;
    let project = project_dir(manifest_path)?;
    let target = crate::helpers::safe_project_file(&project, &format!("mods/{file}"))?;
    if !target.is_file() {
        return Err(format!("no mod jar named {file}"));
    }
    let jar = std::fs::File::open(&target).map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(jar).map_err(|e| format!("{file} is not a readable jar: {e}"))?;
    let mut metadata = serde_json::Map::new();
    for name in [
        "fabric.mod.json",
        "quilt.mod.json",
        "META-INF/neoforge.mods.toml",
        "META-INF/mods.toml",
        "mcmod.info",
    ] {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut text = String::new();
            if entry.read_to_string(&mut text).is_ok() {
                metadata.insert(name.to_string(), Value::String(truncate(text)));
            }
        }
    }
    if metadata.is_empty() {
        return Err(format!("{file} declares no loader metadata"));
    }
    Ok(json!({ "file": file, "metadata": Value::Object(metadata) }))
}

fn lookup_modrinth_project(args: &Value) -> Result<Value, String> {
    use tuffbox_core::provider::{ContentProvider, ModrinthProvider};

    let id = args
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "lookup_modrinth_project needs an `id`".to_string())?;
    let project = ModrinthProvider::new()
        .get_project(id)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&project).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with_manifest() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("project.tuffbox.json"),
            r#"{
              "schemaVersion": "0.1.0",
              "project": { "id": "demo", "name": "Demo Pack", "version": "0.1.0" },
              "minecraft": { "version": "1.20.1" },
              "loader": { "type": "fabric", "version": "0.15.11" },
              "mods": [
                {
                  "id": "sodium",
                  "name": "Sodium",
                  "source": { "type": "modrinth", "projectId": "sodium" },
                  "version": "0.5.8",
                  "side": "client",
                  "dependencies": []
                }
              ]
            }"#,
        )
        .expect("write manifest");
        dir
    }

    fn manifest_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("project.tuffbox.json")
    }

    #[test]
    fn unknown_tools_are_reported_not_silently_ignored() {
        let dir = project_with_manifest();
        let error = dispatch(&manifest_path(&dir), "make_coffee", "{}").unwrap_err();
        assert!(error.contains("unknown tool make_coffee"), "{error}");
    }

    #[test]
    fn broken_arguments_never_reach_a_tool() {
        let dir = project_with_manifest();
        let error = dispatch(&manifest_path(&dir), "list_mods", "{not json").unwrap_err();
        assert!(error.contains("not valid JSON"), "{error}");
    }

    #[test]
    fn list_mods_returns_the_manifest_modules() {
        let dir = project_with_manifest();
        let value = dispatch(&manifest_path(&dir), "list_mods", "{}").expect("ok");
        assert_eq!(value["count"], 1);
        assert_eq!(value["mods"][0]["id"], "sodium");
    }

    #[test]
    fn reading_a_config_file_stays_inside_the_instance() {
        let dir = project_with_manifest();
        let config = dir.path().join("config");
        std::fs::create_dir_all(&config).expect("mkdir");
        std::fs::write(config.join("sodium.json"), "{\"quality\":\"low\"}").expect("write");

        let ok = dispatch(
            &manifest_path(&dir),
            "read_config_file",
            r#"{"path":"config/sodium.json"}"#,
        )
        .expect("read");
        assert!(ok["content"].as_str().unwrap_or_default().contains("low"));

        // Traversal must be refused by the same guard the editor uses.
        let escaped = dispatch(
            &manifest_path(&dir),
            "read_config_file",
            r#"{"path":"../../etc/passwd"}"#,
        )
        .unwrap_err();
        assert!(!escaped.is_empty());

        let missing = dispatch(
            &manifest_path(&dir),
            "read_config_file",
            r#"{"path":"config/nope.json"}"#,
        )
        .unwrap_err();
        assert!(missing.contains("not a file"), "{missing}");
    }

    #[test]
    fn list_worlds_survives_an_instance_without_saves() {
        let dir = project_with_manifest();
        let value = dispatch(&manifest_path(&dir), "list_worlds", "{}").expect("ok");
        assert_eq!(value["count"], 0);
    }

    #[test]
    fn java_status_reports_the_required_major() {
        let dir = project_with_manifest();
        let value = dispatch(&manifest_path(&dir), "java_status", "{}").expect("ok");
        assert_eq!(value["minecraft"], "1.20.1");
        assert_eq!(value["requiredMajor"], tuffbox_core::jre::required_java_major("1.20.1"));
        assert!(value["installed"].is_array());
    }

    #[test]
    fn a_missing_log_is_an_error_the_model_can_read() {
        let dir = project_with_manifest();
        let error = dispatch(&manifest_path(&dir), "read_log_tail", "{}").unwrap_err();
        assert!(error.contains("latest.log"), "{error}");
    }

    #[test]
    fn log_tail_returns_only_the_requested_number_of_lines() {
        let dir = project_with_manifest();
        let logs = dir.path().join("logs");
        std::fs::create_dir_all(&logs).expect("mkdir");
        let body: String = (0..500).map(|i| format!("line {i}\n")).collect();
        std::fs::write(logs.join("latest.log"), body).expect("write log");

        let value = dispatch(&manifest_path(&dir), "read_log_tail", r#"{"lines":5}"#).expect("ok");
        assert_eq!(value["lines"], 5);
        let text = value["log"].as_str().unwrap_or_default();
        assert!(text.ends_with("line 499"), "{text}");
        assert!(!text.contains("line 494\n"), "tail leaked older lines: {text}");
    }

    #[test]
    fn every_catalogued_tool_has_a_dispatch_arm() {
        // The catalogue drives the model and the dispatcher runs it; if the two
        // drift, the model gets "unknown tool" for something it was told exists.
        let dir = project_with_manifest();
        let path = manifest_path(&dir);
        for tool in tuffbox_core::ai_tools::tool_catalogue() {
            let error = dispatch(&path, &tool.name, "{}").err().unwrap_or_default();
            assert!(
                !error.contains("unknown tool"),
                "{} is catalogued but not dispatchable",
                tool.name
            );
        }
    }

    #[test]
    fn project_summary_counts_the_instance() {
        let dir = project_with_manifest();
        let resourcepacks = dir.path().join("resourcepacks");
        std::fs::create_dir_all(&resourcepacks).expect("mkdir");
        std::fs::write(resourcepacks.join("fancy.zip"), b"PK").expect("write");
        std::fs::create_dir_all(dir.path().join("saves").join("demo")).expect("mkdir");

        let value = dispatch(&manifest_path(&dir), "project_summary", "{}").expect("ok");
        assert_eq!(value["name"], "Demo Pack");
        assert_eq!(value["minecraft"], "1.20.1");
        assert_eq!(value["loader"]["type"], "fabric");
        assert_eq!(value["mods"], 1);
        assert_eq!(value["resourcepacks"], 1);
        assert_eq!(value["worlds"], 1);
    }

    #[test]
    fn list_content_guards_the_folder_name() {
        let dir = project_with_manifest();
        let bad = dispatch(
            &manifest_path(&dir),
            "list_content",
            r#"{"kind":"../mods"}"#,
        )
        .unwrap_err();
        assert!(bad.contains("resourcepacks or shaderpacks"), "{bad}");

        let ok = dispatch(&manifest_path(&dir), "list_content", "{}").expect("ok");
        assert_eq!(ok["folder"], "resourcepacks");
        assert_eq!(ok["count"], 0);
    }

    #[test]
    fn read_options_file_parses_and_filters_keys() {
        let dir = project_with_manifest();
        std::fs::write(
            dir.path().join("options.txt"),
            "renderDistance:8\nnarrator:0\nguiScale:3\n",
        )
        .expect("write");

        let all = dispatch(&manifest_path(&dir), "read_options_file", "{}").expect("ok");
        assert_eq!(all["count"], 3);

        let filtered = dispatch(
            &manifest_path(&dir),
            "read_options_file",
            r#"{"keys":["renderDistance"]}"#,
        )
        .expect("ok");
        assert_eq!(filtered["count"], 1);
        assert_eq!(filtered["entries"][0]["value"], "8");
    }

    #[test]
    fn search_local_files_finds_by_name_and_extension() {
        let dir = project_with_manifest();
        let config = dir.path().join("config");
        std::fs::create_dir_all(&config).expect("mkdir");
        std::fs::write(config.join("sodium-options.json"), "{}").expect("write");
        std::fs::write(config.join("sodium.txt"), "x").expect("write");

        let any = dispatch(
            &manifest_path(&dir),
            "search_local_files",
            r#"{"query":"sodium"}"#,
        )
        .expect("ok");
        assert_eq!(any["count"], 2);

        let only_json = dispatch(
            &manifest_path(&dir),
            "search_local_files",
            r#"{"query":"sodium","extension":"json"}"#,
        )
        .expect("ok");
        assert_eq!(only_json["count"], 1);
        assert!(only_json["files"][0]
            .as_str()
            .unwrap_or_default()
            .ends_with("config/sodium-options.json"));

        let missing = dispatch(
            &manifest_path(&dir),
            "search_local_files",
            r#"{"query":"nothing-like-this"}"#,
        )
        .expect("ok");
        assert_eq!(missing["count"], 0);
    }

    #[test]
    fn empty_collections_are_reported_as_empty_not_as_errors() {
        let dir = project_with_manifest();
        for (tool, key) in [
            ("list_crash_reports", "reports"),
            ("list_snapshots", "snapshots"),
            ("recent_launches", "launches"),
            ("list_servers", "servers"),
        ] {
            let value = dispatch(&manifest_path(&dir), tool, "{}").expect(tool);
            assert_eq!(value["count"], 0, "{tool}");
            assert!(value[key].is_array(), "{tool}");
        }
    }

    #[test]
    fn world_dimensions_refuses_to_walk_out_of_the_saves_folder() {
        let dir = project_with_manifest();
        for world in ["../mods", "a/b", "a\\b"] {
            let error = dispatch(
                &manifest_path(&dir),
                "world_dimensions",
                &json!({ "world": world }).to_string(),
            )
            .unwrap_err();
            assert!(error.contains("plain folder name"), "{world}: {error}");
        }

        let unknown = dispatch(
            &manifest_path(&dir),
            "world_dimensions",
            r#"{"world":"absent"}"#,
        )
        .unwrap_err();
        assert!(unknown.contains("no world folder"), "{unknown}");
    }

    #[test]
    fn world_dimensions_lists_what_exists() {
        let dir = project_with_manifest();
        let region = dir.path().join("saves").join("demo").join("region");
        std::fs::create_dir_all(&region).expect("mkdir");
        let value = dispatch(
            &manifest_path(&dir),
            "world_dimensions",
            r#"{"world":"demo"}"#,
        )
        .expect("ok");
        let dims = value["dimensions"].as_array().cloned().unwrap_or_default();
        assert!(dims.iter().any(|d| d == "overworld"), "{value}");
    }

    #[test]
    fn disk_usage_counts_the_instance() {
        let dir = project_with_manifest();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).expect("mkdir");
        std::fs::write(mods.join("a.jar"), vec![0u8; 1024]).expect("write");

        let value = dispatch(&manifest_path(&dir), "disk_usage", "{}").expect("ok");
        assert!(value["modsBytes"].as_u64().unwrap_or(0) >= 1024, "{value}");
        assert!(value["instanceBytes"].as_u64().unwrap_or(0) >= 1024, "{value}");
        assert!(value["files"].as_u64().unwrap_or(0) >= 2, "{value}");
    }

    #[test]
    fn mod_file_metadata_reports_a_missing_jar_clearly() {
        let dir = project_with_manifest();
        let error = dispatch(
            &manifest_path(&dir),
            "mod_file_metadata",
            r#"{"file":"absent.jar"}"#,
        )
        .unwrap_err();
        assert!(error.contains("no mod jar"), "{error}");
    }
}
