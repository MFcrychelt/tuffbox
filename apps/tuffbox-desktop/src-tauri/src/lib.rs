use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    player_name: String,
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
        player_name: profile.player_name.clone().unwrap_or_else(|| "Player".to_string()),
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

#[tauri::command(rename_all = "camelCase")]
async fn sync_mods_folder(path: String) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = std::path::PathBuf::from(&path);
        let mut manifest = ProjectManifest::load_from_path(&manifest_path).map_err(|e| e.to_string())?;

        let mut project_dir = manifest_path.clone();
        if manifest_path.is_file() {
            project_dir.pop();
        }

        // Scan all content folders: mods/, resourcepacks/, shaderpacks/, datapacks/
        let content_dirs: &[(&str, &str, tuffbox_core::manifest::ContentType)] = &[
            ("mods", "jar", tuffbox_core::manifest::ContentType::Mod),
            ("resourcepacks", "zip", tuffbox_core::manifest::ContentType::Resourcepack),
            ("shaderpacks", "zip", tuffbox_core::manifest::ContentType::Shaderpack),
            ("datapacks", "zip", tuffbox_core::manifest::ContentType::Datapack),
        ];

        let provider = tuffbox_core::ModrinthProvider::new();
        let mut any_changes = false;

        for &(dir_name, ext, default_content_type) in content_dirs {
            let dir = project_dir.join(dir_name);
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let file_type = match entry.file_type() {
                    Ok(ft) => ft,
                    Err(_) => continue,
                };
                if !file_type.is_file() {
                    continue;
                }
                if !entry.path().extension().map_or(false, |e| e == ext) {
                    continue;
                }

                let file_name = entry.file_name().to_string_lossy().to_string();
                if manifest.mods.iter().any(|m| m.file_name.as_deref() == Some(&*file_name)) {
                    continue;
                }

                // Try to identify this file against Modrinth by content hash.
                let identified = tuffbox_core::identify_local_jar_via_modrinth(
                    &provider,
                    &entry.path(),
                    tuffbox_core::manifest::Side::Both,
                )
                .ok()
                .flatten();

                let mod_spec = if let Some(mut identified) = identified {
                    identified.file_name = Some(file_name.clone());
                    identified
                } else {
                    let id = file_name
                        .trim_end_matches(&format!(".{}", ext))
                        .to_string();
                    tuffbox_core::manifest::ModSpec {
                        id,
                        name: file_name.clone(),
                        version: "unknown".to_string(),
                        side: tuffbox_core::manifest::Side::Both,
                        source: tuffbox_core::manifest::ModSource {
                            kind: tuffbox_core::manifest::SourceKind::Local,
                            project_id: None,
                            file_id: None,
                            url: None,
                            path: Some(format!("{}/{}", dir_name, file_name)),
                        },
                        file_name: Some(file_name),
                        hashes: None,
                        dependencies: vec![],
                        status: vec![],
                        content_type: default_content_type,
                    }
                };

                manifest.mods.push(mod_spec);
                any_changes = true;
            }
        }

        if any_changes {
            save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        }

        list_mods_impl(&path)
    }).await.map_err(|e| e.to_string())?
}

fn list_mods_impl(path: &str) -> Result<Vec<serde_json::Value>, String> {
    let manifest = ProjectManifest::load_from_path(path).map_err(|e| e.to_string())?;
    let mods = manifest
        .mods
        .into_iter()
        .map(|m| {
            let content_type = match m.content_type {
                tuffbox_core::manifest::ContentType::Mod => "mod",
                tuffbox_core::manifest::ContentType::Resourcepack => "resourcepack",
                tuffbox_core::manifest::ContentType::Shaderpack => "shader",
                tuffbox_core::manifest::ContentType::Datapack => "datapack",
            };
            let icon_url: Option<String> = match &m.source.kind {
                tuffbox_core::manifest::SourceKind::Modrinth => {
                    m.source.project_id.as_ref().map(|pid| {
                        format!("https://cdn.modrinth.com/data/{pid}/icon.png")
                    })
                }
                _ => None,
            };
            serde_json::json!({
                "id": m.id,
                "name": m.name,
                "version": m.version,
                "side": format!("{:?}", m.side).to_lowercase(),
                "source": format!("{:?}", m.source.kind).to_lowercase(),
                "projectId": m.source.project_id,
                "fileName": m.file_name,
                "iconUrl": icon_url,
                "contentType": content_type,
            })
        })
        .collect();
    Ok(mods)
}

#[tauri::command(rename_all = "camelCase")]
async fn list_mods(path: String) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || {
        list_mods_impl(&path)
    }).await.map_err(|e| e.to_string())?
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
    content_type: Option<String>,
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
                project_type: content_type,
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
        save_manifest(&PathBuf::from(&path), &manifest).map_err(|e| e.to_string())?;
        download_project_mods(&PathBuf::from(&path), &manifest);
        Ok(())
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
        download_project_mods(&manifest_path, &manifest);
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
        download_project_mods(&manifest_path, &manifest);
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
    let removed_mod = manifest.mods.iter().find(|m| m.id == mod_id).cloned();
    manifest.mods.retain(|m| m.id != mod_id);
    if manifest.mods.len() == original_len {
        return Err(format!("mod {mod_id} not found in project"));
    }
    save_manifest(&PathBuf::from(&path), &manifest).map_err(|e| e.to_string())?;

    // Remove the physical file too, so a removed entry doesn't linger in
    // its content folder and get loaded by Minecraft anyway on the next
    // launch. Uses the entry's own content type so resourcepacks/shaders
    // are removed from the right folder, not `mods/`.
    if let Some(removed_mod) = removed_mod {
        if let Some(file_name) = removed_mod.file_name {
            if let Some(instance_dir) = tuffbox_core::instance_dir_for_manifest(&PathBuf::from(&path)) {
                let content_dir = tuffbox_core::content_dir_for(&instance_dir, removed_mod.content_type);
                let _ = std::fs::remove_file(content_dir.join(file_name));
            }
        }
    }
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
async fn update_project_mod(path: String, mod_id: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        auto_snapshot(&PathBuf::from(&path), "update-mod").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        update_mod_from_modrinth(&mut manifest, &mod_id).map_err(|e| e.to_string())?;
        save_manifest(&PathBuf::from(&path), &manifest).map_err(|e| e.to_string())?;
        download_project_mods(&PathBuf::from(&path), &manifest);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Returns all available versions for a Modrinth project, filtered by the
/// current Minecraft version and loader so the user only sees versions
/// that are compatible with their project.
#[tauri::command(rename_all = "camelCase")]
async fn get_mod_versions(
    mod_id: String,
    minecraft_version: String,
    loader: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || {
        let provider = tuffbox_core::ModrinthProvider::new();
        let query = ProviderSearchQuery {
            query: None,
            minecraft_version: Some(minecraft_version),
            loader,
            ..Default::default()
        };
        let versions = provider.get_versions(&mod_id, &query).map_err(|e| e.to_string())?;
        Ok(versions
            .into_iter()
            .map(|v| serde_json::json!({
                "id": v.id,
                "versionNumber": v.version_number,
                "gameVersions": v.game_versions,
                "loaders": v.loaders,
            }))
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Changes a mod entry to a specific version (identified by Modrinth
/// version id), downloading the new file and updating metadata in the
/// manifest.
#[tauri::command(rename_all = "camelCase")]
async fn change_mod_version(
    path: String,
    mod_id: String,
    new_version_id: String,
) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "change-mod-version").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;

        let provider = tuffbox_core::ModrinthProvider::new();
        let version_info = provider
            .get_version(&new_version_id)
            .map_err(|e| e.to_string())?;
        let project = provider
            .get_project(&version_info.project_id)
            .map_err(|e| e.to_string())?;
        let file = ProviderFileInfo::primary_file(&version_info)
            .cloned()
            .ok_or_else(|| format!("no primary file for version {}", version_info.id))?;
        let dependencies = provider
            .resolve_dependencies(&version_info.id)
            .unwrap_or_default();

        let idx = manifest
            .mods
            .iter()
            .position(|m| m.id == mod_id)
            .ok_or_else(|| format!("mod {mod_id} not found in project"))?;

        let side = manifest.mods[idx].side;
        manifest.mods[idx] = build_mod_spec(&project, &version_info, file, dependencies, side);

        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods(&manifest_path, &manifest);

        Ok(serde_json::json!({
            "version": version_info.version_number,
            "id": version_info.id,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Scans the `mods/` folder for `.jar` files that appear to be built for a
/// different mod loader than what the project uses (e.g. a Forge mod in a
/// Fabric project), and returns a list of suggestions with the file name
/// and a recommendation.
#[tauri::command(rename_all = "camelCase")]
async fn detect_wrong_loader_mods(
    path: String,
) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let project_dir = PathBuf::from(&path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let mods_dir = project_dir.join("mods");
        let project_loader = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string();
        let mut results = Vec::new();

        let entries = match std::fs::read_dir(&mods_dir) {
            Ok(e) => e,
            Err(_) => return Ok(results),
        };

        let provider = tuffbox_core::ModrinthProvider::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.extension().map_or(false, |e| e == "jar") {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().to_string();
            // Skip if already tracked in manifest
            if manifest.mods.iter().any(|m| m.file_name.as_deref() == Some(&*file_name)) {
                continue;
            }

            // Try to identify via Modrinth hash lookup
            let sha1: String = match tuffbox_core::mc_install::sha1_file(&path) {
                Ok(h) => h,
                Err(_) => continue,
            };

            let identified = provider
                .get_version_by_hash(&sha1)
                .ok()
                .flatten();

            if let Some(version) = identified {
                let jar_loaders: Vec<&str> = version.loaders.iter().map(|s| s.as_str()).collect();
                let is_compatible = jar_loaders.is_empty()
                    || jar_loaders.contains(&project_loader.as_str());

                if !is_compatible && !jar_loaders.is_empty() {
                    results.push(serde_json::json!({
                        "fileName": file_name,
                        "detectedLoader": jar_loaders.join(", "),
                        "projectLoader": project_loader,
                        "recommendation": "disable",
                        "reason": format!(
                            "{} was built for {} but this project uses {}. Disable it (.jar.disabled) or remove it.",
                            file_name, jar_loaders.join(", "), project_loader
                        ),
                    }));
                }
            }
        }
        Ok(results)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Renames a .jar file in mods/ to .jar.disabled so Minecraft won't load it.
#[tauri::command(rename_all = "camelCase")]
async fn disable_wrong_loader_jar(
    path: String,
    file_name: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let project_dir = PathBuf::from(&path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let src = project_dir.join("mods").join(&file_name);
        let dst = project_dir.join("mods").join(format!("{}.disabled", file_name));
        if !src.is_file() {
            return Err(format!("{} not found in mods/", file_name));
        }
        std::fs::rename(&src, &dst).map_err(|e| e.to_string())?;
        Ok(format!("{} → {}.disabled", file_name, file_name))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Removes a specific file from mods/ (used for wrong-loader jar cleanup).
#[tauri::command(rename_all = "camelCase")]
async fn remove_loose_jar(
    path: String,
    file_name: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let project_dir = PathBuf::from(&path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let target = project_dir.join("mods").join(&file_name);
        if !target.is_file() {
            return Err(format!("{} not found in mods/", file_name));
        }
        std::fs::remove_file(&target).map_err(|e| e.to_string())?;
        Ok(format!("Removed {}", file_name))
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

/// Full-text search across all config and script files in the project.
#[tauri::command(rename_all = "camelCase")]
fn search_in_configs(path: String, query: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let roots = ["config", "defaultconfigs", "kubejs", "scripts"];
    let whitelist: &[&str] = &["json","json5","toml","properties","cfg","yaml","yml","js","zs","txt","md","html","css","sh"];
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    fn walk(dir: &Path, cb: &mut dyn FnMut(&Path)) {
        let entries = match std::fs::read_dir(dir) { Ok(e) => e, Err(_) => return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() { walk(&p, cb); } else { cb(&p); }
        }
    }

    for root in &roots {
        let root_dir = project_dir.join(root);
        if !root_dir.is_dir() { continue; }
        walk(&root_dir, &mut |file_path: &Path| {
            if results.len() >= 200 { return; }
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !whitelist.contains(&ext) { return; }
            let Ok(content) = std::fs::read_to_string(file_path) else { return };
            if content.len() > 1024 * 1024 { return; }
            for (line_no, line) in content.lines().enumerate() {
                if line.to_lowercase().contains(&query_lower) {
                    if let Ok(rel) = file_path.strip_prefix(&project_dir) {
                        results.push(serde_json::json!({
                            "path": rel.to_string_lossy(),
                            "line": line_no + 1,
                            "text": line.trim().chars().take(200).collect::<String>(),
                        }));
                    }
                    if results.len() >= 200 { return; }
                }
            }
        });
        if results.len() >= 200 { break; }
    }
    Ok(results)
}

/// ── Launch statistics (like NitroLaunch stats plugin) ──────────

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct LaunchStats {
    #[serde(default)]
    launches: u64,
    #[serde(default)]
    crashes: u64,
    #[serde(default)]
    last_launch: Option<String>,
    #[serde(default)]
    total_playtime_seconds: u64,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
struct ProjectStats {
    #[serde(default)]
    instances: std::collections::HashMap<String, LaunchStats>,
}

fn stats_path(project_dir: &std::path::Path) -> std::path::PathBuf {
    project_dir.join(".tuffbox").join("stats.json")
}

fn load_stats(project_dir: &std::path::Path) -> ProjectStats {
    let p = stats_path(project_dir);
    std::fs::read_to_string(&p).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_stats(project_dir: &std::path::Path, stats: &ProjectStats) -> Result<(), String> {
    let p = stats_path(project_dir);
    if let Some(par) = p.parent() { std::fs::create_dir_all(par).map_err(|e| e.to_string())?; }
    std::fs::write(&p, serde_json::to_string_pretty(stats).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Records a launch event in the project stats.
#[tauri::command(rename_all = "camelCase")]
fn record_launch(path: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let mut stats = load_stats(&project_dir);
    let entry = stats.instances.entry("client".into()).or_default();
    entry.launches += 1;
    entry.last_launch = Some(tuffbox_core::time_util::rfc3339_now());
    save_stats(&project_dir, &stats)
}

/// Records a crash event in the project stats.
#[tauri::command(rename_all = "camelCase")]
fn record_crash(path: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let mut stats = load_stats(&project_dir);
    let entry = stats.instances.entry("client".into()).or_default();
    entry.crashes += 1;
    save_stats(&project_dir, &stats)
}

/// Returns launch/crash statistics for the project.
#[tauri::command(rename_all = "camelCase")]
fn get_launch_stats(path: String) -> Result<serde_json::Value, String> {
    let project_dir = manifest_parent(&path)?;
    let stats = load_stats(&project_dir);
    let mut all_launches = 0u64;
    let mut all_crashes = 0u64;
    let mut last = None;
    for (_id, inst) in &stats.instances {
        all_launches += inst.launches;
        all_crashes += inst.crashes;
        if inst.last_launch.is_some() { last = inst.last_launch.clone(); }
    }
    Ok(serde_json::json!({
        "totalLaunches": all_launches,
        "totalCrashes": all_crashes,
        "lastLaunch": last,
        "byProfile": stats.instances.iter().map(|(id, inst)| serde_json::json!({
            "id": id, "launches": inst.launches, "crashes": inst.crashes,
            "lastLaunch": inst.last_launch,
        })).collect::<Vec<_>>(),
    }))
}


#[tauri::command(rename_all = "camelCase")]
fn get_manifest_schema(path: String) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "schemaVersion": manifest.schema_version,
        "hasBrief": manifest.brief.is_some(),
        "modCount": manifest.mods.len(),
        "profileCount": manifest.profiles.len(),
    }))
}

/// ── Offline validation / project health report ──────────────────────

/// Runs a set of offline checks on a project without launching Minecraft:
/// JSON syntax errors in config files, missing dependency edges in the graph,
/// circular dependency warnings, and a generated testing checklist.
#[tauri::command(rename_all = "camelCase")]
fn run_project_validation(path: String) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let graph = DependencyGraph::from_manifest(&manifest);
    let diagnostics = Resolver::analyze_project(&manifest, &graph);
    let project_dir = manifest_parent(&path)?;

    // Check JSON files for parse errors
    let mut json_errors: Vec<serde_json::Value> = Vec::new();
    let roots = ["config", "defaultconfigs", "kubejs", "scripts"];
    for root in &roots {
        let dir = project_dir.join(root);
        if !dir.is_dir() { continue; }
        fn walk_json(dir: &Path, acc: &mut Vec<serde_json::Value>) {
            let entries = match std::fs::read_dir(dir) { Ok(e) => e, Err(_) => return };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() { walk_json(&p, acc); continue; }
                if p.extension().map_or(false, |e| e == "json") {
                    if let Ok(content) = std::fs::read_to_string(&p) {
                        if content.len() < 512 * 1024 {
                            if let Err(e) = serde_json::from_str::<serde_json::Value>(&content) {
                                acc.push(serde_json::json!({
                                    "path": p.strip_prefix(dir.parent().unwrap_or(&dir)).unwrap_or(&p).to_string_lossy(),
                                    "error": e.to_string(),
                                }));
                            }
                        }
                    }
                }
                if acc.len() >= 50 { return; }
            }
        }
        walk_json(&dir, &mut json_errors);
    }

    // Check for circular dependencies in the manifest mod list
    let mut circular: Vec<Vec<String>> = Vec::new();
    {
        let mut seen = std::collections::HashSet::new();
        for m in &manifest.mods {
            for dep in &m.dependencies {
                if dep.kind == tuffbox_core::manifest::DependencyKind::Requires {
                    let target = &dep.target;
                    if let Some(target_mod) = manifest.mods.iter().find(|t| t.id == *target) {
                        if target_mod.dependencies.iter().any(|d| d.target == m.id) {
                            let pair = vec![m.id.clone(), target.clone()];
                            let key = if m.id < *target { (m.id.clone(), target.clone()) } else { (target.clone(), m.id.clone()) };
                            let key_str = format!("{}<=>{}", key.0, key.1);
                            if seen.insert(key_str) {
                                circular.push(pair);
                            }
                        }
                    }
                }
            }
        }
    }

    let error_diags: Vec<_> = diagnostics.iter().filter(|d| d.severity == tuffbox_core::DiagnosticSeverity::Error).collect();
    let warning_diags: Vec<_> = diagnostics.iter().filter(|d| d.severity == tuffbox_core::DiagnosticSeverity::Warning).collect();
    let mods_without_source = manifest.mods.iter().filter(|m| m.source.url.is_none()).count();
    let mods_without_hash = manifest.mods.iter().filter(|m| m.hashes.as_ref().and_then(|h| h.sha1.as_ref()).is_none()).count();

    Ok(serde_json::json!({
        "passed": error_diags.is_empty() && json_errors.is_empty() && circular.is_empty(),
        "jsonErrors": json_errors,
        "graphErrors": error_diags.len(),
        "graphWarnings": warning_diags.len(),
        "graphErrorList": error_diags.iter().map(|d| serde_json::json!({"code": d.code, "message": d.message})).collect::<Vec<_>>(),
        "graphWarningList": warning_diags.iter().take(10).map(|d| serde_json::json!({"code": d.code, "message": d.message})).collect::<Vec<_>>(),
        "circularDeps": circular,
        "modsWithoutSource": mods_without_source,
        "modsWithoutHash": mods_without_hash,
        "totalMods": manifest.mods.len(),
        "totalProfiles": manifest.profiles.len(),
    }))
}



/// ── Batch update manager ────────────────────────────────────────────

/// Checks every Modrinth-sourced mod in the project for available updates,
/// comparing the installed version against the latest compatible version.
/// Returns a list with update info for each mod that could be updated.
#[tauri::command(rename_all = "camelCase")]
async fn check_mod_updates(path: String) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let provider = tuffbox_core::ModrinthProvider::new();
        let mut updates = Vec::new();

        let query = ProviderSearchQuery {
            query: None,
            minecraft_version: Some(manifest.minecraft.version.clone()),
            loader: Some(tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string()),
            ..Default::default()
        };

        for m in &manifest.mods {
            if m.source.kind != SourceKind::Modrinth { continue; }
            let Some(project_id) = &m.source.project_id else { continue; };

            // Get latest compatible version
            let latest = match provider.get_versions(project_id, &query) {
                Ok(versions) => versions.into_iter().next(),
                Err(_) => None,
            };
            let Some(latest) = latest else { continue; };

            if latest.version_number != m.version {
                let file = ProviderFileInfo::primary_file(&latest).cloned();
                updates.push(serde_json::json!({
                    "modId": m.id,
                    "name": m.name,
                    "currentVersion": m.version,
                    "latestVersion": latest.version_number,
                    "versionId": latest.id,
                    "fileName": file.as_ref().map(|f| &f.filename),
                    "gameVersions": latest.game_versions,
                    "loaders": latest.loaders,
                }));
            }
        }
        Ok(updates)
    }).await.map_err(|e| e.to_string())?
}

/// Applies all available mod updates at once (batch update), creating
/// a single auto-snapshot before the changes.
#[tauri::command(rename_all = "camelCase")]
async fn update_all_mods(path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "batch-update-all").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let mut updated = Vec::new();

        let provider = tuffbox_core::ModrinthProvider::new();
        let query = ProviderSearchQuery {
            query: None,
            minecraft_version: Some(manifest.minecraft.version.clone()),
            loader: Some(tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string()),
            ..Default::default()
        };

        for idx in 0..manifest.mods.len() {
            if manifest.mods[idx].source.kind != SourceKind::Modrinth { continue; }
            let Some(project_id) = manifest.mods[idx].source.project_id.clone() else { continue; };
            let current_version = manifest.mods[idx].version.clone();

            let latest = match provider.get_versions(&project_id, &query) {
                Ok(versions) => versions.into_iter().next(),
                Err(_) => None,
            };
            let Some(latest) = latest else { continue; };
            if latest.version_number == current_version { continue; }

            let file = ProviderFileInfo::primary_file(&latest).cloned();
            let Some(file) = file else { continue; };

            let project = provider.get_project(&project_id).map_err(|e| e.to_string())?;
            let dependencies = provider.resolve_dependencies(&latest.id).unwrap_or_default();
            let side = manifest.mods[idx].side;
            manifest.mods[idx] = build_mod_spec(&project, &latest, file, dependencies, side);
            updated.push(manifest.mods[idx].name.clone());
        }

        if !updated.is_empty() {
            save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
            download_project_mods(&manifest_path, &manifest);
        }
        Ok(updated)
    }).await.map_err(|e| e.to_string())?
}



/// ── Performance audit ────────────────────────────────────────────────

/// Runs a performance audit on the project configs, checking for common
/// settings that degrade Minecraft performance. Returns a list of
/// recommendations with config file paths and suggested changes.
#[tauri::command(rename_all = "camelCase")]
fn audit_performance(path: String) -> Result<Vec<serde_json::Value>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mut findings = Vec::new();

    // Check Sodium/Embeddium settings for common performance traps
    let config_dir = project_dir.join("config");
    if config_dir.is_dir() {
        // Check sodium-options.json for common issues
        for (filename, check_fn) in SODIUM_CHECKS {
            let fp = config_dir.join(filename);
            if !fp.is_file() { continue; }
            if let Ok(content) = std::fs::read_to_string(&fp) {
                check_fn(&content, &mut findings);
            }
        }
        // Check Forge server config for render distance / spawn limits
        for (pattern, check_fn) in FORGE_PERF_CHECKS {
            if let Ok(entries) = std::fs::read_dir(&config_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.contains(pattern) {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            check_fn(&content, &name, &mut findings);
                        }
                    }
                }
            }
        }
    }

    // Check if performance mods are missing
    let mod_slugs: std::collections::HashSet<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    let perf_mods = ["sodium", "embeddium", "lithium", "ferrite-core", "immediatelyfast", "modernfix", "memoryleakfix", "smoothboot", "entityculling"];
    let mut missing_perf = Vec::new();
    for pm in perf_mods {
        if !mod_slugs.contains(pm) {
            missing_perf.push(pm);
        }
    }
    if !missing_perf.is_empty() {
        findings.push(serde_json::json!({
            "severity": if missing_perf.len() >= 3 { "warning" } else { "info" },
            "code": "MISSING_PERFORMANCE_MODS",
            "message": format!("Consider adding performance mods: {}", missing_perf.join(", ")),
            "file": null,
        }));
    }

    // Check JVM args
    let profile = manifest.profiles.iter().find(|p| p.id == "client")
        .or_else(|| manifest.profiles.first());
    if let Some(profile) = profile {
        let jvm = profile.jvm_args.join(" ");
        if !jvm.contains("-XX:+UseG1GC") && !jvm.contains("-XX:+UseZGC") && !jvm.contains("-XX:+UseShenandoahGC") {
            findings.push(serde_json::json!({
                "severity": "info",
                "code": "NO_GC_SETTING",
                "message": "No GC specified in JVM args. Consider -XX:+UseG1GC for Minecraft.",
                "file": null,
            }));
        }
        if profile.memory_mb.unwrap_or(4096) < 3072 {
            findings.push(serde_json::json!({
                "severity": "warning",
                "code": "LOW_MEMORY",
                "message": format!("Memory is set to {} MB — 4-8 GB is recommended for modded Minecraft.", profile.memory_mb.unwrap_or(4096)),
                "file": null,
            }));
        }
    }

    Ok(findings)
}

/// Sodium config checks: (filename, fn(&content, &mut findings))
const SODIUM_CHECKS: &[(&str, fn(&str, &mut Vec<serde_json::Value>))] = &[
    ("sodium-options.json", |c: &str, f: &mut Vec<serde_json::Value>| {
        // Check if vsync is enabled (can cap FPS)
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(c) {
            if v.get("quality").and_then(|q| q.get("use_block_face_culling")).and_then(|x| x.as_str()) == Some("1") { return; }
            if v.get("advanced").and_then(|a| a.get("use_chunk_multidraw")).and_then(|x| x.as_bool()) == Some(false) {
                f.push(serde_json::json!({"severity":"info","code":"CHUNK_MULTIDRAW_OFF","message":"Chunk multidraw is disabled in Sodium; enable it for better FPS.","file":"config/sodium-options.json"}));
            }
            let render_dist = v.get("quality").and_then(|q| q.get("render_distance")).and_then(|x| x.as_str()).unwrap_or("16");
            if render_dist.parse::<u32>().unwrap_or(16) > 16 {
                f.push(serde_json::json!({"severity":"warning","code":"HIGH_RENDER_DISTANCE","message":format!("Render distance is {render_dist} — consider lowering to 12-16 for modded."),"file":"config/sodium-options.json"}));
            }
        }
    }),
];

/// Forge/NeoForge config checks: (filename_pattern, fn(&content, &filename, &mut findings))
const FORGE_PERF_CHECKS: &[(&str, fn(&str, &str, &mut Vec<serde_json::Value>))] = &[
    ("forge-server", |c: &str, name: &str, f: &mut Vec<serde_json::Value>| {
        if c.contains("max-tick-time") {
            for line in c.lines() {
                if line.contains("max-tick-time") {
                    let val = line.split('=').last().unwrap_or("").trim();
                    if val == "-1" {
                        f.push(serde_json::json!({"severity":"warning","code":"MAX_TICK_TIME_DISABLED","message":"max-tick-time is -1 (off) — the server won't crash on overload but may become permanently unresponsive.","file":format!("config/{name}")}));
                    }
                }
            }
        }
        // Check entity spawning limits
        for search in &["max-entity-collisions", "spawn-limits", "max-breed", "despawn-ranges"] {
            if c.contains(search) {
                f.push(serde_json::json!({"severity":"info","code":"SERVER_PERF_CONFIG_PRESENT","message":format!("Server performance config detected: {search}. Review limits for your player count."),"file":format!("config/{name}")}));
            }
        }
    }),
];



/// ── Ore generation scanner ──────────────────────────────────────────

/// Scans the project configs for ore-generation settings using both the
/// builtin knowledge base and heuristics, returning a list of detected
/// ore gen toggle keys with estimated values.
#[tauri::command(rename_all = "camelCase")]
fn scan_ore_generation(path: String) -> Result<Vec<serde_json::Value>, String> {
    let _manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mut config_contents = Vec::new();

    // Gather all config files
    for root in &["config", "defaultconfigs"] {
        let dir = project_dir.join(root);
        if !dir.is_dir() { continue; }
        fn walk(dir: &std::path::Path, acc: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = entry.path();
                if p.is_dir() { walk(&p, acc); continue; }
                if let Some(ext) = p.extension() {
                    if ext == "toml" || ext == "json" || ext == "cfg" || ext == "json5" {
                        if let Ok(content) = std::fs::read_to_string(&p) {
                            if content.len() < 512 * 1024 {
                                if let Ok(rel) = p.strip_prefix(dir.parent().unwrap_or(&dir)) {
                                    acc.push((rel.to_string_lossy().to_string(), content));
                                }
                            }
                        }
                    }
                }
            }
        }
        walk(&dir, &mut config_contents);
    }

    // Run heuristics scan
    let heuristic_hits = tuffbox_core::knowledge::heuristics::scan_configs_for_ore_gen(&config_contents);
    
    // Cross-reference with builtin knowledge base
    let mut results = Vec::new();
    for hit in &heuristic_hits {
        // Check if knowledge base has this mod
        let kb_hint = tuffbox_core::knowledge::builtin::ModKnowledgeEntry::lookup(&hit.resource_name);
        let confidence = match (hit.confidence, kb_hint.is_some()) {
            (_, true) => "high",
            (tuffbox_core::knowledge::heuristics::HeuristicConfidence::Medium, _) => "medium",
            _ => "low",
        };
        results.push(serde_json::json!({
            "resource": hit.resource_name,
            "configFile": hit.config_file,
            "enabledKey": hit.enabled_key,
            "enabledValue": hit.enabled_value,
            "veinSize": hit.vein_size,
            "minHeight": hit.min_height,
            "maxHeight": hit.max_height,
            "spawnsPerChunk": hit.spawns_per_chunk,
            "confidence": confidence,
            "knownMod": kb_hint.map(|k| k.name.clone()),
        }));
    }
    Ok(results)
}

/// ── Duplicate detection ─────────────────────────────────────────────

/// Scans installed mods for duplicate resources (e.g., two mods both
/// adding "tin_ingot") and returns resolution suggestions with
/// generated KubeJS/CraftTweaker scripts.
#[tauri::command(rename_all = "camelCase")]
fn detect_duplicate_items(path: String) -> Result<Vec<serde_json::Value>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mods_dir = project_dir.join("mods");
    
    // Build mod→items mapping from what we can extract
    let mut mod_items: Vec<(String, Vec<String>)> = Vec::new();
    // Use knowledge base programmatic items as a starting point
    for entry in tuffbox_core::knowledge::builtin::ModKnowledgeEntry::builtin() {
        if manifest.mods.iter().any(|m| m.id == entry.slug) && !entry.programmatic_items.is_empty() {
            mod_items.push((entry.slug.clone(), entry.programmatic_items.clone()));
        }
    }

    // Also try to read known item registry from mod jars if available
    if mods_dir.is_dir() {
        for entry in std::fs::read_dir(&mods_dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.extension().map_or(true, |e| e != "jar") { continue; }
            // Quick pattern-match from jar filename
            let name = entry.file_name().to_string_lossy().to_string();
            let id = name.trim_end_matches(".jar").to_string();
            // Try to classify jar filename fragments as potential items
            let parts: Vec<&str> = id.split('-').collect();
            let mut items = Vec::new();
            for part in &parts {
                if let Some((mat, ty)) = tuffbox_core::knowledge::heuristics::classify_item(part) {
                    items.push(format!("{}_{}", mat, ty));
                }
            }
            if !items.is_empty() {
                mod_items.push((id, items));
            }
        }
    }

    // Run duplicate detection
    let groups = tuffbox_core::knowledge::heuristics::detect_duplicate_groups(&mod_items);
    let resolutions = tuffbox_core::unified::duplicate::resolve_duplicates(&groups);

    let mut results = Vec::new();
    for (idx, res) in resolutions.iter().enumerate() {
        results.push(serde_json::json!({
            "id": format!("dedup-{}", idx),
            "material": res.material,
            "itemType": res.item_type,
            "keepItem": res.keep,
            "removeItems": res.remove,
            "kubejsScript": res.to_kubejs6(),
            "crafttweakerScript": res.to_crafttweaker(),
        }));
    }
    Ok(results)
}



/// ── Almost Unified config generator ────────────────────────────────

/// Generates an Almost Unified config (unify.json) tailored for the
/// project's installed mods, and optionally writes it to disk.
#[tauri::command(rename_all = "camelCase")]
fn generate_unify_config(path: String, save: Option<bool>) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let mod_slugs: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    let config = tuffbox_core::unified::unify_config::UnifyConfig::for_project(&mod_slugs);

    if save.unwrap_or(false) {
        let project_dir = manifest_parent(&path)?;
        let config_path = project_dir.join("config").join("almostunified").join("unify.json");
        config.save_to(&config_path).map_err(|e| e.to_string())?;
    }

    let kubejs = tuffbox_core::unified::unify_config::generate_unification_script(&[]);
    Ok(serde_json::json!({
        "config": config,
        "expandedTagsCount": config.expanded_tags().len(),
        "materialCount": config.materials.len(),
        "priorityCount": config.mod_priorities.len(),
        "kubejsScript": kubejs,
    }))
}

/// ── Crash Assistant analysis ───────────────────────────────────────

/// Runs the full Crash Assistant analysis on the project, detecting
/// common crash patterns: wrong Java, mixin failures, missing mods,
/// Intel CPU bugs, integrated GPU, corrupted installs, and more.
#[tauri::command(rename_all = "camelCase")]
#[allow(dead_code)]
fn run_crash_assistant(path: String) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;

    let installed_mods: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    let mut crash_content = Vec::new();
    let mut latest_log = String::new();
    let mut launcher_log = String::new();

    // Read crash reports
    let crash_dir = project_dir.join("crash-reports");
    if crash_dir.is_dir() {
        for entry in std::fs::read_dir(&crash_dir).into_iter().flatten().flatten() {
            if entry.path().extension().map_or(false, |e| e == "txt") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if content.len() < 4 * 1024 * 1024 {
                        crash_content.push(content);
                    }
                }
            }
        }
    }

    // Read latest.log
    let latest_path = project_dir.join("logs").join("latest.log");
    if latest_path.is_file() {
        latest_log = tuffbox_core::process::read_log_tail(&latest_path, 900).unwrap_or_default();
    }

    // Read launcher.log
    let launcher_path = project_dir.join("logs").join("launcher.log");
    if launcher_path.is_file() {
        launcher_log = std::fs::read_to_string(&launcher_path).unwrap_or_default();
    }

    // Gather system info
    let java_path = manifest.java.as_ref().and_then(|j| j.path.clone()).unwrap_or_default();
    let java_version = if !java_path.is_empty() {
        tuffbox_core::jre::check_java_at_path(&std::path::PathBuf::from(&java_path))
            .map(|r| r.version).unwrap_or_default()
    } else {
        tuffbox_core::jre::find_all_runtimes().ok().and_then(|r| r.into_iter().next())
            .map(|r| r.version).unwrap_or_default()
    };

    let ctx = tuffbox_core::crash_assistant::AnalysisCtx {
        crash_content: crash_content,
        latest_log: latest_log,
        launcher_log: launcher_log,
        installed_mods: installed_mods.clone(),
        previous_mods: Vec::new(),
        java_version,
        java_vendor: String::new(),
        os_name: std::env::consts::OS.to_string(),
        mc_version: manifest.minecraft.version.clone(),
        loader: format!("{:?}", manifest.loader.kind).to_lowercase(),
        loader_version: manifest.loader.version.clone(),
        cpu_name: String::new(),
        gpu_names: Vec::new(),
        total_ram_mb: 0,
        is_offline: false,
        win_events: Vec::new(),
    };

    let report = tuffbox_core::crash_assistant::run_full_analysis(&ctx);

    Ok(serde_json::json!({
        "findings": report.findings.iter().map(|f| serde_json::json!({
            "severity": format!("{:?}", f.severity).to_lowercase(),
            "code": f.code,
            "title": f.title,
            "description": f.description,
            "autoFix": f.auto_fix,
            "references": f.references,
        })).collect::<Vec<_>>(),
        "supportMessageDiscord": report.support_message_discord,
        "supportMessageGithub": report.support_message_github,
        "modsAdded": report.mods_added,
        "modsRemoved": report.mods_removed,
        "suspectedMods": report.suspected_mods,
        "findingsCount": report.findings.len(),
    }))
}



/// ── Package/Class Finder + Jdeps (Crash Assistant tools) ──────────

/// Searches all mod JARs to find which one contains a given Java class.
/// This mirrors Crash Assistant's Package/Class Finder GUI tool.
#[tauri::command(rename_all = "camelCase")]
fn find_class_in_mods(path: String, class_name: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let mods_dir = project_dir.join("mods");
    let results = tuffbox_core::crash_assistant::find_class_in_mods(&class_name, &mods_dir);
    Ok(results.into_iter().map(|r| serde_json::json!({
        "className": r.class_name,
        "modId": r.mod_id,
        "modName": r.mod_name,
    })).collect())
}

/// Searches all mod JARs to find which mods depend on a given class
/// (Jdeps analysis tool from Crash Assistant).
#[tauri::command(rename_all = "camelCase")]
fn find_dependents_on_class(path: String, class_name: String) -> Result<Vec<serde_json::Value>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mods_dir = project_dir.join("mods");
    let installed: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    let results = tuffbox_core::crash_assistant::find_mods_depending_on_class(&class_name, &mods_dir, &installed);
    Ok(results.into_iter().map(|r| serde_json::json!({
        "className": r.class_name,
        "modId": r.mod_id,
        "modName": r.mod_name,
    })).collect())
}

/// Runs the full Crash Assistant analysis and also includes MCreator
/// mod list, class finder results from crash logs, and Jdeps results.
#[tauri::command(rename_all = "camelCase")]
fn run_crash_assistant_full(path: String) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mods_dir = project_dir.join("mods");

    let installed: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    let mut crash_content = Vec::new();
    let mut latest_log = String::new();
    let mut launcher_log = String::new();

    let cd = project_dir.join("crash-reports");
    if cd.is_dir() {
        for e in std::fs::read_dir(&cd).into_iter().flatten().flatten() {
            if e.path().extension().map_or(false, |e| e=="txt") {
                if let Ok(ct) = std::fs::read_to_string(e.path()) {
                    if ct.len() < 4*1024*1024 { crash_content.push(ct); }
                }
            }
        }
    }
    let lp = project_dir.join("logs").join("latest.log");
    if lp.is_file() { latest_log = tuffbox_core::process::read_log_tail(&lp,900).unwrap_or_default(); }
    let la = project_dir.join("logs").join("launcher.log");
    if la.is_file() { launcher_log = std::fs::read_to_string(&la).unwrap_or_default(); }

    let jv = manifest.java.as_ref().and_then(|j| j.path.clone()).unwrap_or_default();
    let java_version = if !jv.is_empty() {
        tuffbox_core::jre::check_java_at_path(&PathBuf::from(&jv)).map(|r|r.version).unwrap_or_default()
    } else { tuffbox_core::jre::find_all_runtimes().ok().and_then(|r|r.into_iter().next()).map(|r|r.version).unwrap_or_default() };

    let ctx = tuffbox_core::crash_assistant::AnalysisCtx {
        crash_content, latest_log, launcher_log,
        installed_mods: installed.clone(), previous_mods: Vec::new(),
        java_version, java_vendor: String::new(), os_name: std::env::consts::OS.to_string(),
        mc_version: manifest.minecraft.version.clone(),
        loader: format!("{:?}", manifest.loader.kind).to_lowercase(),
        loader_version: manifest.loader.version.clone(),
        cpu_name: String::new(), gpu_names: Vec::new(), total_ram_mb: 0,
        is_offline: false, win_events: Vec::new(),
    };

    let report = tuffbox_core::crash_assistant::run_full_analysis(&ctx);

    // Also run class finder on any NoClassDefFoundError classes found
    let mut class_finder = Vec::new();
    let combined = ctx.crash_content.join("
") + "
" + &ctx.latest_log;
    for line in combined.lines() {
        if line.contains("NoClassDefFoundError")||line.contains("ClassNotFoundException") {
            if let Some(cls) = line.split(": ").nth(1).and_then(|s| s.split_whitespace().next()) {
                if cls.len()>5 && cls.len()<200 && cls.contains('.') {
                    let matches = tuffbox_core::crash_assistant::find_class_in_mods(cls, &mods_dir);
                    for m in matches { class_finder.push(serde_json::json!({"className":m.class_name,"modId":m.mod_id,"modName":m.mod_name})); }
                }
            }
        }
    }
    class_finder.truncate(20);

    Ok(serde_json::json!({
        "findings": report.findings.iter().map(|f| serde_json::json!({
            "severity": f.severity,"code": f.code,"title": f.title,
            "description": f.description,"autoFix": f.auto_fix,"references": f.references,
        })).collect::<Vec<_>>(),
        "supportMessageDiscord": report.support_message_discord,
        "supportMessageGithub": report.support_message_github,
        "modsAdded": report.mods_added,"modsRemoved": report.mods_removed,
        "suspectedMods": report.suspected_mods,
        "mcreatorMods": report.mcreator_mods,
        "classFinderResults": class_finder,
        "findingsCount": report.findings.len(),
    }))
}



/// ── Mod compatibility checker ──────────────────────────────────────

/// Scans installed mods against the knowledge base to find known
/// compatibility issues: conflicts, missing dependencies, wrong-loader
/// mods, and version mismatches.
#[tauri::command(rename_all = "camelCase")]
fn check_mod_compatibility(path: String) -> Result<Vec<serde_json::Value>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mut issues = Vec::new();

    let mods: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();

    // Check known conflicts from knowledge base
    for a in &mods {
        for b in &mods {
            if a < b {
                if let Some(reason) = tuffbox_core::knowledge::check_known_conflict(a, b) {
                    issues.push(serde_json::json!({
                        "severity": "error", "code": "KNOWN_CONFLICT",
                        "message": reason,
                        "mods": [a, b],
                    }));
                }
            }
        }
    }

    // Check wrong-loader mods via heuristic
    let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
    let mods_dir = project_dir.join("mods");
    let provider = tuffbox_core::ModrinthProvider::new();
    if mods_dir.is_dir() {
        for entry in std::fs::read_dir(&mods_dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.extension().map_or(true, |e| e != "jar") { continue; }
            if let Ok(sha1) = tuffbox_core::mc_install::sha1_file(&p) {
                if let Ok(Some((_proj, ver))) = provider.identify_local_jar(&sha1) {
                    let loaders: Vec<&str> = ver.loaders.iter().map(|s| s.as_str()).collect();
                    if !loaders.is_empty() && !loaders.contains(&loader.as_str()) {
                        issues.push(serde_json::json!({
                            "severity": "warning", "code": "WRONG_LOADER",
                            "message": format!("{} is for {} but project uses {}", entry.file_name().to_string_lossy(), loaders.join(","), loader),
                            "mods": [entry.file_name().to_string_lossy()],
                        }));
                    }
                }
            }
        }
    }

    // Run graph diagnostics
    let graph = DependencyGraph::from_manifest(&manifest);
    let diags = Resolver::analyze_project(&manifest, &graph);
    for d in &diags {
        issues.push(serde_json::json!({
            "severity": format!("{:?}", d.severity).to_lowercase(),
            "code": d.code,
            "message": d.message,
            "mods": d.related_nodes.iter().map(|n| n.0.clone()).collect::<Vec<_>>(),
        }));
    }

    Ok(issues)
}

/// Compares two modpacks and returns a diff of mods, versions, and settings.
#[tauri::command(rename_all = "camelCase")]
fn compare_modpacks(
    path_a: String,
    path_b: String,
) -> Result<serde_json::Value, String> {
    let ma = ProjectManifest::load_from_path(&path_a).map_err(|e| e.to_string())?;
    let mb = ProjectManifest::load_from_path(&path_b).map_err(|e| e.to_string())?;

    let mods_a: std::collections::HashSet<String> = ma.mods.iter().map(|m| m.id.clone()).collect();
    let mods_b: std::collections::HashSet<String> = mb.mods.iter().map(|m| m.id.clone()).collect();

    let only_a: Vec<&String> = mods_a.difference(&mods_b).collect();
    let only_b: Vec<&String> = mods_b.difference(&mods_a).collect();
    let common: Vec<&String> = mods_a.intersection(&mods_b).collect();

    // Version differences for common mods
    let mut version_diffs = Vec::new();
    for id in &common {
        let va = ma.mods.iter().find(|m| m.id == **id).map(|m| m.version.clone());
        let vb = mb.mods.iter().find(|m| m.id == **id).map(|m| m.version.clone());
        if va != vb {
            version_diffs.push(serde_json::json!({"id": id, "versionA": va, "versionB": vb}));
        }
    }

    Ok(serde_json::json!({
        "nameA": ma.project.name,
        "nameB": mb.project.name,
        "mcVersionA": ma.minecraft.version,
        "mcVersionB": mb.minecraft.version,
        "loaderA": format!("{:?} {}", ma.loader.kind, ma.loader.version),
        "loaderB": format!("{:?} {}", mb.loader.kind, mb.loader.version),
        "modsOnlyInA": only_a,
        "modsOnlyInB": only_b,
        "commonMods": common.len(),
        "versionDiffs": version_diffs,
        "totalModsA": mods_a.len(),
        "totalModsB": mods_b.len(),
    }))
}


/// ── Backup system (like NitroLaunch backup plugin) ──────────────

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BackupIndex {
    backups: Vec<BackupEntry>,
    max_count: u32,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct BackupEntry {
    id: String,
    name: String,
    created_at: String,
    size_bytes: u64,
    manifest_snapshot: bool,
}

fn backup_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("backups")
}

fn load_backup_index(project_dir: &Path) -> BackupIndex {
    let p = backup_dir(project_dir).join("index.json");
    std::fs::read_to_string(&p).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(BackupIndex { backups: vec![], max_count: 20 })
}

fn save_backup_index(project_dir: &Path, idx: &BackupIndex) -> Result<(), String> {
    let d = backup_dir(project_dir);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::fs::write(d.join("index.json"), serde_json::to_string_pretty(idx).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

/// Creates a full backup of the project (mods, configs, resourcepacks,
/// shaderpacks, manifest + lockfile) as a zip archive.
#[tauri::command(rename_all = "camelCase")]
fn create_project_backup(path: String, name: Option<String>) -> Result<serde_json::Value, String> {
    let project_dir = manifest_parent(&path)?;
    let dir = backup_dir(&project_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let backup_name = name.unwrap_or_else(|| "manual-backup".into());
    let id = format!("{}-{}", backup_name.replace(' ', "-"), tuffbox_core::time_util::compact_now());
    let zip_path = dir.join(format!("{}.zip", id));

    let output = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(output);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut total_size: u64 = 0;
    for folder in &["mods", "config", "defaultconfigs", "kubejs", "scripts", "resourcepacks", "shaderpacks", "datapacks"] {
        let d = project_dir.join(folder);
        if d.is_dir() {
            fn add_dir(zip: &mut zip::ZipWriter<std::fs::File>, opts: zip::write::SimpleFileOptions, base: &Path, dir: &Path, size: &mut u64) -> Result<(), String> {
                for e in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
                    let e = e.map_err(|e| e.to_string())?;
                    let p = e.path();
                    if p.is_dir() { add_dir(zip, opts, base, &p, size)?; }
                    else if p.is_file() {
                        if let Ok(meta) = p.metadata() { *size += meta.len(); }
                        let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                        zip.start_file(rel, opts).map_err(|e| e.to_string())?;
                        zip.write_all(&std::fs::read(&p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                    }
                }
                Ok(())
            }
            add_dir(&mut zip, opts, &project_dir, &d, &mut total_size)?;
        }
    }

    // Also backup manifest and lockfile
    let mainfest = project_dir.join("project.tuffbox.json");
    if mainfest.is_file() {
        zip.start_file("project.tuffbox.json", opts).map_err(|e| e.to_string())?;
        zip.write_all(&std::fs::read(&mainfest).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;

    // Update index
    let mut idx = load_backup_index(&project_dir);
    idx.backups.push(BackupEntry {
        id: id.clone(), name: backup_name.clone(),
        created_at: tuffbox_core::time_util::rfc3339_now(),
        size_bytes: total_size, manifest_snapshot: true,
    });
    // Trim old backups
    while idx.backups.len() > idx.max_count as usize {
        let old = idx.backups.remove(0);
        let _ = std::fs::remove_file(dir.join(format!("{}.zip", old.id)));
    }
    save_backup_index(&project_dir, &idx)?;

    Ok(serde_json::json!({
        "id": id, "name": backup_name, "path": zip_path.to_string_lossy(),
        "sizeBytes": total_size, "createdAt": tuffbox_core::time_util::rfc3339_now(),
    }))
}

/// Lists all project backups.
#[tauri::command(rename_all = "camelCase")]
fn list_backups(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let idx = load_backup_index(&project_dir);
    Ok(idx.backups.into_iter().rev().map(|b| serde_json::json!({
        "id": b.id, "name": b.name, "createdAt": b.created_at,
        "sizeBytes": b.size_bytes, "manifestSnapshot": b.manifest_snapshot,
    })).collect())
}

/// Deletes a specific backup.
#[tauri::command(rename_all = "camelCase")]
fn delete_backup(path: String, backup_id: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let dir = backup_dir(&project_dir);
    let zip_path = dir.join(format!("{}.zip", backup_id));
    if zip_path.is_file() { std::fs::remove_file(&zip_path).map_err(|e| e.to_string())?; }
    let mut idx = load_backup_index(&project_dir);
    idx.backups.retain(|b| b.id != backup_id);
    save_backup_index(&project_dir, &idx)
}

/// ── AI Crash Explanation context builder ─────────────────────────

/// Builds a structured AI context from crash data (but does NOT call any
/// LLM — the frontend can send this to any AI provider).
#[tauri::command(rename_all = "camelCase")]
fn build_ai_crash_context(path: String) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;

    let mut crash_content = String::new();
    let cd = project_dir.join("crash-reports");
    if cd.is_dir() {
        for e in std::fs::read_dir(&cd).into_iter().flatten().flatten() {
            if e.path().extension().map_or(false, |e| e=="txt") {
                if let Ok(ct) = std::fs::read_to_string(e.path()) {
                    if ct.len() < 4*1024*1024 { crash_content = ct; break; }
                }
            }
        }
    }
    let latest = project_dir.join("logs").join("latest.log");
    let latest_log = if latest.is_file() { tuffbox_core::process::read_log_tail(&latest, 900).unwrap_or_default() } else { String::new() };

    let jv = manifest.java.as_ref().and_then(|j|j.path.clone()).unwrap_or_default();
    let java_version = if !jv.is_empty() {
        tuffbox_core::jre::check_java_at_path(&PathBuf::from(&jv)).map(|r|r.version).unwrap_or_default()
    } else { "unknown".into() };

    // Get crash assistant findings
    let ctx = tuffbox_core::crash_assistant::AnalysisCtx {
        crash_content: vec![crash_content.clone()],
        latest_log: latest_log.clone(),
        launcher_log: String::new(),
        installed_mods: manifest.mods.iter().map(|m| m.id.clone()).collect(),
        previous_mods: Vec::new(), java_version: java_version.clone(), java_vendor: String::new(),
        os_name: std::env::consts::OS.to_string(),
        mc_version: manifest.minecraft.version.clone(),
        loader: format!("{:?}", manifest.loader.kind).to_lowercase(),
        loader_version: manifest.loader.version.clone(),
        cpu_name: String::new(), gpu_names: Vec::new(), total_ram_mb: 0,
        is_offline: false, win_events: Vec::new(),
    };
    let report = tuffbox_core::crash_assistant::run_full_analysis(&ctx);

    let ai_ctx = tuffbox_core::ai_explanation::CrashAiContext {
        mc_version: manifest.minecraft.version.clone(),
        loader: format!("{:?}", manifest.loader.kind).to_lowercase(),
        loader_version: manifest.loader.version.clone(),
        java_version,
        os: std::env::consts::OS.to_string(),
        installed_mods: manifest.mods.iter().map(|m| m.id.clone()).collect(),
        crash_report_excerpt: crash_content,
        latest_log_excerpt: latest_log,
        suspected_mods: report.suspected_mods,
        crash_assistant_findings: tuffbox_core::ai_explanation::findings_to_ai(&report.findings),
        recent_changes: Vec::new(),
        graph_diagnostics: Vec::new(),
    };

    let prompt = tuffbox_core::ai_explanation::build_crash_prompt(&ai_ctx);
    let triage = tuffbox_core::ai_explanation::build_triage_prompt(&ai_ctx);

    Ok(serde_json::json!({
        "context": ai_ctx,
        "prompt": prompt,
        "triagePrompt": triage,
        "promptLength": prompt.len(),
        "findingsCount": report.findings.len(),
    }))
}

/// ── Mod recommendation engine ─────────────────────────────────────

/// Analyzes the current modpack and suggests mods that would fill gaps
/// or improve the pack based on the knowledge base and common patterns.
#[tauri::command(rename_all = "camelCase")]
fn recommend_mods(path: String) -> Result<Vec<serde_json::Value>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let installed: std::collections::HashSet<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
    let mut recommendations = Vec::new();

    // Performance mods recommendation
    let perf_mods = [
        ("sodium", "Sodium", "Massive FPS boost via modern rendering engine", "optimization"),
        ("lithium", "Lithium", "Server-side optimization for tick performance", "optimization"),
        ("ferrite-core", "FerriteCore", "Reduces memory usage by optimizing game state", "optimization"),
        ("immediatelyfast", "ImmediatelyFast", "Immediate mode rendering optimization", "optimization"),
        ("modernfix", "ModernFix", "Fixes hundreds of performance bugs", "optimization"),
        ("entityculling", "EntityCulling", "Skips rendering of invisible entities", "optimization"),
        ("memoryleakfix", "MemoryLeakFix", "Fixes memory leaks in Minecraft/mod code", "optimization"),
        ("smoothboot", "Smooth Boot", "Makes game startup less CPU-intensive", "optimization"),
    ];

    let mut missing_perf = Vec::new();
    for (slug, name, desc, _cat) in &perf_mods {
        if !installed.contains(*slug) { missing_perf.push((*slug, *name, *desc)); }
    }
    if !missing_perf.is_empty() {
        for (slug, name, desc) in missing_perf.iter().take(4) {
            recommendations.push(serde_json::json!({
                "reason": "performance", "slug": slug, "name": name,
                "description": desc, "priority": "high",
            }));
        }
    }

    // QoL mods
    let qol_mods = [
        ("jei", "JEI", "Recipe viewer — essential for modded Minecraft", "qol"),
        ("jade", "Jade", "Shows what block/entity you're looking at", "qol"),
        ("appleskin", "AppleSkin", "Shows hunger/saturation values of food", "qol"),
        ("controlling", "Controlling", "Search for keybinds easily", "qol"),
        ("mouse-tweaks", "Mouse Tweaks", "Better inventory mouse handling", "qol"),
    ];
    for (slug, name, desc, _cat) in &qol_mods {
        if !installed.contains(*slug) {
            recommendations.push(serde_json::json!({
                "reason": "qol", "slug": slug, "name": name,
                "description": desc, "priority": "medium",
            }));
        }
    }

    // If Fabric, recommend Fabric API
    if loader == "fabric" && !installed.contains("fabric-api") {
        recommendations.push(serde_json::json!({
            "reason": "dependency", "slug": "fabric-api", "name": "Fabric API",
            "description": "Required by most Fabric mods — core library", "priority": "critical",
        }));
    }

    // If Create is installed, suggest Create addons
    if installed.contains("create") {
        let create_addons = [
            ("create_enchantment_industry", "Create: Enchantment Industry", "Automated enchanting with Create"),
            ("create_steam_n_rails", "Create: Steam n Rails", "Trains and rail systems"),
            ("create_confectionery", "Create: Confectionery", "Food and sweets automation"),
        ];
        for (slug, name, desc) in &create_addons {
            if !installed.contains(*slug) {
                recommendations.push(serde_json::json!({
                    "reason": "synergy", "slug": slug, "name": name,
                    "description": desc, "priority": "low",
                }));
            }
        }
    }

    Ok(recommendations)
}

/// Returns a compatibility database entry for a mod slug from the builtin
/// knowledge base.
#[tauri::command(rename_all = "camelCase")]
fn get_mod_info(slug: String) -> Result<Option<serde_json::Value>, String> {
    if let Some(entry) = tuffbox_core::knowledge::ModKnowledgeEntry::lookup(&slug) {
        Ok(Some(serde_json::json!({
            "slug": entry.slug, "name": entry.name,
            "configPaths": entry.config_paths,
            "oreKeys": entry.ore_keys,
            "knownConflicts": entry.known_conflicts,
            "loaders": entry.loaders,
            "category": entry.category,
        })))
    } else {
        Ok(None)
    }
}


/// Restores a project backup zip, extracting it over the current project.
/// Creates a snapshot before restoring as a safety net.
#[tauri::command(rename_all = "camelCase")]
fn restore_backup(path: String, backup_id: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let zip_path = project_dir.join(".tuffbox").join("backups").join(format!("{}.zip", backup_id));
    if !zip_path.is_file() { return Err("backup not found".into()); }

    // Safety: snapshot before restore
    let manifest_path = PathBuf::from(&path);
    auto_snapshot(&manifest_path, "before-restore").map_err(|e| e.to_string())?;

    let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name.ends_with('/') { continue; }
        let target = project_dir.join(&name);
        if let Some(parent) = target.parent() { std::fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
        let mut dest = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut dest).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// ── Problematic mods config ─────────────────────────────────────

/// Writes a problematic-mods.json config for mods known to cause crashes.
/// Compatible with Crash Assistant's problematic_mods_config.json format.
#[tauri::command(rename_all = "camelCase")]
fn save_problematic_mods_config(path: String, entries: Vec<serde_json::Value>) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let p = project_dir.join("config").join("problematic_mods_config.json");
    if let Some(par) = p.parent() { std::fs::create_dir_all(par).map_err(|e| e.to_string())?; }
    let json = serde_json::to_string_pretty(&serde_json::json!({ "mods": entries })).map_err(|e| e.to_string())?;
    std::fs::write(&p, json).map_err(|e| e.to_string())
}

/// Returns the current problematic mods config.
#[tauri::command(rename_all = "camelCase")]
fn get_problematic_mods_config(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let p = project_dir.join("config").join("problematic_mods_config.json");
    if !p.is_file() { return Ok(vec![]); }
    let raw = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    Ok(v.get("mods").and_then(|m| m.as_array()).cloned().unwrap_or_default())
}

/// ── Server launch ────────────────────────────────────────────────

/// Launches the server profile and captures the log. Prepares the instance
/// with server-safe mods, generates server.properties, and starts the JVM.
#[tauri::command(rename_all = "camelCase")]
async fn launch_server(path: String) -> Result<tuffbox_core::LaunchResult, String> {
    record_launch(path.clone())?;
    launch_profile(path, "server".into()).await
}

/// Generates a default server.properties file for the project.
#[tauri::command(rename_all = "camelCase")]
fn generate_server_properties(path: String) -> Result<String, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let profile = manifest.profiles.iter().find(|p| p.id == "server")
        .or_else(|| manifest.profiles.first());

    let mut props = String::new();
    props.push_str("# TuffBox generated server.properties
");
    props.push_str(&format!("server-port=25565
"));
    props.push_str(&format!("max-players=20
"));
    props.push_str(&format!("view-distance=10
"));
    props.push_str(&format!("simulation-distance=10
"));
    props.push_str(&format!("max-world-size=29999984
"));
    props.push_str(&format!("allow-flight=false
"));
    props.push_str(&format!("online-mode=true
"));
    props.push_str(&format!("difficulty=normal
"));
    props.push_str(&format!("gamemode=survival
"));
    props.push_str(&format!("enable-command-block=false
"));
    props.push_str(&format!("spawn-protection=16
"));
    props.push_str(&format!("max-tick-time=60000
"));
    props.push_str(&format!("level-name=world
"));
    props.push_str(&format!("motd=A TuffBox {} Server\n", manifest.project.name));

    if let Some(profile) = profile {
        if let Some(mem) = profile.memory_mb {
            props.push_str(&format!("# Memory: {} MB
", mem));
        }
    }

    let project_dir = manifest_parent(&path)?;
    let target = project_dir.join("server.properties");
    std::fs::write(&target, &props).map_err(|e| e.to_string())?;
    Ok(props)
}

/// ── Recipe scanner from actual JARs ──────────────────────────────

/// Scans all mod JARs in the mods/ folder for recipe JSON files and
/// returns a structured list. This gives real recipe data instead of
/// the hardcoded examples in the RecipeBrowser.
#[tauri::command(rename_all = "camelCase")]
fn scan_mod_recipes(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let mods_dir = project_dir.join("mods");
    let mut recipes = Vec::new();

    if !mods_dir.is_dir() { return Ok(recipes); }

    for entry in std::fs::read_dir(&mods_dir).into_iter().flatten().flatten() {
        let p = entry.path();
        if p.extension().map_or(true, |e| e != "jar") { continue; }
        let mod_name = entry.file_name().to_string_lossy().trim_end_matches(".jar").to_string();

        if let Ok(f) = std::fs::File::open(&p) {
            if let Ok(mut zip) = zip::ZipArchive::new(f) {
                for i in 0..zip.len() {
                    if let Ok(zf) = zip.by_index(i) {
                        let name = zf.name().to_string();
                        if name.starts_with("data/") && name.ends_with(".json") && name.contains("recipe") {
                            if let Ok(raw) = std::io::read_to_string(zf) {
                                if raw.len() < 128 * 1024 {
                                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                                        let recipe_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("crafting");
                                        let result = parsed.get("result");
                                        let output = if let Some(r) = result.and_then(|r| r.as_str()) { r.to_string() }
                                            else if let Some(r) = result.and_then(|r| r.get("item")).and_then(|i| i.as_str()) { r.to_string() }
                                            else { "?".into() };
                                        let ingredients = extract_ingredients(&parsed);
                                        recipes.push(serde_json::json!({
                                            "id": name.trim_end_matches(".json"),
                                            "type": recipe_type,
                                            "output": output,
                                            "input": ingredients.join(", "),
                                            "sourceFile": name,
                                            "modSource": mod_name,
                                        }));
                                        if recipes.len() >= 200 { break; }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if recipes.len() >= 200 { break; }
    }

    Ok(recipes)
}

fn extract_ingredients(v: &serde_json::Value) -> Vec<String> {
    let mut items = Vec::new();
    if let Some(key) = v.get("key").and_then(|k| k.as_object()) {
        for (_, val) in key {
            if let Some(item) = val.get("item").and_then(|i| i.as_str()) { items.push(item.to_string()); }
            else if let Some(tag) = val.get("tag").and_then(|t| t.as_str()) { items.push(format!("#{}", tag)); }
        }
    }
    if let Some(ings) = v.get("ingredients").and_then(|i| i.as_array()) {
        for ing in ings {
            if let Some(item) = ing.get("item").and_then(|i| i.as_str()) { items.push(item.to_string()); }
            else if let Some(tag) = ing.get("tag").and_then(|t| t.as_str()) { items.push(format!("#{}", tag)); }
            else if let Some(list) = ing.as_array() {
                for l in list { if let Some(s) = l.get("item").and_then(|i| i.as_str()) { items.push(s.to_string()); } }
            }
        }
    }
    items.truncate(8);
    items
}


/// ── World management ────────────────────────────────────────────

/// Lists Minecraft worlds in the project's saves/ folder.
#[tauri::command(rename_all = "camelCase")]
fn list_worlds(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let saves_dir = project_dir.join("saves");
    if !saves_dir.is_dir() { return Ok(vec![]); }
    let mut worlds = Vec::new();
    for entry in std::fs::read_dir(&saves_dir).into_iter().flatten().flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            let level_dat = p.join("level.dat");
            let mut size: u64 = 0;
            fn dir_size(d: &std::path::Path, s: &mut u64) {
                for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.is_dir() { dir_size(&p, s); }
                    else if let Ok(m) = p.metadata() { *s += m.len(); }
                }
            }
            dir_size(&p, &mut size);
            let has_level = level_dat.is_file();
            let size_str = if size < 1048576 { format!("{:.1} KB", size as f64 / 1024.0) }
                else if size < 1073741824 { format!("{:.1} MB", size as f64 / 1048576.0) }
                else { format!("{:.1} GB", size as f64 / 1073741824.0) };
            worlds.push(serde_json::json!({"name": name, "size": size, "sizeFormatted": size_str, "hasLevelDat": has_level}));
        }
    }
    worlds.sort_by_key(|w| -(w["size"].as_u64().unwrap_or(0) as i64));
    Ok(worlds)
}

/// Backs up a single world as a zip archive.
#[tauri::command(rename_all = "camelCase")]
fn backup_world(path: String, world_name: String) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    if !world_dir.is_dir() { return Err("world not found".into()); }
    let backup_dir = project_dir.join(".tuffbox").join("world-backups");
    std::fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    let zip_name = format!("{}-{}.zip", world_name, tuffbox_core::time_util::compact_now());
    let zip_path = backup_dir.join(&zip_name);
    let out = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(out);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    fn add_world(zip: &mut zip::ZipWriter<std::fs::File>, opts: zip::write::SimpleFileOptions, base: &std::path::Path, dir: &std::path::Path) -> Result<(), String> {
        for e in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            let p = e.path();
            if p.is_dir() { add_world(zip, opts, base, &p)?; }
            else if p.is_file() {
                let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                zip.start_file(rel, opts).map_err(|e| e.to_string())?;
                zip.write_all(&std::fs::read(&p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    add_world(&mut zip, opts, &world_dir.parent().unwrap(), &world_dir)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(zip_path.to_string_lossy().to_string())
}

/// ── Modpack templates ───────────────────────────────────────────

/// Saves the current project as a reusable template (copies manifest + modlist metadata).
#[tauri::command(rename_all = "camelCase")]
fn save_as_template(path: String, template_name: String) -> Result<(), String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let template_dir = project_dir.join(".tuffbox").join("templates");
    std::fs::create_dir_all(&template_dir).map_err(|e| e.to_string())?;
    let template = serde_json::json!({
        "name": template_name,
        "createdAt": tuffbox_core::time_util::rfc3339_now(),
        "manifest": manifest,
        "modCount": manifest.mods.len(),
    });
    let fname = template_name.to_lowercase().replace(' ', "-");
    let p = template_dir.join(format!("{}.json", fname));
    std::fs::write(&p, serde_json::to_string_pretty(&template).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Lists saved modpack templates.
#[tauri::command(rename_all = "camelCase")]
fn list_templates(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let template_dir = project_dir.join(".tuffbox").join("templates");
    if !template_dir.is_dir() { return Ok(vec![]); }
    let mut templates = Vec::new();
    for entry in std::fs::read_dir(&template_dir).into_iter().flatten().flatten() {
        if entry.path().extension().map_or(false, |e| e == "json") {
            if let Ok(raw) = std::fs::read_to_string(entry.path()) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    templates.push(v);
                }
            }
        }
    }
    Ok(templates)
}

/// ── Download progress tracking ──────────────────────────────────

static DOWNLOAD_PROGRESS: once_cell::sync::Lazy<std::sync::Mutex<std::collections::HashMap<String, (u64, u64)>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Returns the current download progress for active mod downloads.
#[tauri::command(rename_all = "camelCase")]
fn get_download_progress() -> Result<Vec<serde_json::Value>, String> {
    let map = DOWNLOAD_PROGRESS.lock().map_err(|e| e.to_string())?;
    Ok(map.iter().map(|(k, (done, total))| serde_json::json!({
        "id": k, "downloaded": done, "total": total,
        "percent": if *total > 0 { ((*done as f64 / *total as f64) * 100.0).round() as u32 } else { 0 }
    })).collect())
}

/// ── Keyboard shortcut reference ─────────────────────────────────

/// Returns a keyboard shortcut reference sheet.
#[tauri::command(rename_all = "camelCase")]
fn get_keyboard_shortcuts() -> Result<Vec<serde_json::Value>, String> {
    Ok(vec![
        serde_json::json!({"key": "Ctrl+1", "action": "Dashboard", "context": "global"}),
        serde_json::json!({"key": "Ctrl+2", "action": "IDE Workflow", "context": "global"}),
        serde_json::json!({"key": "Ctrl+3", "action": "Mods", "context": "global"}),
        serde_json::json!({"key": "Ctrl+4", "action": "Dependency Graph", "context": "global"}),
        serde_json::json!({"key": "Ctrl+5", "action": "Config Editor", "context": "global"}),
        serde_json::json!({"key": "Ctrl+6", "action": "Health Check", "context": "global"}),
        serde_json::json!({"key": "Ctrl+7", "action": "Snapshots", "context": "global"}),
        serde_json::json!({"key": "Ctrl+S", "action": "Save file", "context": "Config Editor"}),
        serde_json::json!({"key": "Ctrl+N", "action": "New project", "context": "Dashboard"}),
        serde_json::json!({"key": "Ctrl+O", "action": "Open project", "context": "Dashboard"}),
        serde_json::json!({"key": "Escape", "action": "Close modal / deselect", "context": "global"}),
    ])
}


/// ── Config linter ────────────────────────────────────────────────

/// Lints a config file for common Minecraft issues: syntax errors,
/// duplicate keys, missing defaults, and performance-sapping settings.
#[tauri::command(rename_all = "camelCase")]
fn lint_config(path: String, relative_path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let target = project_dir.join(&relative_path);
    let content = std::fs::read_to_string(&target).map_err(|e| e.to_string())?;
    let mut issues = Vec::new();
    let ext = std::path::Path::new(&relative_path).extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "json" => { if let Err(e) = serde_json::from_str::<serde_json::Value>(&content) { issues.push(serde_json::json!({"severity":"error","code":"JSON_SYNTAX","message":format!("JSON syntax error: {}", e),"line":null})); } },
        "properties" | "txt" => {
            let mut seen_keys = std::collections::HashSet::new();
            for (line_no, line) in content.lines().enumerate() {
                let t = line.trim();
                if t.is_empty() || t.starts_with('#') { continue; }
                if !t.contains('=') && t.len() > 2 {
                    issues.push(serde_json::json!({"severity":"warning","code":"PROPERTY_NO_EQ","message":"Line without = sign","line":line_no+1}));
                    continue;
                }
                if let Some(eq) = t.find('=') {
                    let key = t[..eq].trim();
                    if key.is_empty() { issues.push(serde_json::json!({"severity":"warning","code":"EMPTY_KEY","message":"Empty key","line":line_no+1})); }
                    else if !seen_keys.insert(key.to_string()) { issues.push(serde_json::json!({"severity":"warning","code":"DUPLICATE_KEY","message":format!("Duplicate key: {}", key),"line":line_no+1})); }
                }
            }
        }
        "toml" => { if let Err(e) = toml::from_str::<toml::Value>(&content) { issues.push(serde_json::json!({"severity":"error","code":"TOML_SYNTAX","message":format!("TOML syntax error: {}", e),"line":null})); } }
        _ => {}
    }

    // Check for common performance-sapping server settings
    if content.contains("max-tick-time=-1") {
        issues.push(serde_json::json!({"severity":"warning","code":"MAX_TICK_TIME_DISABLED","message":"max-tick-time is -1 (off). Server won't crash on overload but may freeze indefinitely.","line":null}));
    }
    if content.contains("view-distance=") {
        for line in content.lines() { if line.contains("view-distance=") { if let Some(v) = line.split('=').last() { if let Ok(n) = v.trim().parse::<u32>() { if n > 16 { issues.push(serde_json::json!({"severity":"warning","code":"HIGH_VIEW_DISTANCE","message":format!("View distance {} may cause lag on modded servers.", n),"line":null})); }}} break; } }
    }

    Ok(issues)
}

/// ── Memory cleanup / temp files ──────────────────────────────────

/// Cleans up temporary files from the project: old test runs, stale
/// snapshots, and downloaded mod jars that are no longer in the manifest.
#[tauri::command(rename_all = "camelCase")]
fn cleanup_project(path: String) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mut cleaned = Vec::new();

    // Remove mods not in manifest (orphaned jars)
    let mods_dir = project_dir.join("mods");
    let known_files: std::collections::HashSet<String> = manifest.mods.iter().filter_map(|m| m.file_name.clone()).collect();
    if mods_dir.is_dir() {
        for entry in std::fs::read_dir(&mods_dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().extension().map_or(false, |e| e == "jar") && !known_files.contains(&name) {
                let _ = std::fs::remove_file(entry.path());
                cleaned.push(format!("mods/{}", name));
            }
        }
    }

    // Remove old test run logs (older than 30 days)
    let test_runs = project_dir.join(".tuffbox").join("test-runs");
    if test_runs.is_dir() {
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86400);
        for entry in std::fs::read_dir(&test_runs).into_iter().flatten().flatten() {
            if entry.path().is_dir() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mod_time) = meta.modified() {
                        if mod_time < cutoff {
                            let _ = std::fs::remove_dir_all(entry.path());
                            cleaned.push(format!("test-runs/{}", entry.file_name().to_string_lossy()));
                        }
                    }
                }
            }
        }
    }

    Ok(serde_json::json!({"cleaned": cleaned, "count": cleaned.len()}))
}

/// ── App version & update check ───────────────────────────────────

/// Returns the current TuffBox version.
#[tauri::command(rename_all = "camelCase")]
fn get_app_version() -> Result<String, String> {
    Ok("0.1.0-alpha".into())
}

/// Stub for update check — returns mock data.
#[tauri::command(rename_all = "camelCase")]
fn check_for_app_update() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "currentVersion": "0.1.0-alpha",
        "latestVersion": "0.1.0-alpha",
        "updateAvailable": false,
        "checkedAt": tuffbox_core::time_util::rfc3339_now(),
    }))
}


/// ── World preview (level.dat reader) ────────────────────────────

/// Reads a Minecraft world's metadata from saves/<name>/level.dat
/// and returns structured world info: name, seed, game type, etc.
#[tauri::command(rename_all = "camelCase")]
fn read_world_info(path: String, world_name: String) -> Result<serde_json::Value, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let info = tuffbox_core::level_dat::read_world_info(&world_dir)
        .map_err(|e| format!("Failed to read level.dat: {}", e))?;
    Ok(serde_json::json!({
        "name": info.name, "seed": info.seed, "gameType": info.game_type,
        "lastPlayed": info.last_played, "time": info.time,
        "spawnX": info.spawn_x, "spawnY": info.spawn_y, "spawnZ": info.spawn_z,
        "difficulty": info.difficulty, "hardcore": info.hardcore,
        "cheatsEnabled": info.cheats_enabled,
        "sizeBytes": info.size_bytes, "sizeFormatted": info.size_formatted,
    }))
}

/// ── Export to GitHub Releases ──────────────────────────────────

/// Generates GitHub Release-compatible changelog and asset manifest.
#[tauri::command(rename_all = "camelCase")]
fn generate_github_release(path: String, tag: Option<String>, target: Option<String>) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let version = tag.unwrap_or_else(|| manifest.project.version.clone());
    let tag_name = format!("v{}", version);
    let changelog = format!("# {} {}\n\n{}", manifest.project.name, manifest.project.version, manifest.project.description.as_deref().unwrap_or(""));

    // List export artifacts
    let mut artifacts = Vec::new();
    let artifact_dir = project_dir.join(".tuffbox").join("artifacts");
    if artifact_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&artifact_dir) {
            for e in entries.flatten() {
                artifacts.push(e.file_name().to_string_lossy().to_string());
            }
        }
    }

    // Generate release body
    let body = format!(
        "## {} v{}

**Minecraft:** {} | **Loader:** {} {}

### Changelog

{}

### Installed Mods ({})

{}",
        manifest.project.name, version,
        manifest.minecraft.version,
        format!("{:?}", manifest.loader.kind).to_lowercase(), manifest.loader.version,
        changelog,
        manifest.mods.len(),
        manifest.mods.iter().map(|m| format!("- {} {}", m.name, m.version)).take(50).collect::<Vec<_>>().join("\n")
    );

    let release_dir = if let Some(t) = target { std::path::PathBuf::from(&t) } else { project_dir.join("release") };
    std::fs::create_dir_all(&release_dir).map_err(|e| e.to_string())?;
    let release_json = release_dir.join("github-release.json");
    let payload = serde_json::json!({
        "tag_name": tag_name, "name": format!("{} {}", manifest.project.name, version),
        "body": body, "draft": true, "prerelease": version.contains("alpha") || version.contains("beta"),
        "artifacts": artifacts,
    });
    std::fs::write(&release_json, serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "tagName": tag_name, "body": body,
        "releaseJsonPath": release_json.to_string_lossy().to_string(),
        "artifacts": artifacts, "artifactCount": artifacts.len(),
    }))
}

/// ── Localization helper ──────────────────────────────────────────

static L10N: once_cell::sync::Lazy<std::collections::HashMap<&str, &str>> = once_cell::sync::Lazy::new(|| {
    let mut m = std::collections::HashMap::new();
    // Common UI strings
    m.insert("dashboard", "Панель / Dashboard");
    m.insert("mods", "Моды");
    m.insert("graph", "Граф зависимостей");
    m.insert("settings", "Настройки");
    m.insert("launch", "Запустить / Launch");
    m.insert("open_ide", "Открыть IDE");
    m.insert("snapshots", "Снапшоты");
    m.insert("export", "Экспорт");
    m.insert("release", "Релиз");
    m.insert("diagnostics", "Диагностика");
    m.insert("configs", "Конфигурации");
    m.insert("test", "Тестовые запуски");
    m.insert("history", "История изменений");
    m.insert("back", "Назад");
    m.insert("save", "Сохранить");
    m.insert("cancel", "Отмена");
    m.insert("delete", "Удалить");
    m.insert("remove", "Убрать");
    m.insert("add", "Добавить");
    m.insert("search", "Поиск");
    m.insert("refresh", "Обновить");
    m.insert("loading", "Загрузка...");
    m.insert("error", "Ошибка");
    m.insert("success", "Успешно");
    m.insert("warning", "Предупреждение");
    m.insert("no_project", "Откройте проект");
    m
});

/// Returns a localized string (RU/EN). Falls back to the key itself.
#[tauri::command(rename_all = "camelCase")]
fn localize(key: String) -> Result<String, String> {
    Ok(L10N.get(key.as_str()).copied().unwrap_or(&key).to_string())
}

/// Returns all localization keys (for UI reference).
#[tauri::command(rename_all = "camelCase")]
fn list_localizations() -> Result<Vec<serde_json::Value>, String> {
    Ok(L10N.iter().map(|(k, v)| serde_json::json!({"key": k, "ru": v.split(" / ").next().unwrap_or(v)})).collect())
}


/// ── Batch operations for CLI/scripting ────────────────────────────

/// Exports the dependency graph as a DOT string (Graphviz format),
/// which can be rendered to PNG/SVG with the `dot` command.
#[tauri::command(rename_all = "camelCase")]
fn export_graph_dot(path: String) -> Result<String, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let graph = DependencyGraph::from_manifest(&manifest);
    let mut dot = String::from("digraph TuffBox {
");
    dot.push_str("  rankdir=LR;
  node [shape=box, style=filled, fillcolor=\"#18181b\", fontcolor=\"#e5e7eb\", color=\"#27272a\"];
");
    dot.push_str("  edge [color=\"#3f3f46\", fontcolor=\"#71717a\"];

");

    for node in &graph.nodes {
        let color = match node.kind {
            tuffbox_core::graph::NodeKind::Mod => "#1bd96a22",
            tuffbox_core::graph::NodeKind::Profile => "#8b5cf622",
            _ => "#f59e0b22",
        };
        let shape = if node.kind == tuffbox_core::graph::NodeKind::Profile { "ellipse" } else { "box" };
        dot.push_str(&format!("  \"{}\" [label=\"{}\", shape={}, fillcolor=\"{}\", color=\"{}\"];
",
            node.id.0, node.label, shape, color,
            if color.len() > 9 { &color[..7] } else { color }));
    }

    for edge in &graph.edges {
        let style = if edge.kind == tuffbox_core::graph::EdgeKind::Requires { "solid" }
            else if edge.kind == tuffbox_core::graph::EdgeKind::Conflicts { "dashed, color=\"#ef4444\"" }
            else { "dotted" };
        dot.push_str(&format!("  \"{}\" -> \"{}\" [label=\"{:?}\", style={}];
",
            edge.from.0, edge.to.0, edge.kind, style));
    }

    dot.push_str("}
");
    Ok(dot)
}

/// Exports the full project state as a JSON report (manifest + graph +
/// diagnostics + snapshots) for external tooling.
#[tauri::command(rename_all = "camelCase")]
fn export_project_report(path: String) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let graph = DependencyGraph::from_manifest(&manifest);
    let diagnostics = Resolver::analyze_project(&manifest, &graph);
    let project_dir = manifest_parent(&path)?;
    let store = SnapshotStore::new(&project_dir);
    let snapshots = store.list().unwrap_or_default();

    let change_plan = Resolver::create_fix_plan(&graph, &diagnostics);

    Ok(serde_json::json!({
        "project": manifest.project,
        "minecraft": manifest.minecraft,
        "loader": { "kind": format!("{:?}", manifest.loader.kind), "version": manifest.loader.version },
        "modCount": manifest.mods.len(),
        "mods": manifest.mods.iter().map(|m| serde_json::json!({
            "id": m.id, "name": m.name, "version": m.version, "side": format!("{:?}", m.side),
            "source": format!("{:?}", m.source.kind), "contentType": format!("{:?}", m.content_type),
        })).collect::<Vec<_>>(),
        "graph": { "nodes": graph.nodes.len(), "edges": graph.edges.len() },
        "diagnostics": diagnostics.iter().map(|d| serde_json::json!({
            "severity": format!("{:?}", d.severity), "code": d.code, "message": d.message,
        })).collect::<Vec<_>>(),
        "snapshots": snapshots.len(),
        "changePlan": change_plan.map(|p| serde_json::json!({
            "summary": p.summary, "risk": format!("{:?}", p.risk),
            "actions": p.actions.len(), "requiresSnapshot": p.requires_snapshot,
        })),
        "generatedAt": tuffbox_core::time_util::rfc3339_now(),
    }))
}

/// Batch export: generates .mrpack, server pack, Prism, CurseForge
/// and GitHub release all at once.
#[tauri::command(rename_all = "camelCase")]
fn batch_export_all(path: String) -> Result<Vec<serde_json::Value>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let base = project_dir.join("export");
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    let mut results = Vec::new();

    type ExportFn = Box<dyn Fn(&ProjectManifest, &std::path::Path, &std::path::Path) -> Result<tuffbox_core::ExportResult, tuffbox_core::ExportError>>;
    let exports: Vec<(&str, ExportFn)> = vec![
        ("mrpack", Box::new(|m, p, o| tuffbox_core::exporter::export_modrinth_pack(m, p, o))),
        ("server-pack", Box::new(|m, p, o| tuffbox_core::exporter::export_server_pack(m, p, o))),
    ];

    for (kind, export_fn) in &exports {
        let out = base.join(format!("{}-{}.zip", manifest.project.id, kind));
        match export_fn(&manifest, &PathBuf::from(&path), &out) {
            Ok(result) => results.push(serde_json::json!({"kind": kind, "path": result.path.to_string_lossy(), "files": result.file_count, "status": "ok"})),
            Err(e) => results.push(serde_json::json!({"kind": kind, "status": "error", "error": e.to_string()})),
        }
    }

    Ok(results)
}


#[tauri::command(rename_all = "camelCase")]
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
        download_project_mods(&manifest_path, &manifest);
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
        download_project_mods(&manifest_path, &manifest);
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
        // Use recursive resolution: install direct deps + transitive deps
        let installed = install_modrinth_with_dependencies(&mut manifest, &missing, "auto");
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods(&manifest_path, &manifest);
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

/// Actually applies the crash-diagnosis fix plan (update/disable suspected
/// mod, install missing dependency, etc.), the same way the Graph tab's
/// "Apply full plan" does for resolver plans.
///
/// Previously the Diagnostics UI had a "Fix Issue" button that only set a
/// success message in the frontend without calling into the backend at
/// all — no snapshot, no manifest change, nothing. This command gives that
/// button (renamed "Apply fix plan") a real effect: it recomputes the plan
/// server-side (so the UI can't apply a stale/tampered plan), snapshots
/// first when the plan calls for it, and returns what was actually done so
/// the UI can report a truthful result instead of an assumed one.
#[tauri::command(rename_all = "camelCase")]
async fn apply_crash_fix_plan(path: String, report_id: Option<String>) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let diagnosis = get_crash_diagnosis(path.clone(), report_id)?;
        let plan = diagnosis.fix_plan;

        if plan.actions.is_empty() {
            return Ok(Vec::new());
        }

        if plan.requires_snapshot {
            auto_snapshot(&manifest_path, "apply-crash-fix-plan").map_err(|e| e.to_string())?;
        }

        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let mut applied = Vec::new();
        for action in plan.actions {
            apply_change_action(&mut manifest, action, &mut applied)?;
        }
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods(&manifest_path, &manifest);
        Ok(applied)
    })
    .await
    .map_err(|e| e.to_string())?
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

/// Returns true if crashes were detected in the project.
#[tauri::command(rename_all = "camelCase")]
fn has_crashed(path: String) -> Result<bool, String> {
    let project_dir = manifest_parent(&path)?;
    let crash_dir = project_dir.join("crash-reports");
    if crash_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&crash_dir) {
            if entries.filter_map(|e| e.ok()).any(|e| e.path().extension().map_or(false, |x| x == "txt")) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[tauri::command(rename_all = "camelCase")]
async fn launch_with_quick_play(
    path: String, profile: String,
    _quick_play_type: Option<String>, _quick_play_value: Option<String>,
) -> Result<tuffbox_core::LaunchResult, String> {
    launch_profile(path, profile).await
}

#[tauri::command(rename_all = "camelCase")]
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
    let _instance_id = profile.clone();
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
        // Auto-detect the best Java for this Minecraft version instead of
        // always grabbing whatever JVM happens to be newest on the system
        // — using e.g. Java 21 for Forge 1.20.1 (which needs Java 17)
        // fails deep inside Forge's bootstrap launcher with a confusing
        // module-system error instead of launching at all.
        TestLauncher::find_java_for_minecraft(&manifest.minecraft.version).map_err(|e| e.to_string())?
    };

    let progress = tuffbox_core::mc_install::InstallProgress {
        log_path: log_path.clone(),
    };

    progress.log(&format!("# Java: {} (major {})", java.path, java.major));
    progress.log(&format!("# Java version: {}", java.version));
    let required_java = tuffbox_core::jre::required_java_major(&manifest.minecraft.version);
    if java.major != required_java {
        progress.log(&format!(
            "# WARNING: Minecraft {} typically needs Java {required_java}, but the selected runtime is Java {}. \
             If the game fails to start, install Java {required_java} and select it in Project Settings.",
            manifest.minecraft.version, java.major
        ));
    }

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

    // Safety net: make sure every mod declared in the manifest actually has
    // its .jar on disk before we launch. Mods can end up missing here if
    // they were added while offline, if a previous download failed, or if
    // the manifest was hand-edited/imported without a download step.
    // Without this, TuffBox would happily launch vanilla Minecraft while the
    // UI still shows a full mod list.
    progress.log("# Verifying mod files...");
    let sync_report = tuffbox_core::ensure_project_mods_downloaded(&manifest, &game_dir);
    if !sync_report.downloaded.is_empty() {
        progress.log(&format!(
            "# Downloaded {} missing mod file(s): {}",
            sync_report.downloaded.len(),
            sync_report.downloaded.join(", ")
        ));
    }
    if !sync_report.failed.is_empty() {
        for failure in &sync_report.failed {
            progress.log(&format!(
                "# WARNING: failed to prepare mod '{}': {}",
                failure.mod_id, failure.error
            ));
        }
    }

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
fn import_curseforge_project(source: String, target_dir: String) -> Result<String, String> {
    let manifest = tuffbox_core::import_curseforge_pack(&source).map_err(|e| e.to_string())?;
    let target = PathBuf::from(&target_dir);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let manifest_path = target.join("project.tuffbox.json");
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&manifest_path, json).map_err(|e| e.to_string())?;
    Ok(manifest_path.to_string_lossy().to_string())
}

#[tauri::command(rename_all = "camelCase")]
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

/// Packs `logs/`, `crash-reports/` and test-run history into a zip next to
/// the manifest and returns its path, so the UI's "Create logs.zip" action
/// (previously an `alert("not implemented yet")` stub) actually produces a
/// shareable archive.
#[tauri::command(rename_all = "camelCase")]
fn create_logs_zip(path: String) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let timestamp = tuffbox_core::time_util::compact_now();
    let output = project_dir.join(format!("{}-logs-{timestamp}.zip", manifest.project.id));
    let result = tuffbox_core::export_logs_zip(&project_dir, &output).map_err(|e| e.to_string())?;
    Ok(result.path.to_string_lossy().to_string())
}

/// Duplicates a project (manifest + mods/config/overrides folders, minus
/// `.tuffbox/` internal state and snapshots) into a sibling directory,
/// implementing the previously-stubbed "Clone as..." action.
#[tauri::command(rename_all = "camelCase")]
fn clone_project(path: String, new_name: String) -> Result<String, String> {
    let source_dir = manifest_parent(&path)?;
    let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;

    let new_slug = slugify_project_name(&new_name);
    let target_dir = source_dir
        .parent()
        .map(|p| p.join(&new_slug))
        .ok_or_else(|| "project has no parent directory".to_string())?;
    if target_dir.exists() {
        return Err(format!("a folder named '{new_slug}' already exists next to this project"));
    }
    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    for entry_name in ["mods", "config", "defaultconfigs", "kubejs", "scripts", "overrides"] {
        let src = source_dir.join(entry_name);
        if src.is_dir() {
            copy_dir_recursive(&src, &target_dir.join(entry_name)).map_err(|e| e.to_string())?;
        }
    }

    manifest.project.id = new_slug.clone();
    manifest.project.name = new_name;

    let target_manifest = target_dir.join(format!("{new_slug}.tuffbox.json"));
    save_manifest(&target_manifest, &manifest).map_err(|e| e.to_string())?;

    Ok(target_manifest.to_string_lossy().to_string())
}

fn slugify_project_name(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "cloned-project".to_string()
    } else {
        slug
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

/// Re-syncs a project's content folders against the manifest: re-downloads
/// any missing/hash-mismatched mod/resourcepack/shaderpack/datapack files.
/// This is the honest version of the previously-stubbed "Repair Profile"
/// action — it doesn't pretend to fix arbitrary problems, but it does fix
/// the most common real one (missing or corrupted content files).
#[tauri::command(rename_all = "camelCase")]
async fn repair_project(path: String) -> Result<tuffbox_core::ModSyncReport, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let instance_dir = tuffbox_core::instance_dir_for_manifest(&PathBuf::from(&path))
            .ok_or_else(|| "manifest has no parent directory".to_string())?;
        Ok(tuffbox_core::ensure_project_mods_downloaded(&manifest, &instance_dir))
    })
    .await
    .map_err(|e| e.to_string())?
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

/// Lists all log files in the instance's logs/ folder with sizes and
/// modification times, similar to NitroLaunch's get_instance_logs.
#[tauri::command(rename_all = "camelCase")]
fn list_instance_logs(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let logs_dir = project_dir.join("logs");
    if !logs_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<_> = std::fs::read_dir(&logs_dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|entry| {
            let p = entry.path();
            if !p.is_file() { return None; }
            let meta = p.metadata().ok()?;
            let modified = meta.modified().ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            Some(serde_json::json!({
                "name": entry.file_name().to_string_lossy(),
                "size": meta.len(),
                "modified": modified,
            }))
        })
        .collect();
    entries.sort_by_key(|e| {
        -(e["modified"].as_u64().unwrap_or(0) as i64)
    });
    Ok(entries)
}

/// Reads a specific log file from the instance's logs/ folder.
#[tauri::command(rename_all = "camelCase")]
fn read_instance_log(path: String, log_name: String) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let log_path = project_dir.join("logs").join(&log_name);
    // Validate: must be under logs/ and contain no path traversal
    if log_path.parent().map(|p| !p.ends_with("logs")).unwrap_or(true) {
        return Err("invalid log path".to_string());
    }
    let resolved = std::fs::canonicalize(&log_path).map_err(|e| e.to_string())?;
    if !resolved.starts_with(&project_dir.join("logs")) {
        return Err("path traversal detected".to_string());
    }
    tuffbox_core::process::read_log_tail(&log_path, 5000).map_err(|e| e.to_string())
}

/// Returns the total size of the instance on disk (mods, configs,
/// resourcepacks, etc.), useful for UI display like NitroLaunch.
#[tauri::command(rename_all = "camelCase")]
fn get_instance_size(path: String) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let mut total: u64 = 0;
    fn walk(dir: &std::path::Path, total: &mut u64) {
        let entries = match std::fs::read_dir(dir) { Ok(e) => e, Err(_) => return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() { walk(&p, total); }
            else if let Ok(meta) = p.metadata() { *total += meta.len(); }
        }
    }
    for sub in &["mods", "config", "resourcepacks", "shaderpacks", "datapacks", "scripts", "logs"] {
        walk(&project_dir.join(sub), &mut total);
    }
    // Human-readable size
    if total < 1024 { Ok(format!("{} B", total)) }
    else if total < 1024 * 1024 { Ok(format!("{:.1} KB", total as f64 / 1024.0)) }
    else if total < 1024 * 1024 * 1024 { Ok(format!("{:.1} MB", total as f64 / 1024.0 / 1024.0)) }
    else { Ok(format!("{:.1} GB", total as f64 / 1024.0 / 1024.0 / 1024.0)) }
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
    player_name: Option<String>,
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
        profile.player_name = player_name.filter(|name| !name.trim().is_empty());
    }

    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// ── Pinning & session state persisted to .tuffbox/data.json ─────────

#[derive(Debug, serde::Serialize, serde::Deserialize, Default)]
#[allow(dead_code)]
struct LauncherDataState {
    #[serde(default)]
    pinned: std::collections::HashSet<String>,
    #[serde(default)]
    last_opened: Option<String>,
}

#[allow(dead_code)]
fn launcher_data_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("launcher-data.json")
}

#[allow(dead_code)]
fn load_launcher_data(project_dir: &Path) -> LauncherDataState {
    let path = launcher_data_path(project_dir);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[allow(dead_code)]
fn save_launcher_data(project_dir: &Path, state: &LauncherDataState) -> Result<(), String> {
    let path = launcher_data_path(project_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn pin_project(path: String, pin: bool) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let mut state = load_launcher_data(&project_dir);
    let key = path.clone();
    if pin {
        state.pinned.insert(key);
    } else {
        state.pinned.remove(&key);
    }
    save_launcher_data(&project_dir, &state)
}

#[tauri::command(rename_all = "camelCase")]
fn is_project_pinned(path: String) -> Result<bool, String> {
    let project_dir = manifest_parent(&path)?;
    let state = load_launcher_data(&project_dir);
    Ok(state.pinned.contains(&path))
}

#[tauri::command(rename_all = "camelCase")]
fn set_last_opened_project(path: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let mut state = load_launcher_data(&project_dir);
    state.last_opened = Some(path.clone());
    save_launcher_data(&project_dir, &state)
}

#[tauri::command(rename_all = "camelCase")]
fn get_last_opened_project() -> Result<Option<String>, String> {
    // Scan parent dirs of known projects — simple approach
    let home = dirs::home_dir().unwrap_or_default();
    let tuffbox_dir = home.join("TuffBox");
    let data_path = tuffbox_dir.join(".tuffbox").join("launcher-data.json");
    if data_path.exists() {
        if let Ok(raw) = std::fs::read_to_string(&data_path) {
            if let Ok(state) = serde_json::from_str::<LauncherDataState>(&raw) {
                return Ok(state.last_opened);
            }
        }
    }
    Ok(None)
}


#[tauri::command(rename_all = "camelCase")]
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
            player_name: None,
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
        // Route the file into the right instance folder (mods/,
        // resourcepacks/, shaderpacks/, datapacks/) based on what Modrinth
        // actually says this project is, instead of always treating it as
        // a mod jar.
        content_type: tuffbox_core::manifest::ContentType::from_modrinth_project_type(&project.project_type),
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

/// Downloads every manifest-declared entry that isn't already present with
/// a matching hash into its content-type-appropriate folder (`mods/`,
/// `resourcepacks/`, `shaderpacks/`, `datapacks/`).
///
/// This is called right after any manifest mutation that adds/updates
/// content so the files backing those entries actually exist before the
/// next test launch, instead of only existing as metadata in the manifest.
/// Failures are best-effort: an entry that fails to download still shows up
/// in diagnostics/graph as missing rather than silently blocking the whole
/// manifest write.

/// Side-by-side manifest diff between two snapshots.
/// Returns structured changes: added/removed mods, MC/loader version changes,
/// plus a unified diff of the full manifest JSON.
#[tauri::command(rename_all = "camelCase")]
fn diff_manifest_snapshots(
    project_dir: String,
    from_id: String,
    to_id: String,
) -> Result<serde_json::Value, String> {
    let store = SnapshotStore::new(&project_dir);
    let from_snapshot = store.get(&from_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("snapshot {from_id} not found"))?;
    let to_snapshot = store.get(&to_id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("snapshot {to_id} not found"))?;

    let from_text = std::fs::read_to_string(&from_snapshot.manifest_path).unwrap_or_default();
    let to_text = std::fs::read_to_string(&to_snapshot.manifest_path).unwrap_or_default();
    let from_json: serde_json::Value = serde_json::from_str(&from_text).unwrap_or_default();
    let to_json: serde_json::Value = serde_json::from_str(&to_text).unwrap_or_default();

    let from_mods: std::collections::HashSet<String> = from_json
        .get("mods").and_then(|m| m.as_array()).into_iter().flatten()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();
    let to_mods: std::collections::HashSet<String> = to_json
        .get("mods").and_then(|m| m.as_array()).into_iter().flatten()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();
    let added_mods: Vec<_> = to_mods.difference(&from_mods).collect();
    let removed_mods: Vec<_> = from_mods.difference(&to_mods).collect();
    let from_ver = from_json.get("minecraft").and_then(|m| m.get("version")).and_then(|v| v.as_str()).unwrap_or("");
    let to_ver = to_json.get("minecraft").and_then(|m| m.get("version")).and_then(|v| v.as_str()).unwrap_or("");
    let from_loader = from_json.get("loader").and_then(|l| l.get("version")).and_then(|v| v.as_str()).unwrap_or("");
    let to_loader = to_json.get("loader").and_then(|l| l.get("version")).and_then(|v| v.as_str()).unwrap_or("");

    Ok(serde_json::json!({
        "diffText": unified_text_diff(&from_text, &to_text),
        "addedMods": added_mods,
        "removedMods": removed_mods,
        "mcVersionChanged": from_ver != to_ver,
        "fromMcVersion": from_ver,
        "toMcVersion": to_ver,
        "loaderVersionChanged": from_loader != to_loader,
        "fromLoaderVersion": from_loader,
        "toLoaderVersion": to_loader,
    }))
}


/// ── Running instance tracking ──────────────────────────────────────
use std::sync::Mutex as StdMutex;

/// Minimal record of a running Minecraft process.
#[derive(Debug, Clone)]
struct RunningGame {
    instance_id: String,
    child: Arc<StdMutex<std::process::Child>>,
    started_at: u64,
}

/// Global list of running game processes, shared across Tauri commands.
static RUNNING_GAMES: once_cell::sync::Lazy<Arc<StdMutex<Vec<RunningGame>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(StdMutex::new(Vec::new())));

#[tauri::command(rename_all = "camelCase")]
fn list_running_instances() -> Result<Vec<serde_json::Value>, String> {
    let games = RUNNING_GAMES.lock().map_err(|e| e.to_string())?;
    Ok(games
        .iter()
        .map(|g| serde_json::json!({
            "instanceId": g.instance_id,
            "startedAt": g.started_at,
        }))
        .collect())
}

#[tauri::command(rename_all = "camelCase")]
fn kill_running_instance(instance_id: String) -> Result<String, String> {
    let mut games = RUNNING_GAMES.lock().map_err(|e| e.to_string())?;
    let idx = games
        .iter()
        .position(|g| g.instance_id == instance_id)
        .ok_or_else(|| format!("no running instance {instance_id}"))?;
    let game = games.remove(idx);
    let mut child = game.child.lock().map_err(|e| e.to_string())?;
    let _ = child.kill();
    let _ = child.wait();
    Ok(format!("Killed {instance_id}"))
}

/// Records that a process was spawned for an instance (called after
/// successful launch).
#[allow(dead_code)]
fn register_running_instance(instance_id: &str, child: std::process::Child) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Ok(mut games) = RUNNING_GAMES.lock() {
        games.push(RunningGame {
            instance_id: instance_id.to_string(),
            child: Arc::new(StdMutex::new(child)),
            started_at: now,
        });
    }
}

/// ────────────────────────────────────────────────────────────────────
fn download_project_mods(manifest_path: &Path, manifest: &ProjectManifest) -> tuffbox_core::ModSyncReport {
    let instance_dir = tuffbox_core::instance_dir_for_manifest(manifest_path)
        .unwrap_or_else(|| manifest_path.parent().map(|p| p.to_path_buf()).unwrap_or_default());
    tuffbox_core::ensure_project_mods_downloaded(manifest, &instance_dir)
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
            sync_mods_folder,
            search_modrinth_mods,
            preview_modrinth_install,
            get_modrinth_project_icon,
            add_modrinth_mod,
            add_modrinth_mod_with_dependencies,
            add_modrinth_mods_with_dependencies,
            remove_project_mod,
            update_project_mod,
            check_mod_updates,
            update_all_mods,
            get_mod_versions,
            change_mod_version,
            detect_wrong_loader_mods,
            disable_wrong_loader_jar,
            remove_loose_jar,
            list_config_files,
            read_config_file,
            write_config_file,
            search_in_configs,
            get_manifest_schema,
            record_launch,
            record_crash,
            get_launch_stats,
            get_graph,
            get_diagnostics,
            run_project_validation,
            check_mod_compatibility,
            compare_modpacks,
            create_project_backup,
            list_backups,
            delete_backup,
            build_ai_crash_context,
            recommend_mods,
            get_mod_info,
            restore_backup,
            save_problematic_mods_config,
            get_problematic_mods_config,
            launch_server,
            generate_server_properties,
            scan_mod_recipes,
            list_worlds,
            backup_world,
            save_as_template,
            list_templates,
            get_download_progress,
            get_keyboard_shortcuts,
            lint_config,
            cleanup_project,
            get_app_version,
            check_for_app_update,
            read_world_info,
            generate_github_release,
            localize,
            list_localizations,
            export_graph_dot,
            export_project_report,
            batch_export_all,
            audit_performance,
            scan_ore_generation,
            detect_duplicate_items,
            generate_unify_config,
            run_crash_assistant_full,
find_class_in_mods,
find_dependents_on_class,
            get_resolve_change_plan,
            apply_resolve_action,
            apply_resolve_change_plan,
            resolve_missing_dependencies,
            get_crash_diagnosis,
            create_crash_fix_plan,
            apply_crash_fix_plan,
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
            diff_manifest_snapshots,
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
            launch_with_quick_play,
            import_project,
            import_curseforge_project,

            has_crashed,
            open_project_folder,
            delete_project,
            create_logs_zip,
            clone_project,
            repair_project,
            get_home_dir,
            list_running_instances,
            kill_running_instance,
            get_minecraft_versions,
            get_loader_versions,
            create_instance,
            find_java_runtimes,
            get_java_version,
            get_default_java_version,
            get_launch_log,
            list_instance_logs,
            read_instance_log,
            get_instance_size,
            pin_project,
            is_project_pinned,
            set_last_opened_project,
            get_last_opened_project,
            update_project_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
