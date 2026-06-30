use std::path::{Path, PathBuf};
use tuffbox_core::{
    ContentProvider, DependencyGraph, ModSource, ModSpec, PackBrief, ProjectManifest, ProviderFileInfo,
    ProviderSearchQuery, Resolver, Side, Snapshot, SnapshotStore, SourceKind, TuffboxLockfile,
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    id: String,
    name: String,
    version: String,
    minecraft_version: String,
    loader_kind: String,
    loader_version: String,
    java_path: Option<String>,
    memory_mb: u32,
    jvm_args: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFileSummary {
    path: String,
    name: String,
    extension: String,
    size: u64,
    modified: Option<u64>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SchemaStatus {
    current: String,
    detected: String,
    needs_migration: bool,
    supported: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileSummary {
    id: String,
    name: String,
    side: String,
    memory_mb: Option<u32>,
    jvm_args: Vec<String>,
}


#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectChangeEntry {
    id: String,
    snapshot_id: String,
    operation: String,
    reason: String,
    created_at: String,
    path: String,
    category: String,
    kind: String,
    preview: String,
    diff: String,
    can_open: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryFileContent {
    path: String,
    content: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistorySettings {
    tracked: std::collections::HashMap<String, bool>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModInstallPreview {
    project_id: String,
    slug: String,
    name: String,
    version: String,
    file_name: Option<String>,
    side: String,
    dependencies: Vec<tuffbox_core::ModDependencySpec>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TestRunRecord {
    id: String,
    profile: String,
    started_at: String,
    status: String,
    log_path: String,
    duration_seconds: Option<u64>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseSnapshotResult {
    snapshot: tuffbox_core::Snapshot,
    changelog_path: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ReleaseArtifactRecord {
    id: String,
    kind: String,
    path: String,
    created_at: String,
    file_count: usize,
    override_count: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseDraftResult {
    draft_path: String,
    metadata_path: String,
    artifact_count: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotFileDiff {
    path: String,
    from_exists: bool,
    to_exists: bool,
    text: String,
}

#[tauri::command(rename_all = "camelCase")]
fn get_project_schema_status(path: String) -> Result<SchemaStatus, String> {
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let detected = value
        .get("schemaVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("0.1.0")
        .to_string();
    let supported = tuffbox_core::manifest::SUPPORTED_PROJECT_SCHEMA_VERSIONS
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>();
    if !supported.iter().any(|v| v == &detected) {
        return Err(format!(
            "unsupported project schema version {detected}; supported versions: {}",
            supported.join(", ")
        ));
    }
    Ok(SchemaStatus {
        current: tuffbox_core::manifest::CURRENT_PROJECT_SCHEMA_VERSION.to_string(),
        needs_migration: detected != tuffbox_core::manifest::CURRENT_PROJECT_SCHEMA_VERSION,
        detected,
        supported,
    })
}

#[tauri::command(rename_all = "camelCase")]
fn migrate_project_schema(path: String) -> Result<SchemaStatus, String> {
    auto_snapshot(&PathBuf::from(&path), "migrate-schema").map_err(|e| e.to_string())?;
    let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    manifest.migrate_to_current_schema();
    save_manifest(&PathBuf::from(&path), &manifest).map_err(|e| e.to_string())?;
    get_project_schema_status(path)
}

#[tauri::command]
fn validate_project(path: String) -> Result<ProjectSummary, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    manifest.validate_basic().map_err(|e| e.to_string())?;
    let profile = manifest
        .profiles
        .iter()
        .find(|p| p.id == "client")
        .or_else(|| manifest.profiles.first())
        .ok_or_else(|| "project has no profiles".to_string())?;

    Ok(ProjectSummary {
        id: manifest.project.id.clone(),
        name: manifest.project.name.clone(),
        version: manifest.project.version.clone(),
        minecraft_version: manifest.minecraft.version.clone(),
        loader_kind: format!("{:?}", manifest.loader.kind).to_lowercase(),
        loader_version: manifest.loader.version.clone(),
        java_path: manifest.java.as_ref().and_then(|j| j.path.clone()),
        memory_mb: profile.memory_mb.unwrap_or(4096),
        jvm_args: profile.jvm_args.clone(),
    })
}

#[tauri::command(rename_all = "camelCase")]
fn get_project_brief(path: String) -> Result<PackBrief, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    Ok(manifest.brief.unwrap_or_default())
}

#[tauri::command(rename_all = "camelCase")]
fn update_project_brief(path: String, brief: PackBrief) -> Result<(), String> {
    let manifest_path = PathBuf::from(&path);
    auto_snapshot(&manifest_path, "update-brief").map_err(|e| e.to_string())?;
    let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    manifest.brief = Some(brief);
    save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_profiles(path: String) -> Result<Vec<ProfileSummary>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    Ok(manifest
        .profiles
        .into_iter()
        .map(|p| ProfileSummary {
            id: p.id,
            name: p.name,
            side: format!("{:?}", p.side).to_lowercase(),
            memory_mb: p.memory_mb,
            jvm_args: p.jvm_args,
        })
        .collect())
}

#[tauri::command]
fn list_mods(path: String) -> Result<Vec<serde_json::Value>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let mods = manifest
        .mods
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "name": m.name,
                "version": m.version,
                "side": format!("{:?}", m.side).to_lowercase(),
                "source": format!("{:?}", m.source.kind).to_lowercase(),
                "projectId": m.source.project_id,
                "fileName": m.file_name,
                "iconUrl": null,
            })
        })
        .collect();
    Ok(mods)
}

#[tauri::command(rename_all = "camelCase")]
async fn search_modrinth_mods(
    path: String,
    query: String,
    game_version: Option<String>,
    loader: Option<String>,
    category: Option<String>,
    environment: Option<String>,
    license: Option<String>,
    sort: Option<String>,
) -> Result<Vec<tuffbox_core::ProjectInfo>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let provider = tuffbox_core::ModrinthProvider::new();
        let default_loader = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string();
        provider
            .search(&ProviderSearchQuery {
                query: Some(query),
                minecraft_version: game_version.or_else(|| Some(manifest.minecraft.version.clone())),
                loader: loader.or_else(|| Some(default_loader)),
                category,
                environment,
                license,
                sort,
                limit: Some(30),
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn preview_modrinth_install(path: String, mod_id: String) -> Result<ModInstallPreview, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let provider = tuffbox_core::ModrinthProvider::new();
        let project = provider.get_project(&mod_id).map_err(|e| e.to_string())?;
        let query = ProviderSearchQuery {
            query: None,
            minecraft_version: Some(manifest.minecraft.version.clone()),
            loader: Some(tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string()),
            ..Default::default()
        };
        let version = provider
            .get_versions(&mod_id, &query)
            .map_err(|e| e.to_string())?
            .into_iter()
            .next()
            .ok_or_else(|| format!("no compatible version found for {mod_id}"))?;
        let file_name = ProviderFileInfo::primary_file(&version).map(|file| file.filename.clone());
        let dependencies = provider.resolve_dependencies(&version.id).unwrap_or_default();
        let side = format!("{:?}", infer_project_side(Some(&project))).to_lowercase();
        Ok(ModInstallPreview {
            project_id: project.id,
            slug: project.slug,
            name: project.name,
            version: version.version_number,
            file_name,
            side,
            dependencies,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn get_modrinth_project_icon(project_id: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        let provider = tuffbox_core::ModrinthProvider::new();
        provider
            .get_project(&project_id)
            .map(|project| project.icon_url)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn add_modrinth_mod(path: String, mod_id: String, side: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        auto_snapshot(&PathBuf::from(&path), "add-mod").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        add_mod_from_modrinth(&mut manifest, &mod_id, Some(side)).map_err(|e| e.to_string())?;
        save_manifest(&PathBuf::from(&path), &manifest).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn add_modrinth_mod_with_dependencies(
    path: String,
    mod_id: String,
    side: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "add-mod-with-dependencies").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let installed = install_modrinth_with_dependencies(&mut manifest, &[mod_id], &side);
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn add_modrinth_mods_with_dependencies(
    path: String,
    mod_ids: Vec<String>,
    side: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "bulk-add-mods-with-dependencies").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let installed = install_modrinth_with_dependencies(&mut manifest, &mod_ids, &side);
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
fn remove_project_mod(path: String, mod_id: String) -> Result<(), String> {
    auto_snapshot(&PathBuf::from(&path), "remove-mod").map_err(|e| e.to_string())?;
    let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let original_len = manifest.mods.len();
    manifest.mods.retain(|m| m.id != mod_id);
    if manifest.mods.len() == original_len {
        return Err(format!("mod {mod_id} not found in project"));
    }
    save_manifest(&PathBuf::from(&path), &manifest).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_project_mod(path: String, mod_id: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        auto_snapshot(&PathBuf::from(&path), "update-mod").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        update_mod_from_modrinth(&mut manifest, &mod_id).map_err(|e| e.to_string())?;
        save_manifest(&PathBuf::from(&path), &manifest).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
fn list_config_files(path: String) -> Result<Vec<ConfigFileSummary>, String> {
    let project_dir = manifest_parent(&path)?;
    let mut files = Vec::new();
    for root in ["config", "defaultconfigs", "kubejs", "scripts"] {
        let dir = project_dir.join(root);
        if dir.is_dir() {
            collect_config_files(&project_dir, &dir, &mut files).map_err(|e| e.to_string())?;
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

#[tauri::command(rename_all = "camelCase")]
fn read_config_file(path: String, relative_path: String) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let target = safe_project_file(&project_dir, &relative_path)?;
    let metadata = std::fs::metadata(&target).map_err(|e| e.to_string())?;
    if metadata.len() > 2 * 1024 * 1024 {
        return Err("file is too large for the MVP config editor".to_string());
    }
    std::fs::read_to_string(target).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn write_config_file(path: String, relative_path: String, content: String) -> Result<(), String> {
    if content.len() > 2 * 1024 * 1024 {
        return Err("file is too large for the MVP config editor".to_string());
    }
    let manifest_path = PathBuf::from(&path);
    let project_dir = manifest_parent(&path)?;
    let target = safe_project_file(&project_dir, &relative_path)?;
    auto_snapshot_with_changed_files(&manifest_path, "edit-config", &[PathBuf::from(&relative_path)])
        .map_err(|e| e.to_string())?;
    std::fs::write(target, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_graph(path: String) -> Result<DependencyGraph, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    Ok(DependencyGraph::from_manifest(&manifest))
}

#[tauri::command]
fn get_diagnostics(path: String) -> Result<Vec<tuffbox_core::Diagnostic>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let graph = DependencyGraph::from_manifest(&manifest);
    Ok(Resolver::analyze_project(&manifest, &graph))
}

#[tauri::command(rename_all = "camelCase")]
fn get_resolve_change_plan(path: String) -> Result<Option<tuffbox_core::ChangePlan>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let graph = DependencyGraph::from_manifest(&manifest);
    let diagnostics = Resolver::analyze_project(&manifest, &graph);
    Ok(Resolver::create_fix_plan(&graph, &diagnostics))
}

#[tauri::command(rename_all = "camelCase")]
async fn apply_resolve_action(path: String, action_index: usize) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let graph = DependencyGraph::from_manifest(&manifest);
        let diagnostics = Resolver::analyze_project(&manifest, &graph);
        let Some(plan) = Resolver::create_fix_plan(&graph, &diagnostics) else {
            return Ok(Vec::new());
        };
        let Some(action) = plan.actions.get(action_index).cloned() else {
            return Err(format!("action index {action_index} out of range"));
        };
        if plan.requires_snapshot {
            auto_snapshot(&manifest_path, "apply-resolve-action").map_err(|e| e.to_string())?;
        }
        let mut applied = Vec::new();
        apply_change_action(&mut manifest, action, &mut applied)?;
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        Ok(applied)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn apply_resolve_change_plan(path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let graph = DependencyGraph::from_manifest(&manifest);
        let diagnostics = Resolver::analyze_project(&manifest, &graph);
        let Some(plan) = Resolver::create_fix_plan(&graph, &diagnostics) else {
            return Ok(Vec::new());
        };
        if plan.requires_snapshot {
            auto_snapshot(&manifest_path, "apply-resolve-plan").map_err(|e| e.to_string())?;
        }
        let mut applied = Vec::new();
        for action in plan.actions {
            apply_change_action(&mut manifest, action, &mut applied)?;
        }
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        Ok(applied)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn resolve_missing_dependencies(path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let graph = DependencyGraph::from_manifest(&manifest);
        let diagnostics = Resolver::analyze_project(&manifest, &graph);
        let mut missing = diagnostics
            .iter()
            .filter(|d| d.code == "MISSING_DEPENDENCY")
            .filter_map(|d| d.related_nodes.last())
            .filter_map(|id| id.0.strip_prefix("mod:").map(|s| s.to_string()))
            .collect::<Vec<_>>();
        missing.sort();
        missing.dedup();
        if missing.is_empty() {
            return Ok(Vec::new());
        }
        auto_snapshot(&manifest_path, "resolve-dependencies").map_err(|e| e.to_string())?;
        let mut installed = Vec::new();
        for mod_id in missing {
            if manifest.mods.iter().any(|m| m.id == mod_id) {
                continue;
            }
            match add_mod_from_modrinth(&mut manifest, &mod_id, Some("auto".to_string())) {
                Ok(()) => installed.push(mod_id),
                Err(e) => eprintln!("failed to resolve dependency {mod_id}: {e}"),
            }
        }
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
fn get_crash_diagnosis(
    path: String,
    report_id: Option<String>,
) -> Result<tuffbox_core::crash::CrashDiagnosis, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mut snapshots = SnapshotStore::new(&project_dir).list().unwrap_or_default();
    snapshots.reverse();
    snapshots.truncate(6);
    tuffbox_core::crash::build_crash_diagnosis(
        &project_dir,
        &manifest,
        report_id.as_deref(),
        snapshots,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn create_crash_fix_plan(
    path: String,
    report_id: Option<String>,
) -> Result<tuffbox_core::ChangePlan, String> {
    Ok(get_crash_diagnosis(path, report_id)?.fix_plan)
}

#[tauri::command(rename_all = "camelCase")]
fn get_history_settings(path: String) -> Result<HistorySettings, String> {
    let project_dir = manifest_parent(&path)?;
    let settings_path = project_dir.join(".tuffbox").join("history.json");
    if settings_path.is_file() {
        let raw = std::fs::read_to_string(settings_path).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| e.to_string())
    } else {
        Ok(default_history_settings())
    }
}

#[tauri::command(rename_all = "camelCase")]
fn update_history_settings(path: String, settings: HistorySettings) -> Result<HistorySettings, String> {
    let project_dir = manifest_parent(&path)?;
    let settings_dir = project_dir.join(".tuffbox");
    std::fs::create_dir_all(&settings_dir).map_err(|e| e.to_string())?;
    let settings_path = settings_dir.join("history.json");
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_path, json).map_err(|e| e.to_string())?;
    Ok(settings)
}

#[tauri::command(rename_all = "camelCase")]
fn list_project_change_history(path: String) -> Result<Vec<ProjectChangeEntry>, String> {
    let manifest_path = PathBuf::from(&path);
    let project_dir = manifest_parent(&path)?;
    let store = SnapshotStore::new(&project_dir);
    let snapshots = store.list().map_err(|e| e.to_string())?;
    let mut entries = Vec::new();

    for (index, snapshot) in snapshots.iter().enumerate() {
        let after_manifest_path = snapshots
            .get(index + 1)
            .map(|next| next.manifest_path.as_path())
            .unwrap_or(manifest_path.as_path());
        if let (Ok(before), Ok(after)) = (
            ProjectManifest::load_from_path(&snapshot.manifest_path),
            ProjectManifest::load_from_path(after_manifest_path),
        ) {
            entries.extend(mod_change_entries(snapshot, &before, &after));
        }

        for relative in &snapshot.changed_files {
            let relative_text = relative.to_string_lossy().replace('\\', "/");
            let before_path = project_dir
                .join(".tuffbox")
                .join("snapshots")
                .join(&snapshot.id)
                .join("changed_files")
                .join(relative);
            let after_path = project_dir.join(relative);
            let before_text = read_small_text_file(&before_path).unwrap_or_default();
            let after_text = read_small_text_file(&after_path).unwrap_or_default();
            let diff = unified_text_diff(&before_text, &after_text);
            entries.push(ProjectChangeEntry {
                id: format!("{}:{}", snapshot.id, relative_text),
                snapshot_id: snapshot.id.clone(),
                operation: snapshot.name.clone(),
                reason: snapshot.reason.clone(),
                created_at: snapshot.created_at.clone(),
                path: relative_text.clone(),
                category: change_category(&relative_text).to_string(),
                kind: "file_changed".to_string(),
                preview: diff_preview(&diff),
                diff,
                can_open: after_path.is_file() && is_editable_config_path(&after_path),
            });
        }
    }

    entries.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.path.cmp(&b.path)));
    Ok(entries)
}

#[tauri::command(rename_all = "camelCase")]
fn read_project_history_file(path: String, relative_path: String) -> Result<HistoryFileContent, String> {
    let project_dir = manifest_parent(&path)?;
    let target = safe_project_file(&project_dir, &relative_path)?;
    let metadata = std::fs::metadata(&target).map_err(|e| e.to_string())?;
    if metadata.len() > 2 * 1024 * 1024 {
        return Err("file is too large for the built-in editor".to_string());
    }
    Ok(HistoryFileContent {
        path: relative_path,
        content: std::fs::read_to_string(target).map_err(|e| e.to_string())?,
    })
}

#[tauri::command(rename_all = "camelCase")]
fn create_tracked_history_snapshot(path: String, roots: Vec<String>) -> Result<tuffbox_core::Snapshot, String> {
    let manifest_path = PathBuf::from(&path);
    let project_dir = manifest_parent(&path)?;
    let mut changed_files = Vec::new();
    for root in roots {
        match root.as_str() {
            "options.txt" | "servers.dat" => {
                let file = project_dir.join(&root);
                if file.is_file() {
                    changed_files.push(PathBuf::from(root));
                }
            }
            _ => {
                let dir = project_dir.join(&root);
                if dir.is_dir() {
                    collect_tracked_project_files(&project_dir, &dir, &mut changed_files).map_err(|e| e.to_string())?;
                }
            }
        }
    }
    changed_files.sort();
    changed_files.dedup();
    auto_snapshot_with_changed_files(&manifest_path, "track-history", &changed_files).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn rollback_history_file(path: String, snapshot_id: String, relative_path: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let relative = validate_relative_snapshot_path(&relative_path)?;
    let src = project_dir
        .join(".tuffbox")
        .join("snapshots")
        .join(&snapshot_id)
        .join("changed_files")
        .join(&relative);
    if !src.is_file() {
        return Err("file is not stored in this snapshot".to_string());
    }
    let dst = project_dir.join(&relative);
    let canonical_project = std::fs::canonicalize(&project_dir).map_err(|e| e.to_string())?;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let canonical_parent = std::fs::canonicalize(dst.parent().unwrap_or(&project_dir)).map_err(|e| e.to_string())?;
    if !canonical_parent.starts_with(&canonical_project) {
        return Err("file is outside project directory".to_string());
    }
    std::fs::copy(src, dst).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_project_dir(path: String) -> Result<String, String> {
    PathBuf::from(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "manifest has no parent directory".to_string())
}

#[tauri::command]
fn list_snapshots(project_dir: String) -> Result<Vec<tuffbox_core::Snapshot>, String> {
    let store = SnapshotStore::new(&project_dir);
    store.list().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_snapshot(
    project_dir: String,
    name: String,
    reason: String,
) -> Result<tuffbox_core::Snapshot, String> {
    let store = SnapshotStore::new(&project_dir);
    let manifest_path = find_manifest_in_project_dir(&project_dir)?;
    let lockfile_path = manifest_path.with_extension("lock.json");
    let lockfile_path = if lockfile_path.exists() {
        Some(lockfile_path)
    } else {
        None
    };
    store
        .create(
            &name,
            &reason,
            &manifest_path,
            lockfile_path.as_ref(),
            &[] as &[&Path],
        )
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn diff_snapshots(project_dir: String, from: String, to: String) -> Result<tuffbox_core::SnapshotDiff, String> {
    let store = SnapshotStore::new(&project_dir);
    store.diff(from, to).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn rollback_snapshot(project_dir: String, id: String) -> Result<tuffbox_core::Snapshot, String> {
    let store = SnapshotStore::new(&project_dir);
    store.rollback(id).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn get_snapshot_file_diff(
    project_dir: String,
    from: String,
    to: String,
    relative_path: String,
) -> Result<SnapshotFileDiff, String> {
    let relative = validate_relative_snapshot_path(&relative_path)?;
    let base = PathBuf::from(project_dir).join(".tuffbox").join("snapshots");
    let from_path = base.join(&from).join("changed_files").join(&relative);
    let to_path = base.join(&to).join("changed_files").join(&relative);
    let from_exists = from_path.is_file();
    let to_exists = to_path.is_file();
    let from_text = read_small_text_file(&from_path)?;
    let to_text = read_small_text_file(&to_path)?;
    Ok(SnapshotFileDiff {
        path: relative_path,
        from_exists,
        to_exists,
        text: unified_text_diff(&from_text, &to_text),
    })
}

#[tauri::command(rename_all = "camelCase")]
fn validate_modrinth_export(path: String) -> Result<Vec<tuffbox_core::ExportIssue>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    Ok(tuffbox_core::validate_modrinth_export(&manifest))
}

#[tauri::command(rename_all = "camelCase")]
fn generate_release_changelog(path: String) -> Result<String, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let graph = DependencyGraph::from_manifest(&manifest);
    let diagnostics = Resolver::analyze_project(&manifest, &graph);
    let project_dir = manifest_parent(&path)?;
    let snapshots = SnapshotStore::new(&project_dir).list().unwrap_or_default();
    let mut out = String::new();
    out.push_str(&format!("# {} {}\n\n", manifest.project.name, manifest.project.version));
    if let Some(description) = &manifest.project.description {
        out.push_str(description);
        out.push_str("\n\n");
    }
    if let Some(brief) = &manifest.brief {
        if !brief.goal.trim().is_empty() {
            out.push_str(&format!("## Goal\n\n{}\n\n", brief.goal.trim()));
        }
    }
    out.push_str("## Platform\n\n");
    out.push_str(&format!("- Minecraft: {}\n", manifest.minecraft.version));
    out.push_str(&format!("- Loader: {:?} {}\n", manifest.loader.kind, manifest.loader.version));
    out.push_str(&format!("- Mods: {}\n\n", manifest.mods.len()));
    out.push_str("## Included mods\n\n");
    for module in &manifest.mods {
        out.push_str(&format!("- {} `{}` ({:?})\n", module.name, module.version, module.side));
    }
    out.push_str("\n## Diagnostics\n\n");
    if diagnostics.is_empty() {
        out.push_str("- No current diagnostics.\n");
    } else {
        for diagnostic in diagnostics {
            out.push_str(&format!("- {:?}: {} — {}\n", diagnostic.severity, diagnostic.code, diagnostic.message));
        }
    }
    out.push_str("\n## Recent snapshots\n\n");
    for snapshot in snapshots.iter().rev().take(5) {
        out.push_str(&format!("- {} — {} ({})\n", snapshot.created_at, snapshot.name, snapshot.reason));
    }
    Ok(out)
}

#[tauri::command(rename_all = "camelCase")]
fn update_project_version(path: String, version: String) -> Result<ProjectSummary, String> {
    if version.trim().is_empty() {
        return Err("version cannot be empty".to_string());
    }
    let manifest_path = PathBuf::from(&path);
    auto_snapshot(&manifest_path, "version-bump").map_err(|e| e.to_string())?;
    let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    manifest.project.version = version;
    save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
    validate_project(path)
}

#[tauri::command(rename_all = "camelCase")]
fn create_release_snapshot(path: String, changelog: String) -> Result<ReleaseSnapshotResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let manifest_path = PathBuf::from(&path);
    let project_dir = manifest_parent(&path)?;
    let changelog_dir = project_dir.join("releases");
    std::fs::create_dir_all(&changelog_dir).map_err(|e| e.to_string())?;
    let changelog_path = changelog_dir.join(format!("{}-CHANGELOG.md", manifest.project.version));
    std::fs::write(&changelog_path, changelog).map_err(|e| e.to_string())?;
    let snapshot = auto_snapshot_with_changed_files(
        &manifest_path,
        "release",
        &[PathBuf::from("releases").join(format!("{}-CHANGELOG.md", manifest.project.version))],
    )
    .map_err(|e| e.to_string())?;
    Ok(ReleaseSnapshotResult {
        snapshot,
        changelog_path: changelog_path.to_string_lossy().to_string(),
    })
}

#[tauri::command(rename_all = "camelCase")]
fn export_modrinth_pack(path: String, target_path: Option<String>) -> Result<tuffbox_core::ExportResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let output = target_path
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(&path)
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{}-{}.mrpack", manifest.project.id, manifest.project.version))
        });
    let result = tuffbox_core::export_modrinth_pack(&manifest, &path, &output).map_err(|e| e.to_string())?;
    append_release_artifact(&path, "mrpack", &result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command(rename_all = "camelCase")]
fn export_server_pack(path: String, target_path: Option<String>) -> Result<tuffbox_core::ExportResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let output = target_path
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(&path)
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{}-{}-server.zip", manifest.project.id, manifest.project.version))
        });
    let result = tuffbox_core::export_server_pack(&manifest, &path, &output).map_err(|e| e.to_string())?;
    append_release_artifact(&path, "server", &result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command(rename_all = "camelCase")]
fn export_prism_instance(path: String, target_path: Option<String>) -> Result<tuffbox_core::ExportResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let output = target_path
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(&path)
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{}-{}-prism.zip", manifest.project.id, manifest.project.version))
        });
    let result = tuffbox_core::export_prism_instance(&manifest, &path, &output).map_err(|e| e.to_string())?;
    append_release_artifact(&path, "prism", &result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command(rename_all = "camelCase")]
fn export_curseforge_pack(path: String, target_path: Option<String>) -> Result<tuffbox_core::ExportResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let output = target_path
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(&path)
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{}-{}-curseforge.zip", manifest.project.id, manifest.project.version))
        });
    let result = tuffbox_core::export_curseforge_pack(&manifest, &path, &output).map_err(|e| e.to_string())?;
    append_release_artifact(&path, "curseforge", &result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command(rename_all = "camelCase")]
fn list_release_artifacts(path: String) -> Result<Vec<ReleaseArtifactRecord>, String> {
    let project_dir = manifest_parent(&path)?;
    let artifacts_path = project_dir.join(".tuffbox").join("artifacts.json");
    if !artifacts_path.is_file() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(artifacts_path).map_err(|e| e.to_string())?;
    let mut artifacts: Vec<ReleaseArtifactRecord> = serde_json::from_str(&raw).unwrap_or_default();
    artifacts.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(artifacts)
}

#[tauri::command(rename_all = "camelCase")]
fn create_release_draft(path: String, changelog: String) -> Result<ReleaseDraftResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let artifacts = list_release_artifacts(path.clone()).unwrap_or_default();
    let releases_dir = project_dir.join("releases");
    std::fs::create_dir_all(&releases_dir).map_err(|e| e.to_string())?;
    let draft_path = releases_dir.join(format!("{}-DRAFT.md", manifest.project.version));
    let metadata_dir = project_dir.join(".tuffbox");
    std::fs::create_dir_all(&metadata_dir).map_err(|e| e.to_string())?;
    let metadata_path = metadata_dir.join("release-draft.json");

    let mut markdown = String::new();
    markdown.push_str(&format!("# {} {} release draft

", manifest.project.name, manifest.project.version));
    markdown.push_str("## Changelog

");
    markdown.push_str(changelog.trim());
    markdown.push_str("

## Artifacts

");
    if artifacts.is_empty() {
        markdown.push_str("- No artifacts exported yet.
");
    } else {
        for artifact in &artifacts {
            markdown.push_str(&format!(
                "- **{}**: `{}` ({} files, {} overrides)
",
                artifact.kind, artifact.path, artifact.file_count, artifact.override_count
            ));
        }
    }
    markdown.push_str("
## Publish checklist

");
    markdown.push_str("- [ ] Upload artifacts to target platform
");
    markdown.push_str("- [ ] Verify game/loader versions
");
    markdown.push_str("- [ ] Verify server pack starts
");
    markdown.push_str("- [ ] Announce known issues
");
    std::fs::write(&draft_path, markdown).map_err(|e| e.to_string())?;

    let artifact_count = artifacts.len();
    let metadata = serde_json::json!({
        "projectId": manifest.project.id.clone(),
        "version": manifest.project.version.clone(),
        "draftPath": draft_path.to_string_lossy().to_string(),
        "artifacts": artifacts,
        "createdAt": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
        "targets": {
            "modrinth": "draft-placeholder",
            "curseforge": "draft-placeholder",
            "githubReleases": "draft-placeholder"
        }
    });
    std::fs::write(&metadata_path, serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    Ok(ReleaseDraftResult {
        draft_path: draft_path.to_string_lossy().to_string(),
        metadata_path: metadata_path.to_string_lossy().to_string(),
        artifact_count,
    })
}

#[tauri::command]
fn generate_lockfile(path: String) -> Result<TuffboxLockfile, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let graph = DependencyGraph::from_manifest(&manifest);
    Ok(TuffboxLockfile::from_manifest_and_graph(&manifest, &graph))
}

#[tauri::command(rename_all = "camelCase")]
fn capture_test_run_logs(path: String, run_id: String) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let target_dir = project_dir.join(".tuffbox").join("test-runs").join(&run_id);
    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;
    let candidates = [
        (project_dir.join("logs").join("latest.log"), "latest.log"),
        (project_dir.join("launcher.log"), "launcher.log"),
        (project_dir.join("launcher_log.txt"), "launcher_log.txt"),
        (project_dir.join("logs").join("launcher.log"), "logs-launcher.log"),
        (project_dir.join("logs").join("launcher_log.txt"), "logs-launcher_log.txt"),
    ];
    let mut copied = 0usize;
    for (src, name) in candidates {
        if src.is_file() {
            std::fs::copy(&src, target_dir.join(name)).map_err(|e| e.to_string())?;
            copied += 1;
        }
    }
    if copied == 0 {
        return Err("no logs found to capture".to_string());
    }
    Ok(target_dir.to_string_lossy().to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn list_test_runs(path: String) -> Result<Vec<TestRunRecord>, String> {
    let project_dir = manifest_parent(&path)?;
    let runs_path = project_dir.join(".tuffbox").join("test-runs.json");
    if !runs_path.is_file() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&runs_path).map_err(|e| e.to_string())?;
    let mut runs: Vec<TestRunRecord> = serde_json::from_str(&raw).unwrap_or_default();
    for run in &mut runs {
        let log_path = PathBuf::from(&run.log_path);
        if let Ok(log) = tuffbox_core::process::read_log_tail(&log_path, 200) {
            if log.contains("# Launch error:") {
                run.status = "failed".to_string();
            } else if log.contains("Process exited") || log.contains("Stopping!") {
                run.status = "finished".to_string();
            }
            if run.status != "started" && run.duration_seconds.is_none() {
                if let Ok(started) = run.started_at.parse::<u64>() {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    run.duration_seconds = Some(now.saturating_sub(started));
                }
            }
        }
    }
    runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(runs)
}

#[tauri::command]
async fn launch_profile(path: String, profile: String) -> Result<tuffbox_core::LaunchResult, String> {
    let log_path = PathBuf::from(&path)
        .parent()
        .map(|p| p.join("logs").join("latest.log"))
        .ok_or_else(|| "manifest has no parent directory".to_string())?;

    {
        use std::io::Write;
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut log = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&log_path)
            .map_err(|e| e.to_string())?;
        writeln!(log, "# Launching Minecraft...").ok();
        if let Some(project_dir) = PathBuf::from(&path).parent() {
            let launcher_log = project_dir.join("launcher_log.txt");
            let mut launcher = std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&launcher_log)
                .map_err(|e| e.to_string())?;
            writeln!(launcher, "# TuffBox launching profile {profile}").ok();
            if let Some(logs_dir) = log_path.parent() {
                let _ = std::fs::write(logs_dir.join("launcher_log.txt"), format!("# TuffBox launching profile {profile}\n"));
            }
        }
    }

    append_test_run_record(&path, &profile, &log_path).map_err(|e| e.to_string())?;

    let log_path_clone = log_path.clone();
    let log_path_err = log_path.clone();
    tokio::task::spawn_blocking(move || {
        match build_and_spawn(path, profile, log_path_clone) {
            Ok(()) => {}
            Err(e) => {
                use std::io::Write;
                if let Ok(mut log) = std::fs::OpenOptions::new().append(true).open(&log_path_err) {
                    let _ = writeln!(log, "# Launch error: {e}");
                }
            }
        }
    });

    Ok(tuffbox_core::LaunchResult {
        exit_code: None,
        log_path,
    })
}

fn build_and_spawn(path: String, profile: String, log_path: PathBuf) -> Result<(), String> {
    use tuffbox_core::{LaunchOptions, TestLauncher};

    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_profile = manifest
        .profiles
        .iter()
        .find(|p| p.id == profile)
        .ok_or_else(|| format!("profile {profile} not found"))?
        .clone();

    let java_path = manifest.java.as_ref().and_then(|j| j.path.clone());
    let java = if let Some(java_path) = java_path {
        tuffbox_core::jre::check_java_at_path(&PathBuf::from(&java_path)).map_err(|e| e.to_string())?
    } else {
        TestLauncher::find_java().map_err(|e| e.to_string())?
    };

    let progress = tuffbox_core::mc_install::InstallProgress {
        log_path: log_path.clone(),
    };

    progress.log(&format!("# Java: {} (major {})", java.path, java.major));
    progress.log(&format!("# Java version: {}", java.version));

    // game_dir = папка сборки (где mods, config, saves)
    let game_dir = PathBuf::from(&path)
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "manifest has no parent directory".to_string())?;

    // launcher_dir = общая папка TuffBox (где versions, libraries, assets)
    let launcher_dir = dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox");

    std::fs::create_dir_all(&launcher_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;

    progress.log(&format!("# Game directory: {}", game_dir.display()));
    progress.log(&format!("# Launcher directory: {}", launcher_dir.display()));
    progress.log("# Installing Minecraft (this may take a while)...");

    let options = LaunchOptions {
        profile_id: profile.clone(),
        instance_dir: game_dir,
        memory_mb: project_profile.memory_mb.unwrap_or(4096),
        jvm_args: project_profile.jvm_args.clone(),
    };

    let (cmd, _) = TestLauncher::build_command(&manifest, &project_profile, &options, &java, &launcher_dir, &progress)
        .map_err(|e| e.to_string())?;

    progress.log("# Starting Java process...");

    tuffbox_core::process::spawn_and_track(profile, cmd, &log_path)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
fn import_project(source: String, target_dir: String) -> Result<String, String> {
    use tuffbox_core::{import_folder, import_modrinth_pack, import_prism_instance};

    let path = PathBuf::from(&source);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let manifest = if path.is_dir() {
        import_folder(&source).map_err(|e| e.to_string())?
    } else {
        match ext.as_str() {
            "mrpack" => import_modrinth_pack(&source).map_err(|e| e.to_string())?,
            "zip" => import_prism_instance(&source).map_err(|e| e.to_string())?,
            _ => return Err(format!("unsupported import format: {ext}")),
        }
    };

    let target = PathBuf::from(target_dir).join(format!("{}.tuffbox.json", manifest.project.id));
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&target, json).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

#[tauri::command]
#[allow(deprecated)]
fn open_project_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let dir = PathBuf::from(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "manifest has no parent directory".to_string())?;
    use tauri_plugin_shell::ShellExt;
    app.shell()
        .open(dir, None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_project(path: String) -> Result<(), String> {
    std::fs::remove_file(&path).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_minecraft_versions() -> Result<Vec<tuffbox_core::versions::MinecraftVersion>, String> {
    tokio::task::spawn_blocking(|| tuffbox_core::versions::fetch_minecraft_versions())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn get_loader_versions(
    loader: String,
    minecraft_version: String,
) -> Result<Vec<tuffbox_core::versions::LoaderVersion>, String> {
    tokio::task::spawn_blocking(move || tuffbox_core::versions::fetch_loader_versions(&loader, &minecraft_version))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn find_java_runtimes() -> Result<Vec<tuffbox_core::jre::JavaRuntime>, String> {
    tokio::task::spawn_blocking(|| tuffbox_core::jre::find_all_runtimes())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_java_version(path: String) -> Result<String, String> {
    let runtime = tuffbox_core::jre::check_java_at_path(&PathBuf::from(path)).map_err(|e| e.to_string())?;
    Ok(runtime.version)
}

#[tauri::command]
fn get_default_java_version() -> Result<String, String> {
    let runtime = tuffbox_core::jre::find_all_runtimes()
        .map_err(|e| e.to_string())?
        .into_iter()
        .next()
        .ok_or_else(|| "no java runtime found".to_string())?;
    Ok(runtime.version)
}

fn append_release_artifact(
    manifest_path: &str,
    kind: &str,
    result: &tuffbox_core::ExportResult,
) -> anyhow::Result<()> {
    let project_dir = PathBuf::from(manifest_path)
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("manifest has no parent directory"))?;
    let dir = project_dir.join(".tuffbox");
    std::fs::create_dir_all(&dir)?;
    let artifacts_path = dir.join("artifacts.json");
    let mut artifacts: Vec<ReleaseArtifactRecord> = if artifacts_path.is_file() {
        serde_json::from_str(&std::fs::read_to_string(&artifacts_path)?).unwrap_or_default()
    } else {
        Vec::new()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    artifacts.push(ReleaseArtifactRecord {
        id: format!("artifact-{kind}-{now}"),
        kind: kind.to_string(),
        path: result.path.to_string_lossy().to_string(),
        created_at: now.to_string(),
        file_count: result.file_count,
        override_count: result.override_count,
    });
    if artifacts.len() > 100 {
        let keep_from = artifacts.len().saturating_sub(100);
        artifacts = artifacts.split_off(keep_from);
    }
    std::fs::write(artifacts_path, serde_json::to_string_pretty(&artifacts)?)?;
    Ok(())
}

fn append_test_run_record(manifest_path: &str, profile: &str, log_path: &Path) -> anyhow::Result<()> {
    let project_dir = PathBuf::from(manifest_path)
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| anyhow::anyhow!("manifest has no parent directory"))?;
    let dir = project_dir.join(".tuffbox");
    std::fs::create_dir_all(&dir)?;
    let runs_path = dir.join("test-runs.json");
    let mut runs: Vec<TestRunRecord> = if runs_path.is_file() {
        serde_json::from_str(&std::fs::read_to_string(&runs_path)?).unwrap_or_default()
    } else {
        Vec::new()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    runs.push(TestRunRecord {
        id: format!("run-{profile}-{now}"),
        profile: profile.to_string(),
        started_at: now.to_string(),
        status: "started".to_string(),
        log_path: log_path.to_string_lossy().to_string(),
        duration_seconds: None,
    });
    if runs.len() > 100 {
        let keep_from = runs.len().saturating_sub(100);
        runs = runs.split_off(keep_from);
    }
    std::fs::write(runs_path, serde_json::to_string_pretty(&runs)?)?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
fn get_launch_log(path: String) -> Result<String, String> {
    let log_path = PathBuf::from(&path)
        .parent()
        .map(|p| p.join("logs").join("latest.log"))
        .ok_or_else(|| "manifest has no parent directory".to_string())?;
    tuffbox_core::process::read_log_tail(&log_path, 500).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn update_project_settings(
    path: String,
    minecraft_version: String,
    loader: String,
    loader_version: String,
    java_path: Option<String>,
    memory_mb: u32,
    jvm_args: Vec<String>,
) -> Result<(), String> {
    use tuffbox_core::manifest::{JavaSpec, LoaderKind, LoaderSpec, MinecraftSpec};

    let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;

    let loader_kind = match loader.as_str() {
        "fabric" => LoaderKind::Fabric,
        "forge" => LoaderKind::Forge,
        "neoforge" => LoaderKind::Neoforge,
        "quilt" => LoaderKind::Quilt,
        _ => LoaderKind::Vanilla,
    };

    manifest.minecraft = MinecraftSpec {
        version: minecraft_version,
    };
    manifest.loader = LoaderSpec {
        kind: loader_kind,
        version: loader_version,
    };
    manifest.java = Some(JavaSpec {
        major: manifest.java.as_ref().and_then(|j| j.major),
        distribution: manifest.java.as_ref().and_then(|j| j.distribution.clone()),
        path: java_path,
    });

    if let Some(profile) = manifest.profiles.iter_mut().find(|p| p.id == "client") {
        profile.memory_mb = Some(memory_mb);
        profile.jvm_args = jvm_args;
    }

    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_home_dir() -> Result<String, String> {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "home directory not found".to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn create_instance(
    name: String,
    minecraft_version: String,
    loader: String,
    loader_version: String,
    location: String,
) -> Result<String, String> {
    use tuffbox_core::manifest::{
        JavaSpec, LoaderKind, LoaderSpec, MinecraftSpec, ProfileSpec, ProjectManifest,
        ProjectMetadata, Side,
    };

    let loader_kind = match loader.as_str() {
        "fabric" => LoaderKind::Fabric,
        "forge" => LoaderKind::Forge,
        "neoforge" => LoaderKind::Neoforge,
        "quilt" => LoaderKind::Quilt,
        _ => LoaderKind::Vanilla,
    };

    let id = name
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string();

    let dir = PathBuf::from(&location);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let manifest = ProjectManifest {
        schema_version: "0.1.0".to_string(),
        project: ProjectMetadata {
            id: id.clone(),
            name: name.clone(),
            version: "1.0.0".to_string(),
            description: None,
            authors: Vec::new(),
        },
        minecraft: MinecraftSpec {
            version: minecraft_version,
        },
        loader: LoaderSpec {
            kind: loader_kind,
            version: loader_version,
        },
        brief: None,
        java: Some(JavaSpec {
            major: Some(17),
            distribution: None,
            path: None,
        }),
        profiles: vec![ProfileSpec {
            id: "client".to_string(),
            name: "Client".to_string(),
            side: Side::Client,
            include_optional_mods: false,
            include_shaders: true,
            memory_mb: Some(4096),
            jvm_args: vec!["-XX:+UseG1GC".to_string()],
            include_mods: Vec::new(),
        }],
        mods: Vec::new(),
        overrides: None,
    };

    let path = dir.join(format!("{id}.tuffbox.json"));
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

fn find_manifest_in_project_dir(project_dir: &str) -> Result<PathBuf, String> {
    let dir = PathBuf::from(project_dir);
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".tuffbox.json"))
            .unwrap_or(false)
        {
            return Ok(path);
        }
    }
    Err("project manifest not found in project directory".to_string())
}

fn manifest_parent(path: &str) -> Result<PathBuf, String> {
    PathBuf::from(path)
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "manifest has no parent directory".to_string())
}

fn collect_tracked_project_files(
    project_dir: &Path,
    dir: &Path,
    files: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_tracked_project_files(project_dir, &path, files)?;
            continue;
        }
        if !path.is_file() {
            continue;
        }
        let relative = path.strip_prefix(project_dir).unwrap_or(&path).to_path_buf();
        files.push(relative);
    }
    Ok(())
}

fn collect_config_files(
    project_dir: &Path,
    dir: &Path,
    files: &mut Vec<ConfigFileSummary>,
) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_config_files(project_dir, &path, files)?;
            continue;
        }
        if !path.is_file() || !is_editable_config_path(&path) {
            continue;
        }
        let metadata = std::fs::metadata(&path)?;
        if metadata.len() > 2 * 1024 * 1024 {
            continue;
        }
        let relative = path
            .strip_prefix(project_dir)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        files.push(ConfigFileSummary {
            name,
            extension: path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase(),
            path: relative,
            size: metadata.len(),
            modified,
        });
    }
    Ok(())
}

fn safe_project_file(project_dir: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let relative = PathBuf::from(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("invalid project-relative path".to_string());
    }
    if !is_editable_config_path(&relative) {
        return Err("unsupported config file type".to_string());
    }
    let target = project_dir.join(relative);
    let canonical_project = std::fs::canonicalize(project_dir).map_err(|e| e.to_string())?;
    let canonical_target = std::fs::canonicalize(&target).map_err(|e| e.to_string())?;
    if !canonical_target.starts_with(&canonical_project) {
        return Err("file is outside project directory".to_string());
    }
    Ok(canonical_target)
}

fn is_editable_config_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str(),
        "json" | "json5" | "toml" | "properties" | "cfg" | "conf" | "txt" | "js" | "zs" | "yaml" | "yml" | "md"
    )
}

fn validate_relative_snapshot_path(relative_path: &str) -> Result<PathBuf, String> {
    let relative = PathBuf::from(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("invalid snapshot-relative path".to_string());
    }
    Ok(relative)
}

fn read_small_text_file(path: &Path) -> Result<String, String> {
    if !path.is_file() {
        return Ok(String::new());
    }
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 512 * 1024 {
        return Ok(format!(
            "# File is too large for inline diff: {} bytes\n",
            metadata.len()
        ));
    }
    std::fs::read_to_string(path).map_err(|_| "# Binary or non-UTF8 file; inline diff unavailable.\n".to_string())
}

fn unified_text_diff(before: &str, after: &str) -> String {
    if before == after {
        return "No content changes.".to_string();
    }
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let mut table = vec![vec![0usize; after_lines.len() + 1]; before_lines.len() + 1];
    for i in (0..before_lines.len()).rev() {
        for j in (0..after_lines.len()).rev() {
            table[i][j] = if before_lines[i] == after_lines[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    let mut out = String::new();
    let mut i = 0;
    let mut j = 0;
    while i < before_lines.len() && j < after_lines.len() {
        if before_lines[i] == after_lines[j] {
            out.push_str("  ");
            out.push_str(before_lines[i]);
            out.push('\n');
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            out.push_str("- ");
            out.push_str(before_lines[i]);
            out.push('\n');
            i += 1;
        } else {
            out.push_str("+ ");
            out.push_str(after_lines[j]);
            out.push('\n');
            j += 1;
        }
    }
    while i < before_lines.len() {
        out.push_str("- ");
        out.push_str(before_lines[i]);
        out.push('\n');
        i += 1;
    }
    while j < after_lines.len() {
        out.push_str("+ ");
        out.push_str(after_lines[j]);
        out.push('\n');
        j += 1;
    }
    out
}


fn mod_change_entries(
    snapshot: &tuffbox_core::Snapshot,
    before: &ProjectManifest,
    after: &ProjectManifest,
) -> Vec<ProjectChangeEntry> {
    let mut entries = Vec::new();
    let before_mods: std::collections::HashMap<_, _> = before.mods.iter().map(|m| (m.id.as_str(), m)).collect();
    let after_mods: std::collections::HashMap<_, _> = after.mods.iter().map(|m| (m.id.as_str(), m)).collect();

    for (id, module) in &after_mods {
        if !before_mods.contains_key(*id) {
            entries.push(ProjectChangeEntry {
                id: format!("{}:mod-added:{id}", snapshot.id),
                snapshot_id: snapshot.id.clone(),
                operation: snapshot.name.clone(),
                reason: snapshot.reason.clone(),
                created_at: snapshot.created_at.clone(),
                path: "project.tuffbox.json".to_string(),
                category: "Mods".to_string(),
                kind: "mod_added".to_string(),
                preview: format!("Added {} {} ({:?})", module.name, module.version, module.side),
                diff: format!("+ {} {} ({:?})", module.name, module.version, module.side),
                can_open: false,
            });
        }
    }

    for (id, module) in &before_mods {
        if !after_mods.contains_key(*id) {
            entries.push(ProjectChangeEntry {
                id: format!("{}:mod-removed:{id}", snapshot.id),
                snapshot_id: snapshot.id.clone(),
                operation: snapshot.name.clone(),
                reason: snapshot.reason.clone(),
                created_at: snapshot.created_at.clone(),
                path: "project.tuffbox.json".to_string(),
                category: "Mods".to_string(),
                kind: "mod_removed".to_string(),
                preview: format!("Removed {} {} ({:?})", module.name, module.version, module.side),
                diff: format!("- {} {} ({:?})", module.name, module.version, module.side),
                can_open: false,
            });
        }
    }

    for (id, before_module) in &before_mods {
        let Some(after_module) = after_mods.get(*id) else { continue; };
        if before_module.version != after_module.version || before_module.file_name != after_module.file_name || before_module.side != after_module.side {
            entries.push(ProjectChangeEntry {
                id: format!("{}:mod-updated:{id}", snapshot.id),
                snapshot_id: snapshot.id.clone(),
                operation: snapshot.name.clone(),
                reason: snapshot.reason.clone(),
                created_at: snapshot.created_at.clone(),
                path: "project.tuffbox.json".to_string(),
                category: "Mods".to_string(),
                kind: "mod_updated".to_string(),
                preview: format!("Updated {}: {} → {}", after_module.name, before_module.version, after_module.version),
                diff: format!("- {} {} ({:?})\n+ {} {} ({:?})", before_module.name, before_module.version, before_module.side, after_module.name, after_module.version, after_module.side),
                can_open: false,
            });
        }
    }

    entries
}

fn default_history_settings() -> HistorySettings {
    let mut tracked = std::collections::HashMap::new();
    tracked.insert("Mods".to_string(), true);
    tracked.insert("Configs".to_string(), true);
    tracked.insert("Shaders".to_string(), true);
    tracked.insert("Resource Packs".to_string(), true);
    tracked.insert("World/Data".to_string(), false);
    tracked.insert("Other".to_string(), true);
    HistorySettings { tracked }
}

fn change_category(path: &str) -> &'static str {
    let normalized = path.replace('\\', "/").to_lowercase();
    let root = normalized.split('/').next().unwrap_or("");
    if matches!(normalized.as_str(), "options.txt" | "servers.dat") {
        return "Configs";
    }
    match root {
        "config" | "defaultconfigs" | "kubejs" | "scripts" => "Configs",
        "shaderpacks" | "shaders" => "Shaders",
        "resourcepacks" | "texturepacks" => "Resource Packs",
        "datapacks" | "world" | "saves" => "World/Data",
        _ => "Other",
    }
}

fn diff_preview(diff: &str) -> String {
    let lines = diff
        .lines()
        .filter(|line| line.starts_with("+ ") || line.starts_with("- "))
        .take(8)
        .collect::<Vec<_>>();
    if lines.is_empty() {
        diff.lines().take(6).collect::<Vec<_>>().join("\n")
    } else {
        lines.join("\n")
    }
}

fn apply_change_action(
    manifest: &mut ProjectManifest,
    action: tuffbox_core::ChangeAction,
    applied: &mut Vec<String>,
) -> Result<(), String> {
    match action {
        tuffbox_core::ChangeAction::InstallMod { project_id, .. } => {
            add_mod_from_modrinth(manifest, &project_id, Some("auto".to_string()))
                .map_err(|e| e.to_string())?;
            applied.push(format!("installed {project_id}"));
        }
        tuffbox_core::ChangeAction::RemoveMod { node_id }
        | tuffbox_core::ChangeAction::DisableMod { node_id } => {
            let mod_id = node_id.0.strip_prefix("mod:").unwrap_or(&node_id.0).to_string();
            let before = manifest.mods.len();
            manifest.mods.retain(|m| m.id != mod_id);
            if manifest.mods.len() != before {
                applied.push(format!("removed {mod_id}"));
            }
        }
        tuffbox_core::ChangeAction::UpdateMod { node_id, .. } => {
            let mod_id = node_id.0.strip_prefix("mod:").unwrap_or(&node_id.0).to_string();
            update_mod_from_modrinth(manifest, &mod_id).map_err(|e| e.to_string())?;
            applied.push(format!("updated {mod_id}"));
        }
        tuffbox_core::ChangeAction::EditConfig { path, .. } => {
            applied.push(format!("manual config review required for {path}"));
        }
    }
    Ok(())
}

fn install_modrinth_with_dependencies(
    manifest: &mut ProjectManifest,
    mod_ids: &[String],
    side: &str,
) -> Vec<String> {
    let mut installed = Vec::new();
    for mod_id in mod_ids {
        if manifest
            .mods
            .iter()
            .any(|m| m.id == *mod_id || m.source.project_id.as_deref() == Some(mod_id.as_str()))
        {
            continue;
        }
        if add_mod_from_modrinth(manifest, mod_id, Some(side.to_string())).is_ok() {
            installed.push(mod_id.clone());
        }
    }

    let mut failed = std::collections::HashSet::new();
    for _ in 0..50 {
        let missing = manifest
            .mods
            .iter()
            .flat_map(|module| module.dependencies.iter())
            .filter(|dep| dep.kind == tuffbox_core::DependencyKind::Requires)
            .map(|dep| dep.target.clone())
            .filter(|target| {
                !manifest.mods.iter().any(|m| m.id == *target || m.source.project_id.as_deref() == Some(target.as_str()))
                    && !failed.contains(target)
            })
            .collect::<Vec<_>>();

        if missing.is_empty() {
            break;
        }

        for dependency_id in missing {
            match add_mod_from_modrinth(manifest, &dependency_id, Some("auto".to_string())) {
                Ok(()) => installed.push(dependency_id),
                Err(_) => {
                    failed.insert(dependency_id);
                }
            }
        }
    }

    installed
}

fn add_mod_from_modrinth(
    manifest: &mut ProjectManifest,
    mod_id: &str,
    side: Option<String>,
) -> anyhow::Result<()> {
    let provider = tuffbox_core::ModrinthProvider::new();
    let project = provider.get_project(mod_id)?;

    if manifest
        .mods
        .iter()
        .any(|m| m.id == project.slug || m.source.project_id.as_deref() == Some(project.id.as_str()))
    {
        anyhow::bail!("mod {} is already in the project", project.slug);
    }

    let query = ProviderSearchQuery {
        query: None,
        minecraft_version: Some(manifest.minecraft.version.clone()),
        loader: Some(tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string()),
        ..Default::default()
    };
    let versions = provider.get_versions(mod_id, &query)?;
    let version = versions
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no compatible version found for {mod_id}"))?;

    let file = ProviderFileInfo::primary_file(&version)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no primary file for version {}", version.id))?;

    let dependencies = provider.resolve_dependencies(&version.id)?;
    let mod_side = parse_side(side.as_deref(), Some(&project));
    let mod_spec = build_mod_spec(&project, &version, file, dependencies, mod_side);
    manifest.mods.push(mod_spec);
    Ok(())
}

fn update_mod_from_modrinth(manifest: &mut ProjectManifest, mod_id: &str) -> anyhow::Result<()> {
    let provider = tuffbox_core::ModrinthProvider::new();
    let index = manifest
        .mods
        .iter()
        .position(|m| m.id == mod_id)
        .ok_or_else(|| anyhow::anyhow!("mod {mod_id} not found in project"))?;

    let project_id = manifest.mods[index]
        .source
        .project_id
        .clone()
        .unwrap_or_else(|| mod_id.to_string());
    let project = provider.get_project(&project_id)?;

    let query = ProviderSearchQuery {
        query: None,
        minecraft_version: Some(manifest.minecraft.version.clone()),
        loader: Some(tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string()),
        ..Default::default()
    };
    let versions = provider.get_versions(&project_id, &query)?;
    let version = versions
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no compatible version found for {project_id}"))?;

    let file = ProviderFileInfo::primary_file(&version)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no primary file for version {}", version.id))?;

    let side = manifest.mods[index].side;
    let dependencies = provider.resolve_dependencies(&version.id)?;
    manifest.mods[index] = build_mod_spec(&project, &version, file, dependencies, side);
    Ok(())
}

fn build_mod_spec(
    project: &tuffbox_core::ProjectInfo,
    version: &tuffbox_core::VersionInfo,
    file: ProviderFileInfo,
    dependencies: Vec<tuffbox_core::ModDependencySpec>,
    side: Side,
) -> ModSpec {
    ModSpec {
        id: project.slug.clone(),
        name: project.name.clone(),
        source: ModSource {
            kind: SourceKind::Modrinth,
            project_id: Some(project.id.clone()),
            file_id: Some(version.id.clone()),
            url: Some(file.url),
            path: None,
        },
        version: version.version_number.clone(),
        file_name: Some(file.filename),
        hashes: Some(tuffbox_core::FileHashes {
            sha1: file.hashes.sha1,
            sha512: file.hashes.sha512,
        }),
        side,
        dependencies,
        status: vec!["ok".to_string()],
    }
}

fn parse_side(side: Option<&str>, project: Option<&tuffbox_core::ProjectInfo>) -> Side {
    match side {
        Some("client") => Side::Client,
        Some("server") => Side::Server,
        Some("both") => Side::Both,
        Some("auto") | None => infer_project_side(project),
        _ => infer_project_side(project),
    }
}

fn infer_project_side(project: Option<&tuffbox_core::ProjectInfo>) -> Side {
    let Some(project) = project else { return Side::Both; };
    let client = project.client_side.as_deref().unwrap_or("unknown");
    let server = project.server_side.as_deref().unwrap_or("unknown");
    match (client, server) {
        ("required" | "optional", "unsupported") => Side::Client,
        ("unsupported", "required" | "optional") => Side::Server,
        ("required" | "optional", "required" | "optional") => Side::Both,
        _ => Side::Unknown,
    }
}

fn auto_snapshot(manifest_path: &Path, operation: &str) -> anyhow::Result<Snapshot> {
    auto_snapshot_with_changed_files(manifest_path, operation, &[])
}

fn auto_snapshot_with_changed_files(
    manifest_path: &Path,
    operation: &str,
    changed_files: &[PathBuf],
) -> anyhow::Result<Snapshot> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("manifest path has no parent: {}", manifest_path.display()))?;
    let lockfile_path = manifest_path.with_extension("lock.json");
    let lockfile_path = if lockfile_path.exists() { Some(lockfile_path) } else { None };
    let store = SnapshotStore::new(project_dir);
    let name = format!("auto-before-{operation}");
    let reason = format!("Auto snapshot before {operation}");
    Ok(store.create(
        &name,
        &reason,
        manifest_path,
        lockfile_path.as_ref(),
        changed_files,
    )?)
}

fn save_manifest(path: &Path, manifest: &ProjectManifest) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(manifest)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_project_schema_status,
            migrate_project_schema,
            validate_project,
            get_project_brief,
            update_project_brief,
            list_profiles,
            list_mods,
            search_modrinth_mods,
            preview_modrinth_install,
            get_modrinth_project_icon,
            add_modrinth_mod,
            add_modrinth_mod_with_dependencies,
            add_modrinth_mods_with_dependencies,
            remove_project_mod,
            update_project_mod,
            list_config_files,
            read_config_file,
            write_config_file,
            get_graph,
            get_diagnostics,
            get_resolve_change_plan,
            apply_resolve_action,
            apply_resolve_change_plan,
            resolve_missing_dependencies,
            get_crash_diagnosis,
            create_crash_fix_plan,
            get_history_settings,
            update_history_settings,
            list_project_change_history,
            read_project_history_file,
            create_tracked_history_snapshot,
            rollback_history_file,
            get_project_dir,
            list_snapshots,
            create_snapshot,
            diff_snapshots,
            rollback_snapshot,
            get_snapshot_file_diff,
            validate_modrinth_export,
            generate_release_changelog,
            update_project_version,
            create_release_snapshot,
            export_modrinth_pack,
            export_server_pack,
            export_prism_instance,
            export_curseforge_pack,
            list_release_artifacts,
            create_release_draft,
            generate_lockfile,
            capture_test_run_logs,
            list_test_runs,
            launch_profile,
            import_project,
            open_project_folder,
            delete_project,
            get_home_dir,
            get_minecraft_versions,
            get_loader_versions,
            create_instance,
            find_java_runtimes,
            get_java_version,
            get_default_java_version,
            get_launch_log,
            update_project_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
