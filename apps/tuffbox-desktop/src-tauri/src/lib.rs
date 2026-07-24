mod auth;
mod create_mode_api;
mod integrations;
mod launcher_settings;
mod presence;
mod swarm_api;
mod swarm_node;
mod task_progress_api;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;
use tuffbox_core::{
    ContentProvider, DependencyGraph, ModSource, ModSpec, PackBrief, ProjectManifest,
    ProviderFileInfo, ProviderSearchQuery, Resolver, Side, Snapshot, SnapshotStore, SourceKind,
    TuffboxLockfile,
};
use tuffbox_core::crash::FixAction;
use tuffbox_core::launch_error::{LaunchErrorInfo, LaunchErrorKind};
use tuffbox_core::process::{OnExit, ProcessExit};
use tauri::Emitter;

/// Serializes manifest + mods-folder mutations so background `sync_mods_folder`
/// cannot overwrite an in-flight Update All / single update.
static MODS_IO_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

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
    /// Canonical manifest file path (may differ from the path passed in).
    manifest_path: String,
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
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    crash_fingerprint_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    plan_source: Option<String>,
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
    let manifest_path = resolve_manifest_path(&path)?;
    let manifest =
        ProjectManifest::load_from_path(&manifest_path).map_err(|e| e.to_string())?;
    manifest.validate_basic().map_err(|e| e.to_string())?;
    let profile = manifest
        .profiles
        .iter()
        .find(|p| p.id == "client")
        .or_else(|| manifest.profiles.first())
        .ok_or_else(|| "project has no profiles".to_string())?;

    Ok(project_summary_from_manifest(&manifest_path, &manifest, profile))
}

#[tauri::command(rename_all = "camelCase")]
fn resolve_project_path(path: String) -> Result<String, String> {
    Ok(resolve_manifest_path(&path)?
        .to_string_lossy()
        .to_string())
}

fn project_summary_from_manifest(
    manifest_path: &Path,
    manifest: &ProjectManifest,
    profile: &tuffbox_core::manifest::ProfileSpec,
) -> ProjectSummary {
    ProjectSummary {
        id: manifest.project.id.clone(),
        name: manifest.project.name.clone(),
        version: manifest.project.version.clone(),
        minecraft_version: manifest.minecraft.version.clone(),
        loader_kind: tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string(),
        loader_version: manifest.loader.version.clone(),
        java_path: manifest.java.as_ref().and_then(|j| j.path.clone()),
        memory_mb: profile.memory_mb.unwrap_or(4096),
        jvm_args: profile.jvm_args.clone(),
        player_name: profile
            .player_name
            .clone()
            .unwrap_or_else(|| "Player".to_string()),
        manifest_path: manifest_path.to_string_lossy().to_string(),
    }
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
        let _guard = MODS_IO_LOCK
            .lock()
            .map_err(|_| "mods I/O lock poisoned".to_string())?;
        let manifest_path = std::path::PathBuf::from(&path);
        let mut manifest =
            ProjectManifest::load_from_path(&manifest_path).map_err(|e| e.to_string())?;

        let mut project_dir = manifest_path.clone();
        if manifest_path.is_file() {
            project_dir.pop();
        }

        // Scan all content folders: mods/, resourcepacks/, shaderpacks/, datapacks/
        let content_dirs: &[(&str, &str, tuffbox_core::manifest::ContentType)] = &[
            ("mods", "jar", tuffbox_core::manifest::ContentType::Mod),
            (
                "resourcepacks",
                "zip",
                tuffbox_core::manifest::ContentType::Resourcepack,
            ),
            (
                "shaderpacks",
                "zip",
                tuffbox_core::manifest::ContentType::Shaderpack,
            ),
            (
                "datapacks",
                "zip",
                tuffbox_core::manifest::ContentType::Datapack,
            ),
        ];

        let provider = tuffbox_core::ModrinthProvider::new();
        let mut hash_index = tuffbox_core::ModHashIndex::load(&project_dir);
        let mut index_dirty = false;
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
                if manifest
                    .mods
                    .iter()
                    .any(|m| m.file_name.as_deref() == Some(&*file_name))
                {
                    continue;
                }

                let Ok(sha1) = tuffbox_core::sha1_file(&entry.path()) else {
                    continue;
                };

                let identified = resolve_mod_from_hash_or_modrinth(
                    &provider,
                    &mut hash_index,
                    &sha1,
                    &entry.path(),
                    file_name.clone(),
                    &mut index_dirty,
                );

                if let Some(mut identified) = identified {
                    identified.file_name = Some(file_name.clone());
                    identified.content_type = default_content_type;
                    // A leftover jar from a prior update (different filename,
                    // same Modrinth project) must not become a second manifest
                    // entry — that is how "updates duplicate mods".
                    let existing_idx = manifest.mods.iter().position(|m| {
                        identified
                            .source
                            .project_id
                            .as_ref()
                            .is_some_and(|pid| m.source.project_id.as_ref() == Some(pid))
                            || m.id == identified.id
                    });
                    if let Some(idx) = existing_idx {
                        let tracked = existing_mod_file_path(&manifest_path, &manifest.mods[idx]);
                        if tracked
                            .as_ref()
                            .is_some_and(|tracked_path| tracked_path != &entry.path())
                        {
                            let _ = std::fs::remove_file(entry.path());
                            continue;
                        }
                        if tracked.is_none() {
                            let keep_id = manifest.mods[idx].id.clone();
                            identified.id = keep_id;
                            manifest.mods[idx] = identified;
                            any_changes = true;
                        }
                        continue;
                    }
                    manifest.mods.push(identified);
                    any_changes = true;
                    continue;
                }

                // Unidentified jar: often a leftover after Update All when the
                // old filename no longer matches the manifest. If another
                // tracked mod already owns a live jar and this file looks like
                // the same slug, delete instead of creating a Local duplicate.
                let stem = file_name.trim_end_matches(&format!(".{}", ext)).to_lowercase();
                let superseded = manifest.mods.iter().any(|m| {
                    if m.source.kind == SourceKind::Local {
                        return false;
                    }
                    let Some(tracked_name) = m.file_name.as_deref() else {
                        return false;
                    };
                    if tracked_name == file_name {
                        return false;
                    }
                    let tracked_path = existing_mod_file_path(&manifest_path, m);
                    if !tracked_path.as_ref().is_some_and(|p| p.is_file()) {
                        return false;
                    }
                    let id = m.id.to_lowercase().replace('_', "-");
                    stem.starts_with(&id) || stem.split('-').next() == Some(id.as_str())
                });
                if superseded {
                    let _ = std::fs::remove_file(entry.path());
                    continue;
                }

                let local_side = tuffbox_core::scan_mod_jar(&entry.path())
                    .map(|r| r.side)
                    .unwrap_or(tuffbox_core::manifest::Side::Unknown);
                let id = file_name.trim_end_matches(&format!(".{}", ext)).to_string();
                manifest.mods.push(tuffbox_core::manifest::ModSpec {
                    id,
                    name: file_name.clone(),
                    version: "unknown".to_string(),
                    side: local_side,
                    source: tuffbox_core::manifest::ModSource {
                        kind: tuffbox_core::manifest::SourceKind::Local,
                        project_id: None,
                        file_id: None,
                        url: None,
                        path: Some(format!("{}/{}", dir_name, file_name)),
                        icon_url: None,
                        categories: Vec::new(),
                    },
                    file_name: Some(file_name),
                    hashes: Some(tuffbox_core::FileHashes {
                        sha1: Some(sha1),
                        sha512: None,
                    }),
                    dependencies: vec![],
                    status: vec![],
                    content_type: default_content_type,
                    authors: tuffbox_core::scan_mod_jar(&entry.path())
                        .map(|r| r.authors)
                        .unwrap_or_default(),
                });
                any_changes = true;
            }
        }

        // Re-identify local-only manifest entries once and cache the result.
        // Already-indexed Modrinth/CurseForge mods are never re-queried for identity.
        for idx in 0..manifest.mods.len() {
            if manifest.mods[idx].source.project_id.is_some() {
                continue;
            }
            let Some(file_name) = manifest.mods[idx].file_name.clone() else {
                continue;
            };
            let file_path =
                tuffbox_core::content_dir_for(&project_dir, manifest.mods[idx].content_type)
                    .join(&file_name);
            if !file_path.is_file() {
                continue;
            }
            let Ok(sha1) = tuffbox_core::sha1_file(&file_path) else {
                continue;
            };
            if let Some(spec) = resolve_mod_from_hash_or_modrinth(
                &provider,
                &mut hash_index,
                &sha1,
                &file_path,
                file_name.clone(),
                &mut index_dirty,
            ) {
                let mut spec = spec;
                spec.file_name = Some(file_name);
                // Keep jar-scan side only when Modrinth still reported Unknown.
                if spec.side == tuffbox_core::manifest::Side::Unknown {
                    spec.side = manifest.mods[idx].side;
                }
                manifest.mods[idx] = spec;
                any_changes = true;
            }
        }

        // One-time (then cached) Modrinth side backfill for already-tracked mods.
        // Old installs defaulted everything to `both`; refresh until client_side /
        // server_side are stored in the hash index.
        for idx in 0..manifest.mods.len() {
            if manifest.mods[idx].source.kind != SourceKind::Modrinth {
                continue;
            }
            let Some(project_id) = manifest.mods[idx].source.project_id.clone() else {
                continue;
            };
            let sha1 = manifest.mods[idx]
                .hashes
                .as_ref()
                .and_then(|h| h.sha1.clone());
            if let Some(ref sha1) = sha1 {
                if let Some(cached) = hash_index.get(sha1) {
                    if cached.client_side.is_some() || cached.server_side.is_some() {
                        let side = tuffbox_core::manifest::Side::from_modrinth(
                            cached.client_side.as_deref(),
                            cached.server_side.as_deref(),
                        );
                        if manifest.mods[idx].side != side {
                            manifest.mods[idx].side = side;
                            any_changes = true;
                        }
                        continue;
                    }
                }
            }
            let Ok(project) = provider.get_project(&project_id) else {
                continue;
            };
            let side = tuffbox_core::manifest::Side::from_modrinth(
                project.client_side.as_deref(),
                project.server_side.as_deref(),
            );
            if manifest.mods[idx].side != side {
                manifest.mods[idx].side = side;
                any_changes = true;
            }
            if let Some(ref sha1) = sha1 {
                hash_index.put_sides(
                    sha1,
                    project.client_side.as_deref(),
                    project.server_side.as_deref(),
                );
                index_dirty = true;
            }
        }

        if any_changes {
            // Re-load disk before save so we don't clobber concurrent updates
            // that finished after our initial load (Update All race).
            if let Ok(disk) = ProjectManifest::load_from_path(&manifest_path) {
                for disk_mod in &disk.mods {
                    if let Some(pid) = disk_mod.source.project_id.as_ref() {
                        if let Some(idx) = manifest.mods.iter().position(|m| {
                            m.source.project_id.as_ref() == Some(pid) || m.id == disk_mod.id
                        }) {
                            // Prefer the newer file_id / version from disk when present.
                            if disk_mod.source.file_id != manifest.mods[idx].source.file_id
                                || disk_mod.version != manifest.mods[idx].version
                            {
                                manifest.mods[idx] = disk_mod.clone();
                            }
                        }
                    }
                }
            }
            save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        }
        if index_dirty {
            let _ = hash_index.save(&project_dir);
        }

        list_mods_impl(&path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Resolve a jar to a ModSpec via the local hash index, refreshing Modrinth
/// side fields once if the cache predates client_side/server_side storage.
fn resolve_mod_from_hash_or_modrinth(
    provider: &tuffbox_core::ModrinthProvider,
    hash_index: &mut tuffbox_core::ModHashIndex,
    sha1: &str,
    jar_path: &Path,
    file_name: String,
    index_dirty: &mut bool,
) -> Option<tuffbox_core::manifest::ModSpec> {
    let cached_status = hash_index.get(sha1).map(|c| {
        (
            c.status.clone(),
            c.client_side.is_none() && c.server_side.is_none(),
            c.project_id.clone(),
        )
    });
    if let Some((status, needs_sides, project_id)) = cached_status {
        if status == "miss" {
            return None;
        }
        if status == "modrinth" {
            if needs_sides {
                if let Some(pid) = project_id {
                    if let Ok(project) = provider.get_project(&pid) {
                        hash_index.put_sides(
                            sha1,
                            project.client_side.as_deref(),
                            project.server_side.as_deref(),
                        );
                        *index_dirty = true;
                    }
                }
            }
            return hash_index
                .get(sha1)
                .and_then(|c| c.to_mod_spec(file_name, tuffbox_core::manifest::Side::Unknown));
        }
    }

    match tuffbox_core::identify_local_jar_via_modrinth(provider, jar_path) {
        Ok(Some((spec, client_side, server_side))) => {
            hash_index.put_modrinth(
                sha1,
                &spec,
                client_side.as_deref(),
                server_side.as_deref(),
            );
            *index_dirty = true;
            Some(spec)
        }
        Ok(None) => {
            hash_index.put_miss(sha1);
            *index_dirty = true;
            None
        }
        Err(_) => None,
    }
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
                    m.source.icon_url.clone().or_else(|| {
                        m.source
                            .project_id
                            .as_ref()
                            .map(|pid| format!("https://cdn.modrinth.com/data/{pid}/icon.png"))
                    })
                }
                tuffbox_core::manifest::SourceKind::Curseforge => m.source.icon_url.clone(),
                _ => m.source.icon_url.clone(),
            };
            let disabled = m
                .status
                .iter()
                .any(|s| s.eq_ignore_ascii_case("disabled"));
            serde_json::json!({
                "id": m.id,
                "name": m.name,
                "version": m.version,
                "side": format!("{:?}", m.side).to_lowercase(),
                "source": m.source.kind.as_str(),
                "projectId": m.source.project_id,
                "fileName": m.file_name,
                "iconUrl": icon_url,
                "contentType": content_type,
                "disabled": disabled,
                "status": m.status,
            })
        })
        .collect();
    Ok(mods)
}

#[tauri::command(rename_all = "camelCase")]
async fn list_mods(path: String) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || list_mods_impl(&path))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PagedCatalog {
    results: Vec<serde_json::Value>,
    total: u32,
}

#[tauri::command(rename_all = "camelCase")]
async fn search_unified_mods(
    path: String,
    query: String,
    game_version: Option<String>,
    loader: Option<String>,
    content_type: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<PagedCatalog, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let page_size = page_size.unwrap_or(30).clamp(1, 100);
        let page = page.unwrap_or(1).max(1);
        let offset = (page - 1) * page_size;
        let per = (page_size / 2).max(1);

        let mr = tuffbox_core::ModrinthProvider::new();
        let default_loader =
            tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string();
        let mr_total;
        let mut mr_hits: Vec<serde_json::Value> = Vec::new();
        if let Ok(page_result) = mr.search(&ProviderSearchQuery {
            query: Some(query.clone()),
            minecraft_version: game_version
                .clone()
                .or_else(|| Some(manifest.minecraft.version.clone())),
            loader: loader.clone().or_else(|| Some(default_loader)),
            limit: Some(per),
            project_type: content_type.clone(),
            offset: Some(offset / 2),
            ..Default::default()
        }) {
            mr_total = page_result.total;
            for p in page_result.results {
                let value = serde_json::to_value(&p).unwrap_or(serde_json::Value::Null);
                let mut obj = match value {
                    serde_json::Value::Object(m) => m,
                    _ => serde_json::Map::new(),
                };
                obj.insert("provider".into(), serde_json::json!("modrinth"));
                mr_hits.push(serde_json::Value::Object(obj));
            }
        } else {
            mr_total = 0;
        }

        let mut cf_hits: Vec<serde_json::Value> = Vec::new();
        let cf_total;
        let cf_provider = tuffbox_core::CurseForgeProvider::new();
        if cf_provider.is_configured() {
            let project_type = content_type.clone().unwrap_or_else(|| "mod".into());
            let class_id = tuffbox_core::CurseForgeProvider::class_id_for_project_type(&project_type);
            let gv = game_version.unwrap_or_else(|| manifest.minecraft.version.clone());
            let loader_slug = loader
                .clone()
                .unwrap_or_else(|| tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string());
            let mod_loader = if project_type == "mod" {
                tuffbox_core::CurseForgeProvider::mod_loader_type(&loader_slug)
            } else {
                None
            };
            if let Ok(page_result) = cf_provider.search_content(
                class_id,
                &query,
                Some(&gv),
                mod_loader,
                offset / 2,
                per,
                None,
            ) {
                cf_total = page_result.total;
                for hit in page_result.hits {
                    let mapped_type = match hit.class_id.unwrap_or(class_id) {
                        12 => "resourcepack",
                        6552 => "shader",
                        6945 => "datapack",
                        4471 => "modpack",
                        _ => "mod",
                    };
                    cf_hits.push(serde_json::json!({
                        "id": hit.id.to_string(),
                        "slug": hit.slug,
                        "name": hit.name,
                        "description": hit.summary,
                        "projectType": mapped_type,
                        "iconUrl": hit.icon_url,
                        "author": hit.authors.first().cloned(),
                        "downloads": hit.download_count,
                        "follows": hit.thumbs_up_count,
                        "dateModified": hit.date_modified.clone().or(hit.date_created.clone()),
                        "categories": hit.categories,
                        "provider": "curseforge",
                    }));
                }
            } else {
                cf_total = 0;
            }
        } else {
            cf_total = 0;
        }

        let mut results: Vec<serde_json::Value> = Vec::with_capacity(page_size as usize);
        let max = mr_hits.len().max(cf_hits.len());
        for i in 0..max {
            if i < mr_hits.len() {
                results.push(mr_hits[i].clone());
            }
            if i < cf_hits.len() {
                results.push(cf_hits[i].clone());
            }
        }
        results.truncate(page_size as usize);

        Ok(PagedCatalog {
            results,
            total: mr_total.saturating_add(cf_total),
        })
    })
    .await
    .map_err(|e| e.to_string())?
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
    page: Option<u32>,
    page_size: Option<u32>,
) -> Result<PagedCatalog, String> {
    tokio::task::spawn_blocking(move || {
        // A manifest is only needed to infer a default loader / game version.
        // When no project is open (empty path) we still allow browsing the
        // Modrinth catalog with the caller-supplied filters.
        let manifest = ProjectManifest::load_from_path(&path).ok();
        let provider = tuffbox_core::ModrinthProvider::new();
        let default_loader = manifest
            .as_ref()
            .map(|m| tuffbox_core::graph::loader_kind_slug(&m.loader.kind).to_string());
        let page_size = page_size.unwrap_or(30).clamp(1, 100);
        let offset = (page.unwrap_or(1).saturating_sub(1)) * page_size;
        let page_result = provider
            .search(&ProviderSearchQuery {
                query: Some(query),
                minecraft_version: game_version
                    .or_else(|| manifest.as_ref().map(|m| m.minecraft.version.clone())),
                loader: loader.or(default_loader),
                category,
                environment,
                license,
                sort,
                limit: Some(page_size),
                project_type: content_type,
                offset: Some(offset),
            })
            .map_err(|e| e.to_string())?;
        let results = page_result
            .results
            .into_iter()
            .map(|p| {
                let value = serde_json::to_value(&p).unwrap_or(serde_json::Value::Null);
                let mut obj = match value {
                    serde_json::Value::Object(m) => m,
                    _ => serde_json::Map::new(),
                };
                obj.insert("provider".into(), serde_json::json!("modrinth"));
                serde_json::Value::Object(obj)
            })
            .collect();
        Ok(PagedCatalog {
            results,
            total: page_result.total,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn search_curseforge_mods(
    path: String,
    query: String,
    game_version: Option<String>,
    loader: Option<String>,
    content_type: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
    sort_field: Option<u32>,
) -> Result<PagedCatalog, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let provider = tuffbox_core::CurseForgeProvider::new();
        if !provider.is_configured() {
            return Err("CurseForge API key is not configured".to_string());
        }
        let project_type = content_type.unwrap_or_else(|| "mod".into());
        let class_id = tuffbox_core::CurseForgeProvider::class_id_for_project_type(&project_type);
        let gv = game_version.unwrap_or_else(|| manifest.minecraft.version.clone());
        let loader_slug = loader
            .clone()
            .unwrap_or_else(|| tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string());
        let mod_loader = if project_type == "mod" {
            tuffbox_core::CurseForgeProvider::mod_loader_type(&loader_slug)
        } else {
            None
        };
        let page_size = page_size.unwrap_or(30).clamp(1, 50);
        let offset = (page.unwrap_or(1).saturating_sub(1)) * page_size;
        let sort_field = sort_field.unwrap_or(2);
        let page_result = provider
            .search_content(class_id, &query, Some(&gv), mod_loader, offset, page_size, Some(sort_field))
            .map_err(|e| e.to_string())?;
        let results = page_result
            .hits
            .into_iter()
            .map(|hit| {
                let mapped_type = match hit.class_id.unwrap_or(class_id) {
                    12 => "resourcepack",
                    6552 => "shader",
                    6945 => "datapack",
                    4471 => "modpack",
                    _ => "mod",
                };
                serde_json::json!({
                    "id": hit.id.to_string(),
                    "slug": hit.slug,
                    "name": hit.name,
                    "description": hit.summary,
                    "projectType": mapped_type,
                    "iconUrl": hit.icon_url,
                    "author": hit.authors.first().cloned(),
                    "downloads": hit.download_count,
                    "follows": hit.thumbs_up_count,
                    "dateModified": hit.date_modified.clone().or(hit.date_created.clone()),
                    "categories": hit.categories,
                    "provider": "curseforge",
                })
            })
            .collect();
        Ok(PagedCatalog {
            results,
            total: page_result.total,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn add_curseforge_mod(
    app: tauri::AppHandle,
    path: String,
    mod_id: String,
    side: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "add-curseforge-mod").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        add_mod_from_curseforge(&mut manifest, &mod_id, Some(side)).map_err(|e| e.to_string())?;
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods_tracked(&app, &manifest_path, &manifest, None, true);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn preview_modrinth_install(
    path: String,
    mod_id: String,
) -> Result<ModInstallPreview, String> {
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
        let file_name = ProviderFileInfo::select_file_for_loader(
            &version,
            &tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind),
        )
        .map(|file| file.filename.clone());
        let dependencies = provider
            .resolve_dependencies(&version.id)
            .unwrap_or_default();
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
async fn get_modrinth_project(project_id: String) -> Result<tuffbox_core::ProjectInfo, String> {
    tokio::task::spawn_blocking(move || {
        let provider = tuffbox_core::ModrinthProvider::new();
        provider.get_project(&project_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Unified catalog project detail for the in-launcher project page
/// (Modrinth or CurseForge), GDLauncher-style.
#[tauri::command(rename_all = "camelCase")]
async fn get_catalog_project(
    provider: String,
    project_id: String,
) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let provider = provider.trim().to_ascii_lowercase();
        if provider == "curseforge" || provider == "cf" {
            let id: u64 = project_id
                .trim()
                .parse()
                .map_err(|_| format!("Invalid CurseForge project id: {project_id}"))?;
            let cf = tuffbox_core::CurseForgeProvider::new();
            if !cf.is_configured() {
                return Err("CurseForge API key is not configured".into());
            }
            let hit = cf.get_mod(id).map_err(|e| e.to_string())?;
            let description_html = cf.get_mod_description_html(id).unwrap_or_default();
            let mapped_type = match hit.class_id.unwrap_or(6) {
                12 => "resourcepack",
                6552 => "shader",
                6945 => "datapack",
                4471 => "modpack",
                _ => "mod",
            };
            return Ok(serde_json::json!({
                "id": hit.id.to_string(),
                "slug": hit.slug,
                "name": hit.name,
                "description": hit.summary,
                "descriptionHtml": description_html,
                "projectType": mapped_type,
                "iconUrl": hit.icon_url,
                "author": hit.authors.first().cloned(),
                "authors": hit.authors,
                "downloads": hit.download_count,
                "follows": hit.thumbs_up_count,
                "dateModified": hit.date_modified.clone().or(hit.date_created.clone()),
                "categories": hit.categories,
                "provider": "curseforge",
            }));
        }

        let mr = tuffbox_core::ModrinthProvider::new();
        let (project, body_md) = mr
            .get_project_with_body(&project_id)
            .map_err(|e| e.to_string())?;
        let description_html = body_md
            .as_deref()
            .map(tuffbox_core::markdown_to_html)
            .filter(|s| !s.trim().is_empty());
        Ok(serde_json::json!({
            "id": project.id,
            "slug": project.slug,
            "name": project.name,
            "description": project.description,
            "descriptionHtml": description_html,
            "projectType": project.project_type,
            "iconUrl": project.icon_url,
            "author": project.author,
            "authors": project.author.clone().map(|a| vec![a]).unwrap_or_default(),
            "downloads": project.downloads,
            "follows": project.follows,
            "dateModified": project.date_modified,
            "categories": project.categories,
            "license": project.license,
            "clientSide": project.client_side,
            "serverSide": project.server_side,
            "provider": "modrinth",
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Versions/files for the in-launcher project page.
#[tauri::command(rename_all = "camelCase")]
async fn get_catalog_versions(
    provider: String,
    project_id: String,
    minecraft_version: Option<String>,
    loader: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let provider_l = provider.trim().to_ascii_lowercase();
    if provider_l == "curseforge" || provider_l == "cf" {
        return tokio::task::spawn_blocking(move || {
            let id: u64 = project_id
                .trim()
                .parse()
                .map_err(|_| format!("Invalid CurseForge project id: {project_id}"))?;
            let cf = tuffbox_core::CurseForgeProvider::new();
            if !cf.is_configured() {
                return Err("CurseForge API key is not configured".into());
            }
            let gv = minecraft_version.as_deref().filter(|s| !s.is_empty());
            let files = cf.get_mod_files(id, gv).map_err(|e| e.to_string())?;
            let loader_slug = loader
                .as_deref()
                .map(|l| l.trim().to_lowercase())
                .filter(|l| !l.is_empty());
            let mut rows: Vec<serde_json::Value> = files
                .into_iter()
                .map(|f| {
                    let mc_ok = gv
                        .map(|v| f.game_versions.iter().any(|g| g == v))
                        .unwrap_or(true);
                    let loader_ok = match &loader_slug {
                        Some(l) => f
                            .game_versions
                            .iter()
                            .any(|g| g.eq_ignore_ascii_case(l) || (*l == "quilt" && g.eq_ignore_ascii_case("fabric"))),
                        None => true,
                    };
                    let channel = match f.release_type {
                        1 => "release",
                        2 => "beta",
                        3 => "alpha",
                        _ => "release",
                    };
                    serde_json::json!({
                        "id": f.id.to_string(),
                        "versionNumber": f.display_name,
                        "name": f.file_name,
                        "gameVersions": f.game_versions,
                        "loaders": [],
                        "datePublished": f.file_date,
                        "versionType": channel,
                        "compatible": mc_ok && loader_ok,
                        "compatibleMinecraft": mc_ok,
                        "compatibleLoader": loader_ok,
                    })
                })
                .collect();
            rows.sort_by(|a, b| {
                let ad = a.get("datePublished").and_then(|v| v.as_str()).unwrap_or("");
                let bd = b.get("datePublished").and_then(|v| v.as_str()).unwrap_or("");
                bd.cmp(ad)
            });
            Ok(rows)
        })
        .await
        .map_err(|e| e.to_string())?;
    }

    get_mod_versions(project_id, minecraft_version.unwrap_or_default(), loader).await
}

/// Resolves the download URL of the latest Modrinth modpack file (.mrpack) for
/// a project, so the Library "Discover" tab can import a remote pack directly
/// via `install_modpack`.
#[tauri::command(rename_all = "camelCase")]
async fn get_modrinth_pack_download(project_id: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let provider = tuffbox_core::ModrinthProvider::new();
        let versions = provider
            .get_versions(&project_id, &ProviderSearchQuery::default())
            .map_err(|e| e.to_string())?;
        for version in &versions {
            for file in &version.files {
                if file.filename.to_lowercase().ends_with(".mrpack") {
                    return Ok(file.url.clone());
                }
            }
        }
        // Fallback: any primary file if no .mrpack is published.
        for version in &versions {
            if let Some(primary) = version.files.iter().find(|f| f.primary) {
                return Ok(primary.url.clone());
            }
        }
        Err("No downloadable file found for this modpack.".into())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Per-project user state for mods found in the Add-Mod browser.
/// `favorites` is a single set of mod IDs the user liked.
/// `lists` is a map of list_name -> ordered list of mod IDs, supporting
/// multiple named build lists (e.g. "Performance", "PvP", "QoL").
/// `ratings` stores per-mod star ratings (0–5).
/// Stored as JSON under `.tuffbox/` so it survives restarts without
/// polluting the manifest.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModUserState {
    #[serde(default)]
    favorites: std::collections::HashMap<String, bool>,
    #[serde(default)]
    lists: std::collections::HashMap<String, Vec<String>>,
    #[serde(default)]
    ratings: std::collections::HashMap<String, u8>,
}

fn mod_user_state_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("mods_user_state.json")
}

fn load_mod_user_state(project_dir: &Path) -> ModUserState {
    let p = mod_user_state_path(project_dir);
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_mod_user_state(project_dir: &Path, state: &ModUserState) -> Result<(), String> {
    let p = mod_user_state_path(project_dir);
    if let Some(par) = p.parent() {
        std::fs::create_dir_all(par).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &p,
        serde_json::to_string_pretty(state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn get_mod_user_state(path: String) -> Result<ModUserState, String> {
    let project_dir = manifest_parent(&path)?;
    Ok(load_mod_user_state(&project_dir))
}

#[tauri::command(rename_all = "camelCase")]
fn set_mod_user_state(
    path: String,
    mod_id: String,
    favorite: Option<bool>,
    saved: Option<bool>,
    rating: Option<u8>,
) -> Result<ModUserState, String> {
    let project_dir = manifest_parent(&path)?;
    let mut state = load_mod_user_state(&project_dir);
    if let Some(f) = favorite {
        if f {
            state.favorites.insert(mod_id.clone(), true);
        } else {
            state.favorites.remove(&mod_id);
        }
    }
    // Legacy `saved` flag is kept for backward compat: adds/removes the mod
    // from a default list named "Saved". New UI should use `add_to_list` /
    // `remove_from_list` / `create_list` / `delete_list` instead.
    if let Some(s) = saved {
        const DEFAULT_LIST: &str = "Saved";
        let entry = state.lists.entry(DEFAULT_LIST.to_string()).or_default();
        if s {
            if !entry.contains(&mod_id) {
                entry.push(mod_id.clone());
            }
        } else {
            entry.retain(|m| m != &mod_id);
            if entry.is_empty() {
                state.lists.remove(DEFAULT_LIST);
            }
        }
    }
    if let Some(r) = rating {
        if r == 0 {
            state.ratings.remove(&mod_id);
        } else {
            state.ratings.insert(mod_id.clone(), r.min(5));
        }
    }
    save_mod_user_state(&project_dir, &state)?;
    Ok(state)
}

/// Creates a new named build list (empty).
#[tauri::command(rename_all = "camelCase")]
fn create_mod_list(path: String, name: String) -> Result<ModUserState, String> {
    let project_dir = manifest_parent(&path)?;
    let mut state = load_mod_user_state(&project_dir);
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("List name cannot be empty".to_string());
    }
    if !state.lists.contains_key(&trimmed) {
        state.lists.insert(trimmed, Vec::new());
    }
    save_mod_user_state(&project_dir, &state)?;
    Ok(state)
}

/// Deletes a named build list entirely.
#[tauri::command(rename_all = "camelCase")]
fn delete_mod_list(path: String, name: String) -> Result<ModUserState, String> {
    let project_dir = manifest_parent(&path)?;
    let mut state = load_mod_user_state(&project_dir);
    state.lists.remove(&name);
    save_mod_user_state(&project_dir, &state)?;
    Ok(state)
}

/// Renames a build list.
#[tauri::command(rename_all = "camelCase")]
fn rename_mod_list(
    path: String,
    old_name: String,
    new_name: String,
) -> Result<ModUserState, String> {
    let project_dir = manifest_parent(&path)?;
    let mut state = load_mod_user_state(&project_dir);
    let trimmed = new_name.trim().to_string();
    if trimmed.is_empty() {
        return Err("List name cannot be empty".to_string());
    }
    if let Some(mods) = state.lists.remove(&old_name) {
        state.lists.insert(trimmed, mods);
    }
    save_mod_user_state(&project_dir, &state)?;
    Ok(state)
}

/// Adds a mod to a named build list (creates the list if it doesn't exist).
#[tauri::command(rename_all = "camelCase")]
fn add_to_mod_list(path: String, name: String, mod_id: String) -> Result<ModUserState, String> {
    let project_dir = manifest_parent(&path)?;
    let mut state = load_mod_user_state(&project_dir);
    let entry = state.lists.entry(name).or_default();
    if !entry.contains(&mod_id) {
        entry.push(mod_id);
    }
    save_mod_user_state(&project_dir, &state)?;
    Ok(state)
}

/// Removes a mod from a named build list. If the list becomes empty
/// it is kept (user might want to add more mods later).
#[tauri::command(rename_all = "camelCase")]
fn remove_from_mod_list(
    path: String,
    name: String,
    mod_id: String,
) -> Result<ModUserState, String> {
    let project_dir = manifest_parent(&path)?;
    let mut state = load_mod_user_state(&project_dir);
    if let Some(entry) = state.lists.get_mut(&name) {
        entry.retain(|m| m != &mod_id);
    }
    save_mod_user_state(&project_dir, &state)?;
    Ok(state)
}

#[tauri::command(rename_all = "camelCase")]
async fn install_steam_bridge(
    app: tauri::AppHandle,
    path: String,
) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "install-steam-bridge").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;

        if tuffbox_core::steam_bridge::project_has_steam_bridge(&manifest.mods) {
            return Err("Steam Bridge is already in this pack.".into());
        }

        let asset = tuffbox_core::steam_bridge::resolve_steam_bridge_asset(
            &manifest.minecraft.version,
            &manifest.loader.kind,
        )?;
        let match_note = match asset.match_kind {
            tuffbox_core::steam_bridge::SteamBridgeMatchKind::Exact => "exact match",
            tuffbox_core::steam_bridge::SteamBridgeMatchKind::SameMinor => {
                "closest same minor (exact jar not published)"
            }
        };
        let file_name = asset.file_name.clone();
        let mc = asset.mc_version.clone();
        let tag = asset.tag.clone();
        let loader_label = asset.loader_label.clone();
        let spec = tuffbox_core::steam_bridge::build_steam_bridge_mod_spec(&asset);
        manifest.mods.push(spec);
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods_tracked(&app, &manifest_path, &manifest, None, true);

        Ok(serde_json::json!({
            "modId": tuffbox_core::steam_bridge::STEAM_BRIDGE_MOD_ID,
            "fileName": file_name,
            "tag": tag,
            "mcVersion": mc,
            "loader": loader_label,
            "matchKind": match_note,
            "repo": tuffbox_core::steam_bridge::STEAM_BRIDGE_REPO,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn add_modrinth_mod(
    app: tauri::AppHandle,
    path: String,
    mod_id: String,
    side: String,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        auto_snapshot(&PathBuf::from(&path), "add-mod").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        add_mod_from_modrinth(&mut manifest, &mod_id, Some(side)).map_err(|e| e.to_string())?;
        save_manifest(&PathBuf::from(&path), &manifest).map_err(|e| e.to_string())?;
        download_project_mods_tracked(&app, &PathBuf::from(&path), &manifest, None, true);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn add_modrinth_mod_with_dependencies(
    app: tauri::AppHandle,
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
        download_project_mods_tracked(&app, &manifest_path, &manifest, None, true);
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn add_modrinth_mods_with_dependencies(
    app: tauri::AppHandle,
    path: String,
    mod_ids: Vec<String>,
    side: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "bulk-add-mods-with-dependencies")
            .map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let installed = install_modrinth_with_dependencies(&mut manifest, &mod_ids, &side);
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods_tracked(&app, &manifest_path, &manifest, None, true);
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn remove_project_mod(path: String, mod_id: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "remove-mod").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let removed_idx = manifest
            .mods
            .iter()
            .position(|m| {
                m.id == mod_id
                    || m.source.project_id.as_deref() == Some(mod_id.as_str())
                    || m.file_name.as_deref() == Some(mod_id.as_str())
            })
            .ok_or_else(|| format!("mod {mod_id} not found in project"))?;
        let removed_mod = manifest.mods.remove(removed_idx);
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;

        // Prefer live disk hash so we can also clear leftover renamed jars
        // and the persistent Modrinth hash index.
        let mut sha1 = removed_mod
            .hashes
            .as_ref()
            .and_then(|h| h.sha1.clone())
            .filter(|s| !s.is_empty());
        if let Some(path_on_disk) = existing_mod_file_path(&manifest_path, &removed_mod) {
            if let Ok(hash) = tuffbox_core::sha1_file(&path_on_disk) {
                sha1 = Some(hash);
            }
        }

        remove_mod_file_from_disk(&manifest_path, &removed_mod);
        if let Some(ref hash) = sha1 {
            // Drop any jar with the same bytes (renames / .disabled leftovers),
            // but skip files still tracked by other manifest entries so we
            // don't accidentally delete a second copy of the same mod.
            if let Some(instance_dir) = tuffbox_core::instance_dir_for_manifest(&manifest_path) {
                let remaining_names: std::collections::HashSet<&str> = manifest
                    .mods
                    .iter()
                    .filter_map(|m| m.file_name.as_deref())
                    .collect();
                let content_dir =
                    tuffbox_core::content_dir_for(&instance_dir, removed_mod.content_type);
                if let Ok(entries) = std::fs::read_dir(&content_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if !path.is_file() {
                            continue;
                        }
                        // Skip files still referenced by another manifest entry.
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if remaining_names.contains(name) {
                                continue;
                            }
                        }
                        if let Ok(actual) = tuffbox_core::sha1_file(&path) {
                            if actual.eq_ignore_ascii_case(hash) {
                                let _ = std::fs::remove_file(path);
                            }
                        }
                    }
                }
            }
        }

        if let Some(instance_dir) = tuffbox_core::instance_dir_for_manifest(&manifest_path) {
            let mut index = tuffbox_core::ModHashIndex::load(&instance_dir);
            if let Some(hash) = sha1.as_deref() {
                index.remove_sha1(hash);
            }
            index.remove_id(&removed_mod.id);
            if let Some(pid) = removed_mod.source.project_id.as_deref() {
                index.remove_project(pid);
            }
            let _ = index.save(&instance_dir);
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Soft-disable a tracked mod by renaming its jar to `*.disabled` (Prism/Minecraft
/// convention). Keeps the manifest entry so it can be re-enabled later.
#[tauri::command(rename_all = "camelCase")]
async fn disable_project_mod(path: String, mod_id: String) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "disable-mod").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let idx = manifest
            .mods
            .iter()
            .position(|m| {
                m.id == mod_id
                    || m.source.project_id.as_deref() == Some(mod_id.as_str())
                    || m.file_name.as_deref() == Some(mod_id.as_str())
            })
            .ok_or_else(|| format!("mod {mod_id} not found in project"))?;
        let module = &mut manifest.mods[idx];
        if module.status.iter().any(|s| s.eq_ignore_ascii_case("disabled")) {
            return Ok(serde_json::json!({
                "id": module.id,
                "disabled": true,
                "alreadyDisabled": true,
                "fileName": module.file_name,
            }));
        }
        let Some(file_name) = module.file_name.clone() else {
            return Err(format!("{} has no file name to disable", module.name));
        };
        let Some(instance_dir) = tuffbox_core::instance_dir_for_manifest(&manifest_path) else {
            return Err("could not resolve instance directory".to_string());
        };
        let content_dir = tuffbox_core::content_dir_for(&instance_dir, module.content_type);
        let active = content_dir.join(&file_name);
        let disabled = content_dir.join(format!("{file_name}.disabled"));
        if disabled.is_file() && !active.is_file() {
            // Already renamed on disk — just mark the status.
        } else if active.is_file() {
            if disabled.exists() {
                let _ = std::fs::remove_file(&disabled);
            }
            std::fs::rename(&active, &disabled).map_err(|e| {
                format!(
                    "failed to rename {} → {}.disabled: {e}",
                    active.display(),
                    file_name
                )
            })?;
        } else {
            return Err(format!(
                "{} not found on disk (looked for {} and {}.disabled)",
                module.name, file_name, file_name
            ));
        }
        if !module
            .status
            .iter()
            .any(|s| s.eq_ignore_ascii_case("disabled"))
        {
            module.status.push("disabled".to_string());
        }
        let id = module.id.clone();
        let name = module.name.clone();
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "id": id,
            "name": name,
            "disabled": true,
            "alreadyDisabled": false,
            "fileName": format!("{file_name}.disabled"),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Re-enable a previously disabled mod by renaming `*.disabled` back.
#[tauri::command(rename_all = "camelCase")]
async fn enable_project_mod(path: String, mod_id: String) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "enable-mod").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let idx = manifest
            .mods
            .iter()
            .position(|m| {
                m.id == mod_id
                    || m.source.project_id.as_deref() == Some(mod_id.as_str())
                    || m.file_name.as_deref() == Some(mod_id.as_str())
            })
            .ok_or_else(|| format!("mod {mod_id} not found in project"))?;
        let module = &mut manifest.mods[idx];
        let Some(file_name) = module.file_name.clone() else {
            return Err(format!("{} has no file name to enable", module.name));
        };
        let Some(instance_dir) = tuffbox_core::instance_dir_for_manifest(&manifest_path) else {
            return Err("could not resolve instance directory".to_string());
        };
        let content_dir = tuffbox_core::content_dir_for(&instance_dir, module.content_type);
        let active = content_dir.join(&file_name);
        let disabled = content_dir.join(format!("{file_name}.disabled"));
        if active.is_file() {
            // Already active.
        } else if disabled.is_file() {
            std::fs::rename(&disabled, &active).map_err(|e| {
                format!(
                    "failed to rename {}.disabled → {}: {e}",
                    file_name,
                    active.display()
                )
            })?;
        } else {
            return Err(format!(
                "{} is not present as either {} or {}.disabled",
                module.name, file_name, file_name
            ));
        }
        module
            .status
            .retain(|s| !s.eq_ignore_ascii_case("disabled"));
        let id = module.id.clone();
        let name = module.name.clone();
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        Ok(serde_json::json!({
            "id": id,
            "name": name,
            "disabled": false,
            "fileName": file_name,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn update_project_mod(
    app: tauri::AppHandle,
    path: String,
    mod_id: String,
    version_id: Option<String>,
) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let _guard = MODS_IO_LOCK
            .lock()
            .map_err(|_| "mods I/O lock poisoned".to_string())?;
        emit_mod_update_progress(
            &app,
            "preparing",
            "Creating a safety snapshot…",
            0,
            1,
            5,
            Some(&mod_id),
        );
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "update-mod").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let old_mod = manifest
            .mods
            .iter()
            .find(|module| {
                module.id == mod_id || module.source.project_id.as_deref() == Some(mod_id.as_str())
            })
            .cloned()
            .ok_or_else(|| format!("mod {mod_id} not found in project"))?;
        emit_mod_update_progress(
            &app,
            "resolving",
            &format!("Resolving the latest version of {}…", old_mod.name),
            0,
            1,
            20,
            Some(&old_mod.id),
        );
        update_mod_from_modrinth(
            &manifest_path,
            &mut manifest,
            &mod_id,
            version_id.as_deref(),
        )
        .map_err(|e| e.to_string())?;
        emit_mod_update_progress(
            &app,
            "downloading",
            &format!("Downloading {}…", old_mod.name),
            0,
            1,
            40,
            Some(&old_mod.id),
        );
        let report = commit_single_mod_update(&app, &manifest_path, &mut manifest, &old_mod, true)?;
        emit_mod_update_progress(
            &app,
            "done",
            &format!("{} was updated successfully.", old_mod.name),
            1,
            1,
            100,
            Some(&old_mod.id),
        );
        Ok(serde_json::json!({
            "modId": mod_id,
            "download": report,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Returns available versions for a Modrinth project.
/// Loads *all* project versions (like Modrinth App's content updater), marks
/// compatibility against the given Minecraft version + loader, and sorts
/// compatible releases first. The UI can hide incompatible rows by default.
#[tauri::command(rename_all = "camelCase")]
async fn get_mod_versions(
    mod_id: String,
    minecraft_version: String,
    loader: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || {
        let provider = tuffbox_core::ModrinthProvider::new();
        let loader_slug = loader
            .as_deref()
            .map(|l| l.trim().to_lowercase())
            .filter(|l| !l.is_empty());
        // Fetch unfiltered — filter/mark compatibility client-side style
        // (Modrinth App ContentUpdaterModal pattern).
        let query = ProviderSearchQuery {
            query: None,
            minecraft_version: None,
            loader: None,
            ..Default::default()
        };
        let mut versions = provider
            .get_versions(&mod_id, &query)
            .map_err(|e| e.to_string())?;

        // Newest first (Modrinth usually returns that already; don't rely on it).
        versions.sort_by(|a, b| {
            b.date_published
                .as_deref()
                .unwrap_or("")
                .cmp(a.date_published.as_deref().unwrap_or(""))
        });

        let mut rows: Vec<serde_json::Value> = versions
            .into_iter()
            .map(|v| {
                let mc_ok = v.game_versions.iter().any(|gv| gv == &minecraft_version);
                let loader_ok = match &loader_slug {
                    Some(loader) => v
                        .loaders
                        .iter()
                        .any(|l| l == loader || (*loader == "quilt" && l == "fabric")),
                    None => true,
                };
                let compatible = mc_ok && loader_ok;
                serde_json::json!({
                    "id": v.id,
                    "versionNumber": v.version_number,
                    "gameVersions": v.game_versions,
                    "loaders": v.loaders,
                    "name": v.name,
                    "changelog": v.changelog,
                    "datePublished": v.date_published,
                    "versionType": v.version_type.unwrap_or_else(|| "release".to_string()),
                    "compatible": compatible,
                    "compatibleMinecraft": mc_ok,
                    "compatibleLoader": loader_ok,
                })
            })
            .collect();

        // Compatible first, then by channel preference (release > beta > alpha).
        rows.sort_by(|a, b| {
            let a_ok = a
                .get("compatible")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let b_ok = b
                .get("compatible")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            match (a_ok, b_ok) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => {
                    let rank = |row: &serde_json::Value| match row
                        .get("versionType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("release")
                    {
                        "release" => 0,
                        "beta" => 1,
                        "alpha" => 2,
                        _ => 3,
                    };
                    rank(a).cmp(&rank(b)).then_with(|| {
                        b.get("datePublished")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .cmp(
                                a.get("datePublished")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(""),
                            )
                    })
                }
            }
        });

        Ok(rows)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Changes a mod entry to a specific version (identified by Modrinth
/// version id), downloading the new file and updating metadata in the
/// manifest.
#[tauri::command(rename_all = "camelCase")]
async fn change_mod_version(
    app: tauri::AppHandle,
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
        let file = ProviderFileInfo::select_file_for_loader(
            &version_info,
            &tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind),
        )
        .cloned()
        .ok_or_else(|| format!("no primary file for version {}", version_info.id))?;
        let idx = manifest
            .mods
            .iter()
            .position(|m| m.id == mod_id)
            .ok_or_else(|| format!("mod {mod_id} not found in project"))?;

        let old_mod = manifest.mods[idx].clone();
        let side = infer_project_side(Some(&project));
        let previous_deps = old_mod.dependencies.clone();
        let dependencies = provider
            .resolve_dependencies(&version_info.id)
            .unwrap_or(previous_deps);
        let mut new_spec = build_mod_spec(&project, &version_info, file, dependencies, side);
        // Keep the stable UI / dependency id across version switches.
        new_spec.id = old_mod.id.clone();
        manifest.mods[idx] = new_spec;

        let report = commit_single_mod_update(&app, &manifest_path, &mut manifest, &old_mod, true)?;

        Ok(serde_json::json!({
            "version": version_info.version_number,
            "id": version_info.id,
            "download": report,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Inner helper: soft-disable a tracked mod (mirrors the `disable_project_mod`
/// command without the async wrapper) so the fix command can reuse it.
fn disable_project_mod_inner(
    manifest_path: &Path,
    mod_id: &str,
) -> Result<serde_json::Value, String> {
    auto_snapshot(manifest_path, "disable-mod").map_err(|e| e.to_string())?;
    let mut manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    let idx = manifest
        .mods
        .iter()
        .position(|m| {
            m.id == mod_id
                || m.source.project_id.as_deref() == Some(mod_id)
                || m.file_name.as_deref() == Some(mod_id)
        })
        .ok_or_else(|| format!("mod {mod_id} not found in project"))?;
    let module = &mut manifest.mods[idx];
    if module
        .status
        .iter()
        .any(|s| s.eq_ignore_ascii_case("disabled"))
    {
        return Ok(serde_json::json!({
            "id": module.id,
            "disabled": true,
            "alreadyDisabled": true,
            "fileName": module.file_name,
        }));
    }
    let Some(file_name) = module.file_name.clone() else {
        return Err(format!("{} has no file name to disable", module.name));
    };
    let Some(instance_dir) = tuffbox_core::instance_dir_for_manifest(manifest_path) else {
        return Err("could not resolve instance directory".to_string());
    };
    let content_dir = tuffbox_core::content_dir_for(&instance_dir, module.content_type);
    let active = content_dir.join(&file_name);
    let disabled = content_dir.join(format!("{file_name}.disabled"));
    if disabled.is_file() && !active.is_file() {
        // Already renamed on disk.
    } else if active.is_file() {
        if disabled.exists() {
            let _ = std::fs::remove_file(&disabled);
        }
        std::fs::rename(&active, &disabled).map_err(|e| {
            format!("failed to rename {} → {}.disabled: {e}", active.display(), file_name)
        })?;
    } else {
        return Err(format!(
            "{} not found on disk (looked for {} and {}.disabled)",
            module.name, file_name, file_name
        ));
    }
    if !module
        .status
        .iter()
        .any(|s| s.eq_ignore_ascii_case("disabled"))
    {
        module.status.push("disabled".to_string());
    }
    let id = module.id.clone();
    save_manifest(manifest_path, &manifest).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "id": id,
        "disabled": true,
        "alreadyDisabled": false,
        "fileName": format!("{file_name}.disabled"),
    }))
}

/// Inner helper: remove a tracked mod (mirrors `remove_project_mod`).
fn remove_project_mod_inner(manifest_path: &Path, mod_id: &str) -> Result<(), String> {
    auto_snapshot(manifest_path, "remove-mod").map_err(|e| e.to_string())?;
    let mut manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    let removed_idx = manifest
        .mods
        .iter()
        .position(|m| {
            m.id == mod_id
                || m.source.project_id.as_deref() == Some(mod_id)
                || m.file_name.as_deref() == Some(mod_id)
        })
        .ok_or_else(|| format!("mod {mod_id} not found in project"))?;
    let removed_mod = manifest.mods.remove(removed_idx);
    save_manifest(manifest_path, &manifest).map_err(|e| e.to_string())?;
    remove_mod_file_from_disk(manifest_path, &removed_mod);
    Ok(())
}

/// Updates a single mod to the newest version compatible with the project's
/// current Minecraft version + loader. Returns a short summary of what was
/// applied. Used by the crash-diagnosis Fix buttons ("update mod").
fn apply_mod_update_to_latest(
    app: &tauri::AppHandle,
    manifest_path: &Path,
    manifest: &mut ProjectManifest,
    mod_id: &str,
) -> Result<String, String> {
    let idx = manifest
        .mods
        .iter()
        .position(|m| {
            m.id == mod_id
                || m.source.project_id.as_deref() == Some(mod_id)
                || m.file_name.as_deref() == Some(mod_id)
        })
        .ok_or_else(|| format!("mod {mod_id} not found in project"))?;
    let old_mod = manifest.mods[idx].clone();
    let project_id = old_mod
        .source
        .project_id
        .clone()
        .ok_or_else(|| format!("{} is not a Modrinth mod and cannot be auto-updated", old_mod.name))?;
    let (loader_slug, loaders) = update_loaders_for(manifest);
    let provider = tuffbox_core::ModrinthProvider::new();
    let query = ProviderSearchQuery {
        query: None,
        minecraft_version: Some(manifest.minecraft.version.clone()),
        loader: Some(loader_slug.clone()),
        ..Default::default()
    };
    let mut versions = provider
        .get_versions(&project_id, &query)
        .map_err(|e| e.to_string())?;
    versions.sort_by(|a, b| {
        b.date_published
            .as_deref()
            .unwrap_or("")
            .cmp(a.date_published.as_deref().unwrap_or(""))
    });
    let version_info = versions
        .into_iter()
        .find(|v| v.loaders.iter().any(|l| loaders.iter().any(|s| s == l)))
        .ok_or_else(|| format!("no compatible update for {} on this loader", old_mod.name))?;
    if old_mod.version == version_info.version_number {
        return Ok(format!("{} is already on the latest version", old_mod.name));
    }
    let project = provider
        .get_project(&version_info.project_id)
        .map_err(|e| e.to_string())?;
    let file = ProviderFileInfo::select_file_for_loader(&version_info, &loader_slug)
        .cloned()
        .ok_or_else(|| format!("no primary file for version {}", version_info.id))?;
    let dependencies = provider
        .resolve_dependencies(&version_info.id)
        .unwrap_or_else(|_| old_mod.dependencies.clone());
    let mut new_spec = build_mod_spec(
        &project,
        &version_info,
        file,
        dependencies,
        infer_project_side(Some(&project)),
    );
    new_spec.id = old_mod.id.clone();
    manifest.mods[idx] = new_spec;
    let report = commit_single_mod_update(app, manifest_path, manifest, &old_mod, true)?;
    let _ = report;
    Ok(format!(
        "Updated {} → {}",
        old_mod.name, version_info.version_number
    ))
}

/// Reinstalls a mod by removing its tracked entry + jar and re-fetching the
/// current compatible version from Modrinth. Used by the "reinstall mod" fix.
fn apply_mod_reinstall(
    app: &tauri::AppHandle,
    manifest_path: &Path,
    manifest: &mut ProjectManifest,
    mod_id: &str,
) -> Result<String, String> {
    let idx = manifest
        .mods
        .iter()
        .position(|m| {
            m.id == mod_id
                || m.source.project_id.as_deref() == Some(mod_id)
                || m.file_name.as_deref() == Some(mod_id)
        })
        .ok_or_else(|| format!("mod {mod_id} not found in project"))?;
    let old_mod = manifest.mods[idx].clone();
    let project_id = old_mod
        .source
        .project_id
        .clone()
        .ok_or_else(|| format!("{} is not a Modrinth mod and cannot be reinstalled", old_mod.name))?;
    let (loader_slug, loaders) = update_loaders_for(manifest);
    let provider = tuffbox_core::ModrinthProvider::new();
    let query = ProviderSearchQuery {
        query: None,
        minecraft_version: Some(manifest.minecraft.version.clone()),
        loader: Some(loader_slug.clone()),
        ..Default::default()
    };
    let mut versions = provider
        .get_versions(&project_id, &query)
        .map_err(|e| e.to_string())?;
    versions.sort_by(|a, b| {
        b.date_published
            .as_deref()
            .unwrap_or("")
            .cmp(a.date_published.as_deref().unwrap_or(""))
    });
    let version_info = versions
        .into_iter()
        .find(|v| v.loaders.iter().any(|l| loaders.iter().any(|s| s == l)))
        .ok_or_else(|| format!("no compatible version for {} on this loader", old_mod.name))?;
    let project = provider
        .get_project(&version_info.project_id)
        .map_err(|e| e.to_string())?;
    let file = ProviderFileInfo::select_file_for_loader(&version_info, &loader_slug)
        .cloned()
        .ok_or_else(|| format!("no primary file for version {}", version_info.id))?;
    let dependencies = provider
        .resolve_dependencies(&version_info.id)
        .unwrap_or_default();
    manifest.mods.remove(idx);
    remove_mod_file_from_disk(manifest_path, &old_mod);
    let mut new_spec = build_mod_spec(
        &project,
        &version_info,
        file,
        dependencies,
        infer_project_side(Some(&project)),
    );
    new_spec.id = old_mod.id.clone();
    manifest.mods.push(new_spec);
    let report = commit_single_mod_update(app, manifest_path, manifest, &old_mod, true)?;
    let _ = report;
    Ok(format!("Reinstalled {} ({})", old_mod.name, version_info.version_number))
}

/// Applies a machine-actionable fix produced by crash diagnosis.
#[tauri::command(rename_all = "camelCase")]
async fn apply_fix_action(
    app: tauri::AppHandle,
    path: String,
    action: FixAction,
) -> Result<String, String> {
    let manifest_path = PathBuf::from(&path);
    let path_for_record = path.clone();
    let mod_id = action.mod_id.clone().unwrap_or_default();
    let action_for_record = action.clone();
    let result = tokio::task::spawn_blocking(move || {
        match action.kind.as_str() {
            "disableMod" => {
                if mod_id.is_empty() {
                    return Err("disableMod requires a mod id".into());
                }
                let res = disable_project_mod_inner(&manifest_path, &mod_id)?;
                Ok(format!(
                    "Disabled {} ({})",
                    res.get("id").and_then(|v| v.as_str()).unwrap_or(&mod_id),
                    res.get("fileName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("jar")
                ))
            }
            "removeMod" => {
                if mod_id.is_empty() {
                    return Err("removeMod requires a mod id".into());
                }
                remove_project_mod_inner(&manifest_path, &mod_id)?;
                Ok(format!("Removed {mod_id}"))
            }
            "reinstallMod" => {
                if mod_id.is_empty() {
                    return Err("reinstallMod requires a mod id".into());
                }
                auto_snapshot(&manifest_path, "fix-reinstall-mod").map_err(|e| e.to_string())?;
                let mut manifest =
                    ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
                let msg =
                    apply_mod_reinstall(&app, &manifest_path, &mut manifest, &mod_id)?;
                save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
                Ok(msg)
            }
            "updateMod" => {
                if mod_id.is_empty() {
                    return Err("updateMod requires a mod id".into());
                }
                auto_snapshot(&manifest_path, "fix-update-mod").map_err(|e| e.to_string())?;
                let mut manifest =
                    ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
                let msg = apply_mod_update_to_latest(&app, &manifest_path, &mut manifest, &mod_id)?;
                save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
                Ok(msg)
            }
            "installDependency" => {
                if mod_id.is_empty() {
                    return Err("installDependency requires a mod id".into());
                }
                auto_snapshot(&manifest_path, "fix-install-dep").map_err(|e| e.to_string())?;
                let mut manifest =
                    ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
                install_modrinth_with_dependencies(&mut manifest, &[mod_id.clone()], "both");
                save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
                download_project_mods_tracked(&app, &manifest_path, &manifest, None, false);
                Ok(format!("Installed dependency {mod_id}"))
            }
            "updateLoader" => {
                auto_snapshot(&manifest_path, "fix-update-loader").map_err(|e| e.to_string())?;
                let mut manifest =
                    ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
                let (loader_slug, _loaders) = update_loaders_for(&manifest);
                let latest = tuffbox_core::versions::fetch_loader_versions(
                    &loader_slug,
                    &manifest.minecraft.version,
                )
                .map_err(|e| e.to_string())?
                .into_iter()
                .max_by(|a, b| a.id.cmp(&b.id))
                .ok_or_else(|| {
                    format!(
                        "no {loader_slug} build for {}",
                        manifest.minecraft.version
                    )
                })?;
                manifest.loader.version = latest.id.clone();
                save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
                Ok(format!("Updated loader to {}", latest.id))
            }
            "raiseMemory" => {
                auto_snapshot(&manifest_path, "fix-raise-memory").map_err(|e| e.to_string())?;
                let mut manifest =
                    ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
                let target = 6144u32;
                for profile in manifest.profiles.iter_mut() {
                    if profile.memory_mb.map(|m| m < target).unwrap_or(true) {
                        profile.memory_mb = Some(target);
                    }
                }
                save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
                Ok(format!("Set allocated memory to {} MB", target))
            }
            "acceptEula" => {
                let instance_dir =
                    tuffbox_core::instance_dir_for_manifest(&manifest_path)
                        .ok_or_else(|| "could not resolve instance directory".to_string())?;
                let eula_path = instance_dir.join("eula.txt");
                std::fs::write(&eula_path, "# Auto-accepted by TuffBox crash fix\neula=true\n")
                    .map_err(|e| format!("failed to write {}: {e}", eula_path.display()))?;
                Ok("Set eula=true in eula.txt".into())
            }
            "changePort" => {
                let instance_dir =
                    tuffbox_core::instance_dir_for_manifest(&manifest_path)
                        .ok_or_else(|| "could not resolve instance directory".to_string())?;
                let props_path = instance_dir.join("server.properties");
                let content = std::fs::read_to_string(&props_path).unwrap_or_default();
                let mut props = tuffbox_core::properties_parser::PropertiesFile::parse(&content);
                props.set("server-port", "25566");
                std::fs::write(&props_path, props.to_string())
                    .map_err(|e| format!("failed to write {}: {e}", props_path.display()))?;
                Ok("Changed server-port to 25566".into())
            }
            "autoJava" => {
                auto_snapshot(&manifest_path, "fix-auto-java").map_err(|e| e.to_string())?;
                let mut manifest =
                    ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
                let runtimes = tuffbox_core::jre::find_all_runtimes().map_err(|e| e.to_string())?;
                let required = tuffbox_core::jre::required_java_major(&manifest.minecraft.version);
                let best = tuffbox_core::jre::find_runtime_for(&runtimes, required)
                    .ok_or_else(|| "no compatible Java runtime found on this machine".to_string())?;
                let mut java = manifest.java.clone().unwrap_or(tuffbox_core::manifest::JavaSpec {
                    major: None,
                    distribution: None,
                    path: None,
                });
                java.path = Some(best.path.clone());
                java.major = Some(best.major.try_into().unwrap_or(u16::MAX));
                manifest.java = Some(java);
                save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
                Ok(format!("Selected Java {} ({})", best.major, best.path))
            }
            other => Err(format!("unknown fix action kind: {other}")),
        }
    })
    .await
    .map_err(|e| e.to_string())??;

    // Track pending fix so a later healthy relaunch can record *how* it was resolved.
    let launcher = fix_action_to_launcher_action(&action_for_record, &result);
    let _ = swarm_api::record_user_fix_attempt(
        Path::new(&path_for_record),
        "diagnose_hint",
        &result,
        vec![launcher],
        None,
    );

    Ok(result)
}

fn fix_action_to_launcher_action(
    action: &FixAction,
    summary: &str,
) -> tuffbox_core::action_plan::LauncherAction {
    let op = match action.kind.as_str() {
        "disableMod" => "disable_mod",
        "removeMod" => "remove_mod",
        "reinstallMod" => "reinstall_mod",
        "updateMod" => "update_mod",
        "installDependency" => "install_mod",
        "updateLoader" => "update_loader",
        "raiseMemory" => "raise_memory",
        "acceptEula" => "accept_eula",
        "changePort" => "change_port",
        "autoJava" => "auto_java",
        other => other,
    };
    tuffbox_core::action_plan::LauncherAction {
        op: op.into(),
        mod_id: action.mod_id.clone(),
        provider: if op == "install_mod" {
            Some("modrinth".into())
        } else {
            None
        },
        project_id: if op == "install_mod" {
            action.mod_id.clone()
        } else {
            None
        },
        version: None,
        path: None,
        patch_type: None,
        patch: None,
        reason: Some(summary.to_string()),
        risk: "medium".into(),
    }
}

/// Scans the `mods/` folder for `.jar` files that appear to be built for a
/// different mod loader than what the project uses (e.g. a Forge mod in a
/// Fabric project), and returns a list of suggestions with the file name
/// and a recommendation.
#[tauri::command(rename_all = "camelCase")]
async fn detect_wrong_loader_mods(path: String) -> Result<Vec<serde_json::Value>, String> {
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
async fn disable_wrong_loader_jar(path: String, file_name: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let project_dir = PathBuf::from(&path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let src = project_dir.join("mods").join(&file_name);
        let dst = project_dir
            .join("mods")
            .join(format!("{}.disabled", file_name));
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
async fn remove_loose_jar(path: String, file_name: String) -> Result<String, String> {
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
    auto_snapshot_with_changed_files(
        &manifest_path,
        "edit-config",
        &[PathBuf::from(&relative_path)],
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(target, content).map_err(|e| e.to_string())
}

/// Full-text search across all config and script files in the project.
#[tauri::command(rename_all = "camelCase")]
fn search_in_configs(path: String, query: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let roots = ["config", "defaultconfigs", "kubejs", "scripts"];
    let whitelist: &[&str] = &[
        "json",
        "json5",
        "toml",
        "properties",
        "cfg",
        "yaml",
        "yml",
        "js",
        "zs",
        "txt",
        "md",
        "html",
        "css",
        "sh",
    ];
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    fn walk(dir: &Path, cb: &mut dyn FnMut(&Path)) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, cb);
            } else {
                cb(&p);
            }
        }
    }

    for root in &roots {
        let root_dir = project_dir.join(root);
        if !root_dir.is_dir() {
            continue;
        }
        walk(&root_dir, &mut |file_path: &Path| {
            if results.len() >= 200 {
                return;
            }
            let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if !whitelist.contains(&ext) {
                return;
            }
            let Ok(content) = std::fs::read_to_string(file_path) else {
                return;
            };
            if content.len() > 1024 * 1024 {
                return;
            }
            for (line_no, line) in content.lines().enumerate() {
                if line.to_lowercase().contains(&query_lower) {
                    if let Ok(rel) = file_path.strip_prefix(&project_dir) {
                        results.push(serde_json::json!({
                            "path": rel.to_string_lossy(),
                            "line": line_no + 1,
                            "text": line.trim().chars().take(200).collect::<String>(),
                        }));
                    }
                    if results.len() >= 200 {
                        return;
                    }
                }
            }
        });
        if results.len() >= 200 {
            break;
        }
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
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_stats(project_dir: &std::path::Path, stats: &ProjectStats) -> Result<(), String> {
    let p = stats_path(project_dir);
    if let Some(par) = p.parent() {
        std::fs::create_dir_all(par).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        &p,
        serde_json::to_string_pretty(stats).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
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
    let mut all_playtime = 0u64;
    let mut last = None;
    for (_id, inst) in &stats.instances {
        all_launches += inst.launches;
        all_crashes += inst.crashes;
        all_playtime += inst.total_playtime_seconds;
        if inst.last_launch.is_some() {
            last = inst.last_launch.clone();
        }
    }
    Ok(serde_json::json!({
        "totalLaunches": all_launches,
        "totalCrashes": all_crashes,
        "totalPlaytimeSeconds": all_playtime,
        "lastLaunch": last,
        "byProfile": stats.instances.iter().map(|(id, inst)| serde_json::json!({
            "id": id, "launches": inst.launches, "crashes": inst.crashes,
            "playtimeSeconds": inst.total_playtime_seconds,
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
    let manifest = manifest_for_graph(&path)?;
    let graph = DependencyGraph::from_manifest(&manifest);
    let diagnostics = Resolver::analyze_project(&manifest, &graph);
    let project_dir = manifest_parent(&path)?;

    // Check JSON files for parse errors
    let mut json_errors: Vec<serde_json::Value> = Vec::new();
    let roots = ["config", "defaultconfigs", "kubejs", "scripts"];
    for root in &roots {
        let dir = project_dir.join(root);
        if !dir.is_dir() {
            continue;
        }
        fn walk_json(dir: &Path, acc: &mut Vec<serde_json::Value>) {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => return,
            };
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk_json(&p, acc);
                    continue;
                }
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
                if acc.len() >= 50 {
                    return;
                }
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
                            let key = if m.id < *target {
                                (m.id.clone(), target.clone())
                            } else {
                                (target.clone(), m.id.clone())
                            };
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

    let error_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == tuffbox_core::DiagnosticSeverity::Error)
        .collect();
    let warning_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == tuffbox_core::DiagnosticSeverity::Warning)
        .collect();
    let mods_without_source = manifest
        .mods
        .iter()
        .filter(|m| m.source.url.is_none())
        .count();
    let mods_without_hash = manifest
        .mods
        .iter()
        .filter(|m| m.hashes.as_ref().and_then(|h| h.sha1.as_ref()).is_none())
        .count();

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

/// Resolves the installed sha1 from disk, falling back to manifest metadata
/// only when the file is unavailable. The jar is the source of truth: an
/// interrupted older update may have already changed manifest metadata.
fn resolve_mod_sha1(manifest_path: &Path, module: &ModSpec) -> Option<String> {
    if let Some(path) = existing_mod_file_path(manifest_path, module) {
        if let Ok(hash) = tuffbox_core::sha1_file(&path) {
            return Some(hash);
        }
    }
    module
        .hashes
        .as_ref()
        .and_then(|h| h.sha1.as_ref())
        .filter(|h| !h.is_empty())
        .cloned()
}

fn installed_matches_version(
    module: &ModSpec,
    installed_sha1: Option<&str>,
    latest: &tuffbox_core::VersionInfo,
) -> bool {
    // Modrinth version id is authoritative when the install already points
    // at it — hash metadata can be stale after interrupted updates.
    if module.source.file_id.as_deref() == Some(latest.id.as_str()) {
        return true;
    }
    if let Some(installed_sha1) = installed_sha1 {
        if latest.files.iter().any(|f| {
            f.hashes
                .sha1
                .as_deref()
                .is_some_and(|h| h.eq_ignore_ascii_case(installed_sha1))
        }) {
            return true;
        }
    }
    // Same published version string with no conflicting identity — skip the
    // false "update available" badge users see when hashes disagree in case
    // or the jar filename drifted while the install is already current.
    let installed_ver = module.version.trim();
    !installed_ver.is_empty()
        && installed_ver != "unknown"
        && installed_ver == latest.version_number.trim()
}

fn update_loaders_for(manifest: &ProjectManifest) -> (String, Vec<String>) {
    let loader_slug = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string();
    // Quilt can run many Fabric builds; try both like Prism expands loaders.
    let loaders = if loader_slug == "quilt" {
        vec![loader_slug.clone(), "fabric".to_string()]
    } else {
        vec![loader_slug.clone()]
    };
    (loader_slug, loaders)
}

/// Collects pending updates for the project's current Minecraft + loader.
/// Uses Modrinth `version_files/update` for hashed jars, then falls back to
/// `project/{id}/version` for Modrinth mods that still lack a usable hash.
fn resolve_pending_mod_updates(
    manifest_path: &Path,
    manifest: &ProjectManifest,
    provider: &tuffbox_core::ModrinthProvider,
) -> Result<(String, Vec<(usize, tuffbox_core::VersionInfo)>), String> {
    let (loader_slug, loaders) = update_loaders_for(manifest);
    let game_versions = vec![manifest.minecraft.version.clone()];

    let mut hash_to_mod: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut hashes: Vec<String> = Vec::new();
    let mut no_hash_idxs: Vec<usize> = Vec::new();

    for (idx, module) in manifest.mods.iter().enumerate() {
        let is_modrinth = module.source.kind == SourceKind::Modrinth
            || (module.source.kind != SourceKind::Curseforge
                && module.source.kind != SourceKind::Github
                && module.source.project_id.is_some());
        if !is_modrinth {
            continue;
        }
        match resolve_mod_sha1(manifest_path, module) {
            Some(hash) => {
                hash_to_mod.insert(hash.clone(), idx);
                hashes.push(hash);
            }
            None if module.source.project_id.is_some() => no_hash_idxs.push(idx),
            None => {}
        }
    }

    let mut pending: Vec<(usize, tuffbox_core::VersionInfo)> = Vec::new();
    let mut resolved_idxs = std::collections::HashSet::new();

    if !hashes.is_empty() {
        let latest_map = provider
            .get_latest_versions(&hashes, &loaders, &game_versions)
            .map_err(|e| e.to_string())?;
        for (hash, latest) in latest_map {
            let Some(&idx) = hash_to_mod.get(&hash) else {
                continue;
            };
            if installed_matches_version(&manifest.mods[idx], Some(&hash), &latest) {
                continue;
            }
            // Prefer versions that actually ship a file for our loader.
            if ProviderFileInfo::select_file_for_loader(&latest, &loader_slug).is_none() {
                continue;
            }
            resolved_idxs.insert(idx);
            pending.push((idx, latest));
        }
    }

    for idx in no_hash_idxs {
        if resolved_idxs.contains(&idx) {
            continue;
        }
        let module = &manifest.mods[idx];
        let Some(project_id) = module.source.project_id.as_ref() else {
            continue;
        };
        let query = ProviderSearchQuery {
            query: None,
            minecraft_version: Some(manifest.minecraft.version.clone()),
            loader: Some(loader_slug.clone()),
            ..Default::default()
        };
        let Ok(versions) = provider.get_versions(project_id, &query) else {
            continue;
        };
        let Some(latest) = versions.into_iter().next() else {
            continue;
        };
        if installed_matches_version(module, None, &latest) {
            continue;
        }
        if ProviderFileInfo::select_file_for_loader(&latest, &loader_slug).is_none() {
            continue;
        }
        pending.push((idx, latest));
    }

    Ok((loader_slug, pending))
}

/// Checks every Modrinth-sourced mod in the project for available updates,
/// comparing the installed version against the latest compatible version.
/// Uses Modrinth's batch update API plus disk-hash / project-id fallbacks.
/// Returns a list with update info for each mod that could be updated.
#[tauri::command(rename_all = "camelCase")]
async fn check_mod_updates(path: String) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let provider = tuffbox_core::ModrinthProvider::new();
        let (loader_slug, pending) =
            resolve_pending_mod_updates(&manifest_path, &manifest, &provider)?;

        let mut updates = Vec::new();
        for (idx, latest) in pending {
            let m = &manifest.mods[idx];
            let file = ProviderFileInfo::select_file_for_loader(&latest, &loader_slug).cloned();
            updates.push(serde_json::json!({
                "modId": m.id,
                "name": m.name,
                "currentVersion": m.version,
                "latestVersion": latest.version_number,
                "versionId": latest.id,
                "fileName": file.as_ref().map(|f| &f.filename),
                "gameVersions": latest.game_versions,
                "loaders": latest.loaders,
                "changelog": latest.changelog,
                "datePublished": latest.date_published,
                "versionType": latest.version_type,
                "iconUrl": m.source.icon_url,
            }));
        }
        Ok(updates)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Applies all available mod updates at once (batch update), creating
/// a single auto-snapshot before the changes. Uses Modrinth's batch
/// update API to resolve all updates in one request.
#[tauri::command(rename_all = "camelCase")]
async fn update_all_mods(app: tauri::AppHandle, path: String) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        use tauri::Emitter;

        let _guard = MODS_IO_LOCK
            .lock()
            .map_err(|_| "mods I/O lock poisoned".to_string())?;

        emit_mod_update_progress(
            &app,
            "preparing",
            "Creating a safety snapshot…",
            0,
            0,
            5,
            None,
        );
        let manifest_path = PathBuf::from(&path);
        auto_snapshot(&manifest_path, "batch-update-all").map_err(|e| e.to_string())?;
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let mut updated = Vec::new();
        let mut skipped_errors: Vec<String> = Vec::new();

        emit_mod_update_progress(
            &app,
            "checking",
            "Checking Modrinth for compatible updates…",
            0,
            manifest.mods.len(),
            12,
            None,
        );
        let provider = tuffbox_core::ModrinthProvider::new();
        let (loader_slug, pending) =
            resolve_pending_mod_updates(&manifest_path, &manifest, &provider)?;

        let scope_mod_ids: Vec<String> = pending
            .iter()
            .map(|(idx, _)| manifest.mods[*idx].id.clone())
            .collect();
        if !pending.is_empty() {
            let queue: Vec<ModDownloadProgressPayload> = pending
                .iter()
                .map(|(idx, _)| {
                    let module = &manifest.mods[*idx];
                    ModDownloadProgressPayload {
                        id: module.id.clone(),
                        name: module.name.clone(),
                        downloaded: 0,
                        total: 0,
                        percent: 0,
                        status: "queued".to_string(),
                    }
                })
                .collect();
            let _ = app.emit(
                "mod-download-batch",
                serde_json::json!({
                    "phase": "start",
                    "items": queue,
                    "scopeModIds": scope_mod_ids,
                }),
            );
        }

        let mut download = tuffbox_core::ModSyncReport::default();
        let pending_count = pending.len();
        for (position, (idx, latest)) in pending.into_iter().enumerate() {
            let current_mod_id = manifest.mods[idx].id.clone();
            let current_mod_name = manifest.mods[idx].name.clone();
            let percent =
                20 + ((position as f64 / pending_count.max(1) as f64) * 70.0).round() as u32;
            emit_mod_update_progress(
                &app,
                "updating",
                &format!(
                    "Updating {} ({}/{})…",
                    current_mod_name,
                    position + 1,
                    pending_count
                ),
                position,
                pending_count,
                percent,
                Some(&current_mod_id),
            );
            let file = ProviderFileInfo::select_file_for_loader(&latest, &loader_slug).cloned();
            let Some(file) = file else {
                skipped_errors.push(format!(
                    "{}: no compatible file for loader {loader_slug}",
                    manifest.mods[idx].name
                ));
                emit_mod_download_status(&app, &current_mod_id, &current_mod_name, "failed", 0);
                continue;
            };

            let old_mod = manifest.mods[idx].clone();
            let project_id = latest.project_id.clone();
            let project = match provider.get_project(&project_id) {
                Ok(p) => p,
                Err(e) => {
                    skipped_errors.push(format!(
                        "{}: project lookup failed ({e}), using cached metadata",
                        old_mod.name
                    ));
                    project_info_from_mod(&old_mod)
                }
            };
            let previous_deps = old_mod.dependencies.clone();
            let dependencies = provider
                .resolve_dependencies(&latest.id)
                .unwrap_or(previous_deps);
            let mut new_spec = build_mod_spec(
                &project,
                &latest,
                file,
                dependencies,
                infer_project_side(Some(&project)),
            );
            // Keep references and frontend progress scopes valid across an
            // update even when the provider has renamed the project slug.
            new_spec.id = old_mod.id.clone();
            let name = new_spec.name.clone();
            manifest.mods[idx] = new_spec;
            match commit_single_mod_update(&app, &manifest_path, &mut manifest, &old_mod, false) {
                Ok(report) => {
                    download.downloaded.extend(report.downloaded);
                    download.already_present.extend(report.already_present);
                    download.skipped.extend(report.skipped);
                    download.failed.extend(report.failed);
                    updated.push(name);
                }
                Err(error) => {
                    manifest.mods[idx] = old_mod;
                    skipped_errors.push(format!("{name}: {error}"));
                    emit_mod_download_status(&app, &current_mod_id, &current_mod_name, "failed", 0);
                }
            }
        }

        emit_mod_update_progress(
            &app,
            "finalizing",
            "Finalizing the mod list…",
            pending_count,
            pending_count,
            95,
            None,
        );
        if !scope_mod_ids.is_empty() {
            let _ = app.emit(
                "mod-download-batch",
                serde_json::json!({
                    "phase": "done",
                    "downloaded": download.downloaded,
                    "failed": download.failed,
                    "alreadyPresent": download.already_present,
                    "skipped": download.skipped,
                    "scopeModIds": scope_mod_ids,
                    "batchComplete": true,
                }),
            );
        }
        emit_mod_update_progress(
            &app,
            "done",
            "Mod updates complete.",
            pending_count,
            pending_count,
            100,
            None,
        );

        Ok(serde_json::json!({
            "updated": updated,
            "errors": skipped_errors,
            "download": download,
        }))
    })
    .await
    .map_err(|e| e.to_string())?
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
            if !fp.is_file() {
                continue;
            }
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

    // Check if performance mods are missing (treat forks/ports as covering the base mod).
    let keys = installed_mod_keys(&manifest);
    let perf_mods = [
        "sodium",
        "embeddium",
        "lithium",
        "ferrite-core",
        "immediatelyfast",
        "modernfix",
        "memoryleakfix",
        "smoothboot",
        "entityculling",
    ];
    let mut missing_perf = Vec::new();
    for pm in perf_mods {
        let aliases = recommendation_aliases(pm);
        let aliases: Vec<&str> = if aliases.is_empty() {
            vec![pm]
        } else {
            aliases
        };
        if !has_installed(&keys, &aliases) {
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
    let profile = manifest
        .profiles
        .iter()
        .find(|p| p.id == "client")
        .or_else(|| manifest.profiles.first());
    if let Some(profile) = profile {
        let jvm = profile.jvm_args.join(" ");
        if !jvm.contains("-XX:+UseG1GC")
            && !jvm.contains("-XX:+UseZGC")
            && !jvm.contains("-XX:+UseShenandoahGC")
        {
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
const SODIUM_CHECKS: &[(&str, fn(&str, &mut Vec<serde_json::Value>))] = &[(
    "sodium-options.json",
    |c: &str, f: &mut Vec<serde_json::Value>| {
        // Check if vsync is enabled (can cap FPS)
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(c) {
            if v.get("quality")
                .and_then(|q| q.get("use_block_face_culling"))
                .and_then(|x| x.as_str())
                == Some("1")
            {
                return;
            }
            if v.get("advanced")
                .and_then(|a| a.get("use_chunk_multidraw"))
                .and_then(|x| x.as_bool())
                == Some(false)
            {
                f.push(serde_json::json!({"severity":"info","code":"CHUNK_MULTIDRAW_OFF","message":"Chunk multidraw is disabled in Sodium; enable it for better FPS.","file":"config/sodium-options.json"}));
            }
            let render_dist = v
                .get("quality")
                .and_then(|q| q.get("render_distance"))
                .and_then(|x| x.as_str())
                .unwrap_or("16");
            if render_dist.parse::<u32>().unwrap_or(16) > 16 {
                f.push(serde_json::json!({"severity":"warning","code":"HIGH_RENDER_DISTANCE","message":format!("Render distance is {render_dist} — consider lowering to 12-16 for modded."),"file":"config/sodium-options.json"}));
            }
        }
    },
)];

/// Forge/NeoForge config checks: (filename_pattern, fn(&content, &filename, &mut findings))
const FORGE_PERF_CHECKS: &[(&str, fn(&str, &str, &mut Vec<serde_json::Value>))] = &[(
    "forge-server",
    |c: &str, name: &str, f: &mut Vec<serde_json::Value>| {
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
        for search in &[
            "max-entity-collisions",
            "spawn-limits",
            "max-breed",
            "despawn-ranges",
        ] {
            if c.contains(search) {
                f.push(serde_json::json!({"severity":"info","code":"SERVER_PERF_CONFIG_PRESENT","message":format!("Server performance config detected: {search}. Review limits for your player count."),"file":format!("config/{name}")}));
            }
        }
    },
)];

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
        if !dir.is_dir() {
            continue;
        }
        fn walk(dir: &std::path::Path, acc: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, acc);
                    continue;
                }
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
    let heuristic_hits =
        tuffbox_core::knowledge::heuristics::scan_configs_for_ore_gen(&config_contents);

    // Cross-reference with builtin knowledge base
    let mut results = Vec::new();
    for hit in &heuristic_hits {
        // Check if knowledge base has this mod
        let kb_hint =
            tuffbox_core::knowledge::builtin::ModKnowledgeEntry::lookup(&hit.resource_name);
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
        if manifest.mods.iter().any(|m| m.id == entry.slug) && !entry.programmatic_items.is_empty()
        {
            mod_items.push((entry.slug.clone(), entry.programmatic_items.clone()));
        }
    }

    // Also try to read known item registry from mod jars if available
    if mods_dir.is_dir() {
        for entry in std::fs::read_dir(&mods_dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.extension().map_or(true, |e| e != "jar") {
                continue;
            }
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
        let config_path = project_dir
            .join("config")
            .join("almostunified")
            .join("unify.json");
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
        for entry in std::fs::read_dir(&crash_dir)
            .into_iter()
            .flatten()
            .flatten()
        {
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
    let java_path = manifest
        .java
        .as_ref()
        .and_then(|j| j.path.clone())
        .unwrap_or_default();
    let java_version = if !java_path.is_empty() {
        tuffbox_core::jre::check_java_at_path(&std::path::PathBuf::from(&java_path))
            .map(|r| r.version)
            .unwrap_or_default()
    } else {
        tuffbox_core::jre::find_all_runtimes()
            .ok()
            .and_then(|r| r.into_iter().next())
            .map(|r| r.version)
            .unwrap_or_default()
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
    Ok(results
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "className": r.class_name,
                "modId": r.mod_id,
                "modName": r.mod_name,
            })
        })
        .collect())
}

/// Searches all mod JARs to find which mods depend on a given class
/// (Jdeps analysis tool from Crash Assistant).
#[tauri::command(rename_all = "camelCase")]
fn find_dependents_on_class(
    path: String,
    class_name: String,
) -> Result<Vec<serde_json::Value>, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mods_dir = project_dir.join("mods");
    let installed: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    let results = tuffbox_core::crash_assistant::find_mods_depending_on_class(
        &class_name,
        &mods_dir,
        &installed,
    );
    Ok(results
        .into_iter()
        .map(|r| {
            serde_json::json!({
                "className": r.class_name,
                "modId": r.mod_id,
                "modName": r.mod_name,
            })
        })
        .collect())
}

/// Runs the full Crash Assistant analysis and also includes MCreator
/// mod list, class finder results from crash logs, and Jdeps results.
///
/// When `report_id` is set, only that crash report is analyzed (plus
/// `logs/latest.log` and the current installed mod list). Otherwise the
/// newest crash report is used — never the entire crash-reports folder.
#[tauri::command(rename_all = "camelCase")]
fn run_crash_assistant_full(
    path: String,
    report_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let mods_dir = project_dir.join("mods");
    let report = run_crash_assistant_analysis(&path, &manifest, &project_dir, report_id.as_deref())?;

    let mut class_finder = Vec::new();
    let mut combined = String::new();
    if let Some(text) = load_scoped_crash_report(&project_dir, report_id.as_deref()) {
        combined.push_str(&text);
        combined.push('\n');
    }
    let latest = project_dir.join("logs").join("latest.log");
    if latest.is_file() {
        combined.push_str(
            &tuffbox_core::process::read_log_tail(&latest, 2000).unwrap_or_default(),
        );
    }
    for line in combined.lines() {
        if line.contains("NoClassDefFoundError") || line.contains("ClassNotFoundException") {
            if let Some(cls) = line
                .split(": ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
            {
                if cls.len() > 5 && cls.len() < 200 && cls.contains('.') {
                    let matches = tuffbox_core::crash_assistant::find_class_in_mods(cls, &mods_dir);
                    for m in matches {
                        class_finder.push(serde_json::json!({"className":m.class_name,"modId":m.mod_id,"modName":m.mod_name}));
                    }
                }
            }
        }
    }
    class_finder.truncate(20);

    Ok(serde_json::json!({
        "findings": report.findings.iter().map(|f| serde_json::json!({
            "severity": f.severity,
            "code": f.code,
            "title": f.title,
            "description": f.description,
            "autoFix": f.auto_fix,
            "references": f.references,
            "evidence": f.evidence,
            "fixes": f.fixes.iter().map(|a| serde_json::json!({
                "kind": a.kind,
                "label": a.label,
                "modId": a.mod_id,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "supportMessageDiscord": report.support_message_discord,
        "supportMessageGithub": report.support_message_github,
        "modsAdded": report.mods_added,"modsRemoved": report.mods_removed,
        "suspectedMods": report.suspected_mods,
        "mcreatorMods": report.mcreator_mods,
        "classFinderResults": class_finder,
        "findingsCount": report.findings.len(),
        "scope": {
            "reportId": report_id,
            "latestLog": true,
            "installedMods": true,
        },
    }))
}

/// Load a single crash report by id (filename stem / path fragment), or the
/// newest `.txt` under `crash-reports/` when `report_id` is None.
fn load_scoped_crash_report(project_dir: &Path, report_id: Option<&str>) -> Option<String> {
    load_scoped_crash_report_with_path(project_dir, report_id).map(|(_, text)| text)
}

/// True when the caller selected a real crash-report file (not latest.log).
fn is_explicit_crash_report_id(report_id: Option<&str>) -> bool {
    matches!(report_id, Some(id) if !id.is_empty() && id != "__latest_log__")
}

/// Load a crash-report only when `report_id` is an explicit file id/name.
/// Does **not** fall back to the newest crash — that broke "AI explain on
/// latest.log" by always injecting the previous crash text into the prompt.
fn load_scoped_crash_report_with_path(
    project_dir: &Path,
    report_id: Option<&str>,
) -> Option<(PathBuf, String)> {
    let id = report_id.filter(|s| !s.is_empty() && *s != "__latest_log__")?;
    let cd = project_dir.join("crash-reports");
    if !cd.is_dir() {
        return None;
    }
    let files: Vec<std::fs::DirEntry> = std::fs::read_dir(&cd)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "txt"))
        .collect();
    if files.is_empty() {
        return None;
    }
    let entry = files.iter().find(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        name == id
            || name.trim_end_matches(".txt") == id
            || e.path().to_string_lossy().contains(id)
    })?;
    let path = entry.path();
    let text = std::fs::read_to_string(&path)
        .ok()
        .filter(|c| c.len() < 4 * 1024 * 1024)?;
    Some((path, text))
}

/// Newest `crash-reports/*.txt` by mtime (for flows that intentionally want
/// "last crash" rather than a user-selected report or latest.log).
fn load_newest_crash_report(project_dir: &Path) -> Option<(PathBuf, String)> {
    let cd = project_dir.join("crash-reports");
    if !cd.is_dir() {
        return None;
    }
    let mut files: Vec<std::fs::DirEntry> = std::fs::read_dir(&cd)
        .ok()?
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "txt"))
        .collect();
    if files.is_empty() {
        return None;
    }
    files.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));
    let path = files[0].path();
    let text = std::fs::read_to_string(&path)
        .ok()
        .filter(|c| c.len() < 4 * 1024 * 1024)?;
    Some((path, text))
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
            if p.extension().map_or(true, |e| e != "jar") {
                continue;
            }
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
fn compare_modpacks(path_a: String, path_b: String) -> Result<serde_json::Value, String> {
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
        let va = ma
            .mods
            .iter()
            .find(|m| m.id == **id)
            .map(|m| m.version.clone());
        let vb = mb
            .mods
            .iter()
            .find(|m| m.id == **id)
            .map(|m| m.version.clone());
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
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(BackupIndex {
            backups: vec![],
            max_count: 20,
        })
}

fn save_backup_index(project_dir: &Path, idx: &BackupIndex) -> Result<(), String> {
    let d = backup_dir(project_dir);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::fs::write(
        d.join("index.json"),
        serde_json::to_string_pretty(idx).map_err(|e| e.to_string())?,
    )
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
    let id = format!(
        "{}-{}",
        backup_name.replace(' ', "-"),
        tuffbox_core::time_util::compact_now()
    );
    let zip_path = dir.join(format!("{}.zip", id));

    let output = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(output);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let mut total_size: u64 = 0;
    for folder in &[
        "mods",
        "config",
        "defaultconfigs",
        "kubejs",
        "scripts",
        "resourcepacks",
        "shaderpacks",
        "datapacks",
    ] {
        let d = project_dir.join(folder);
        if d.is_dir() {
            fn add_dir(
                zip: &mut zip::ZipWriter<std::fs::File>,
                opts: zip::write::SimpleFileOptions,
                base: &Path,
                dir: &Path,
                size: &mut u64,
            ) -> Result<(), String> {
                for e in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
                    let e = e.map_err(|e| e.to_string())?;
                    let p = e.path();
                    if p.is_dir() {
                        add_dir(zip, opts, base, &p, size)?;
                    } else if p.is_file() {
                        if let Ok(meta) = p.metadata() {
                            *size += meta.len();
                        }
                        let rel = p
                            .strip_prefix(base)
                            .unwrap_or(&p)
                            .to_string_lossy()
                            .replace('\\', "/");
                        zip.start_file(rel, opts).map_err(|e| e.to_string())?;
                        zip.write_all(&std::fs::read(&p).map_err(|e| e.to_string())?)
                            .map_err(|e| e.to_string())?;
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
        zip.start_file("project.tuffbox.json", opts)
            .map_err(|e| e.to_string())?;
        zip.write_all(&std::fs::read(&mainfest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }

    zip.finish().map_err(|e| e.to_string())?;

    // Update index
    let mut idx = load_backup_index(&project_dir);
    idx.backups.push(BackupEntry {
        id: id.clone(),
        name: backup_name.clone(),
        created_at: tuffbox_core::time_util::rfc3339_now(),
        size_bytes: total_size,
        manifest_snapshot: true,
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
    Ok(idx
        .backups
        .into_iter()
        .rev()
        .map(|b| {
            serde_json::json!({
                "id": b.id, "name": b.name, "createdAt": b.created_at,
                "sizeBytes": b.size_bytes, "manifestSnapshot": b.manifest_snapshot,
            })
        })
        .collect())
}

/// Deletes a specific backup.
#[tauri::command(rename_all = "camelCase")]
fn delete_backup(path: String, backup_id: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let dir = backup_dir(&project_dir);
    let zip_path = dir.join(format!("{}.zip", backup_id));
    if zip_path.is_file() {
        std::fs::remove_file(&zip_path).map_err(|e| e.to_string())?;
    }
    let mut idx = load_backup_index(&project_dir);
    idx.backups.retain(|b| b.id != backup_id);
    save_backup_index(&project_dir, &idx)
}

/// ── AI Crash Explanation context builder ─────────────────────────

/// Builds a structured AI context from crash data (but does NOT call any
/// LLM — the frontend can send this to any AI provider).
#[tauri::command(rename_all = "camelCase")]
fn prepare_ai_crash_context(
    path: &str,
    report_id: Option<&str>,
) -> Result<(tuffbox_core::ai_explanation::CrashAiContext, usize), String> {
    let manifest = ProjectManifest::load_from_path(path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(path)?;

    let crash_content =
        load_scoped_crash_report(&project_dir, report_id).unwrap_or_default();
    let latest = project_dir.join("logs").join("latest.log");
    // When explaining latest.log (no crash report selected), pull a larger tail
    // so the model sees the live session instead of an empty crash excerpt.
    let using_crash_file = is_explicit_crash_report_id(report_id);
    let latest_line_budget = if using_crash_file { 900 } else { 2500 };
    let latest_log = if latest.is_file() {
        tuffbox_core::process::read_log_tail(&latest, latest_line_budget).unwrap_or_default()
    } else {
        String::new()
    };
    let crash_excerpt_budget = if using_crash_file { 6000 } else { 800 };
    let latest_excerpt_budget = if using_crash_file { 4000 } else { 7000 };

    let jv = manifest
        .java
        .as_ref()
        .and_then(|j| j.path.clone())
        .unwrap_or_default();
    let java_version = if !jv.is_empty() {
        tuffbox_core::jre::check_java_at_path(&PathBuf::from(&jv))
            .map(|r| r.version)
            .unwrap_or_default()
    } else {
        "unknown".into()
    };

    let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
    let ctx = tuffbox_core::crash_assistant::AnalysisCtx {
        crash_content: vec![crash_content.clone()],
        latest_log: latest_log.clone(),
        launcher_log: String::new(),
        installed_mods: manifest.mods.iter().map(|m| m.id.clone()).collect(),
        previous_mods: Vec::new(),
        java_version: java_version.clone(),
        java_vendor: String::new(),
        os_name: std::env::consts::OS.to_string(),
        mc_version: manifest.minecraft.version.clone(),
        loader: loader.clone(),
        loader_version: manifest.loader.version.clone(),
        cpu_name: String::new(),
        gpu_names: Vec::new(),
        total_ram_mb: 0,
        is_offline: false,
        win_events: Vec::new(),
    };
    let diagnosis = tuffbox_core::crash::build_crash_diagnosis(
        &project_dir,
        &manifest,
        report_id,
        Vec::new(),
    )
    .map_err(|e| e.to_string())?;

    let report = tuffbox_core::crash_assistant::run_full_analysis(&ctx);

    let blame_ids: Vec<String> = diagnosis
        .suspected_mods
        .iter()
        .filter(|s| {
            matches!(
                s.blame_role,
                tuffbox_core::crash::BlameRole::Primary | tuffbox_core::crash::BlameRole::Secondary
            ) || s.confidence >= 80
        })
        .take(3)
        .map(|s| s.id.clone())
        .collect();

    let haystack = format!("{crash_content}\n{latest_log}");
    let fingerprint = tuffbox_core::crash_kb::fingerprint_from_text_with_blame(
        &haystack,
        &manifest.minecraft.version,
        &loader,
        &blame_ids,
    );
    let kb_cases = tuffbox_core::crash_kb::load_all_cases(&project_dir);
    let similar = tuffbox_core::crash_kb::search_similar(&kb_cases, &fingerprint, &haystack, 5);

    let inventory =
        tuffbox_core::project_ai_inventory::collect_project_ai_inventory(&project_dir, &manifest);

    let culprit_details: Vec<tuffbox_core::ai_explanation::CrashAiCulprit> = diagnosis
        .suspected_mods
        .iter()
        .take(8)
        .map(|s| tuffbox_core::ai_explanation::CrashAiCulprit {
            id: s.id.clone(),
            name: s.name.clone(),
            confidence: s.confidence,
            authors: s.authors.clone(),
            blame_role: match s.blame_role {
                tuffbox_core::crash::BlameRole::Primary => "primary".into(),
                tuffbox_core::crash::BlameRole::Secondary => "secondary".into(),
                tuffbox_core::crash::BlameRole::Related => "related".into(),
            },
            match_sources: s.match_sources.clone(),
            evidence: s
                .evidence
                .iter()
                .take(3)
                .map(|e| e.text.clone())
                .collect(),
        })
        .collect();

    let mut installed_sample: Vec<String> = culprit_details.iter().map(|c| c.id.clone()).collect();
    if installed_sample.is_empty() {
        installed_sample = report.suspected_mods.clone();
    }
    for id in inventory.mods.iter().map(|m| m.id.clone()) {
        if installed_sample.len() >= 24 {
            break;
        }
        if !installed_sample.iter().any(|s| s.eq_ignore_ascii_case(&id)) {
            installed_sample.push(id);
        }
    }

    // Pull recent crash-related history into the prompt so the model sees what
    // the user already tried (and what was marked resolved).
    let recent_changes = recent_crash_history_lines(&project_dir, 12);

    let graph_diagnostics: Vec<String> = diagnosis
        .graph_diagnostics
        .iter()
        .take(12)
        .map(|d| format!("[{:?}] {}: {}", d.severity, d.code, d.message))
        .collect();

    let ai_ctx = tuffbox_core::ai_explanation::CrashAiContext {
        mc_version: manifest.minecraft.version.clone(),
        loader: loader.clone(),
        loader_version: manifest.loader.version.clone(),
        java_version,
        os: std::env::consts::OS.to_string(),
        installed_mods: installed_sample,
        installed_mod_count: inventory.mods.len() as u32,
        crash_report_excerpt: tuffbox_core::crash_kb::smart_excerpt(
            &crash_content,
            crash_excerpt_budget,
        ),
        latest_log_excerpt: tuffbox_core::crash_kb::smart_excerpt(
            &latest_log,
            latest_excerpt_budget,
        ),
        suspected_mods: culprit_details.iter().map(|c| c.id.clone()).collect(),
        culprit_details,
        crash_assistant_findings: tuffbox_core::ai_explanation::findings_to_ai(&report.findings),
        recent_changes,
        graph_diagnostics,
        similar_cases: similar,
        fingerprint_key: fingerprint.key.clone(),
        report_id: report_id.map(|s| s.to_string()),
        inventory: Some(inventory),
    };

    Ok((ai_ctx, report.findings.len()))
}

fn recent_crash_history_lines(project_dir: &Path, limit: usize) -> Vec<String> {
    let mut lines = Vec::new();
    // Resolved crash fixes (successful relaunch after apply).
    if let Ok(entries) = swarm_api::list_crash_resolutions(project_dir) {
        for r in entries.into_iter().take(limit) {
            lines.push(format!(
                "RESOLVED [{}] via {}: {}",
                r.fingerprint_key,
                r.verified_by,
                tuffbox_core::crash_kb::truncate_at_char_boundary(&r.human_explanation, 160)
            ));
        }
    }
    // Recent crash_fix snapshots.
    let store = SnapshotStore::new(project_dir);
    if let Ok(snaps) = store.list() {
        for snap in snaps
            .into_iter()
            .filter(|s| s.tags.iter().any(|t| t == "crash_fix" || t == "crash_resolved"))
            .take(limit)
        {
            lines.push(format!(
                "{} [{}] {}",
                if snap.tags.iter().any(|t| t == "crash_resolved") {
                    "RESOLVED_SNAP"
                } else {
                    "FIX_APPLIED"
                },
                snap.crash_fingerprint_key.as_deref().unwrap_or("-"),
                snap.reason
            ));
        }
    }
    lines.truncate(limit);
    lines
}

#[tauri::command(rename_all = "camelCase")]
fn build_ai_crash_context(
    path: String,
    report_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let (ai_ctx, findings_count) =
        prepare_ai_crash_context(&path, report_id.as_deref())?;
    let prompt = tuffbox_core::ai_explanation::build_crash_prompt(&ai_ctx);
    let triage = tuffbox_core::ai_explanation::build_triage_prompt(&ai_ctx);
    let settings = integrations::get_integration_status().settings;

    // Do not ship the full inventory blob to the webview — counts + prompt are enough.
    // Keeps IPC small and avoids UI freezes on big packs.
    let mod_count = ai_ctx.installed_mod_count;
    let config_count = ai_ctx
        .inventory
        .as_ref()
        .map(|i| i.config_files.len())
        .unwrap_or(0);
    let pack_count = ai_ctx
        .inventory
        .as_ref()
        .map(|i| i.resourcepacks.len() + i.datapacks.len() + i.shaderpacks.len())
        .unwrap_or(0);
    let mut ui_ctx = ai_ctx.clone();
    ui_ctx.inventory = None;

    Ok(serde_json::json!({
        "context": ui_ctx,
        "prompt": prompt,
        "triagePrompt": triage,
        "promptLength": prompt.len(),
        "findingsCount": findings_count,
        "similarCaseCount": ai_ctx.similar_cases.len(),
        "fingerprintKey": ai_ctx.fingerprint_key,
        "aiProvider": settings.ai.provider,
        "aiModel": settings.ai.model,
        "aiEndpoint": settings.ai.endpoint,
        "diagnoseMode": settings.ai.diagnose_mode,
        "crashKbEndpoint": settings.ai.crash_kb_endpoint,
        "inventorySummary": {
            "mods": mod_count,
            "configs": config_count,
            "packs": pack_count,
        },
    }))
}

#[tauri::command(rename_all = "camelCase")]
async fn analyze_crash_with_ai(
    path: String,
    report_id: Option<String>,
) -> Result<serde_json::Value, String> {
    // Build structured context directly — avoid JSON round-trip panics / lossy deserialize.
    let (mut ai_ctx, _findings_count) =
        prepare_ai_crash_context(&path, report_id.as_deref())?;
    let settings = integrations::get_integration_status().settings;
    let mode = tuffbox_core::action_plan::DiagnoseMode::parse(&settings.ai.diagnose_mode);
    let similar_count = ai_ctx.similar_cases.len() as u64;

    let project_dir = manifest_parent(&path)?;
    let crash_content =
        load_scoped_crash_report(&project_dir, report_id.as_deref()).unwrap_or_default();
    let latest = project_dir.join("logs").join("latest.log");
    let latest_log = if latest.is_file() {
        tuffbox_core::process::read_log_tail(&latest, 900).unwrap_or_default()
    } else {
        String::new()
    };
    let haystack = format!("{crash_content}\n{latest_log}");
    let fingerprint = tuffbox_core::crash_kb::fingerprint_from_text(
        &haystack,
        &ai_ctx.mc_version,
        &ai_ctx.loader,
    );

    let swarm_on = integrations::swarm_enabled();
    let transport_bases = if swarm_on {
        swarm_node::capsule_transport_bases().await
    } else {
        Vec::new()
    };
    let online_kb = !transport_bases.is_empty();
    let mut network_used = false;
    let mut compact_prompt_used = false;
    let mut kb_short_circuit = false;

    if swarm_on {
        let global_hits =
            integrations::global_capsule_library().lookup(&fingerprint, &haystack, 5);
        if !global_hits.is_empty() {
            let mut merged = tuffbox_core::crash_remote::hits_to_similar_cases(&global_hits);
            merged.extend(ai_ctx.similar_cases.drain(..));
            let mut seen = std::collections::HashSet::new();
            merged.retain(|h| seen.insert(h.id.clone()));
            ai_ctx.similar_cases = merged;
        }
    }

    let inventory_ids: Vec<String> = ai_ctx
        .inventory
        .as_ref()
        .map(|inv| inv.mods.iter().map(|m| m.id.clone()).collect())
        .unwrap_or_default();
    let missing_ids =
        tuffbox_core::ai_explanation::missing_dep_hints_from_graph(&ai_ctx.graph_diagnostics);

    let mut plan = match mode {
        tuffbox_core::action_plan::DiagnoseMode::Server if online_kb => {
            network_used = true;
            let req = tuffbox_core::crash_remote::CrashDiagnoseRequest {
                fingerprint: fingerprint.clone(),
                context: Some(serde_json::to_value(&ai_ctx).unwrap_or_default()),
                excerpt: Some(tuffbox_core::crash_kb::smart_excerpt(&haystack, 4000)),
                prefer_kb_only: false,
            };
            match swarm_node::diagnose_across_transports(&req).await {
                Ok(resp) => {
                    // Explain may read the network; MUST NOT persist peer capsules here.
                    resp.plan
                }
                Err(remote_err) => {
                    if let Some(plan) = integrations::global_capsule_library()
                        .diagnose_best(&fingerprint, &haystack)
                    {
                        kb_short_circuit = true;
                        plan
                    } else if let Some(plan) = strong_plan_from_similar(&ai_ctx) {
                        kb_short_circuit = true;
                        plan
                    } else {
                        let (prompt, compact) =
                            integrations::crash_explain_prompt_for(&settings.ai, &ai_ctx);
                        compact_prompt_used = compact;
                        let value = integrations::call_ai_crash_explain(&settings.ai, &prompt)
                            .await
                            .map_err(|e| {
                                format!("server diagnose failed ({remote_err}); local AI: {e}")
                            })?;
                        let raw = serde_json::to_string(&value).unwrap_or_default();
                        tuffbox_core::action_plan::parse_action_plan(&raw).map_err(|e| {
                            format!("server diagnose failed ({remote_err}); local parse: {e}")
                        })?
                    }
                }
            }
        }
        tuffbox_core::action_plan::DiagnoseMode::Local => {
            let mut ctx = ai_ctx.clone();
            if online_kb {
                network_used = true;
                let req = tuffbox_core::crash_remote::CrashLookupRequest {
                    fingerprint: fingerprint.clone(),
                    excerpt: Some(tuffbox_core::crash_kb::smart_excerpt(&haystack, 2000)),
                    mc_version: Some(ctx.mc_version.clone()),
                    loader: Some(ctx.loader.clone()),
                    limit: 5,
                };
                if let Some(resp) = swarm_node::lookup_across_transports(&req).await {
                    let mut remote =
                        tuffbox_core::crash_remote::hits_to_similar_cases(&resp.hits);
                    remote.extend(ctx.similar_cases.drain(..));
                    let mut seen = std::collections::HashSet::new();
                    remote.retain(|h| seen.insert(h.id.clone()));
                    ctx.similar_cases = remote;
                }
            }
            // Prefer strong KB/capsule hit before tiny Ollama models.
            if let Some(plan) = integrations::global_capsule_library()
                .diagnose_best(&fingerprint, &haystack)
                .filter(|p| p.confidence >= tuffbox_core::swarm::STRONG_MATCH_THRESHOLD)
            {
                network_used = swarm_on || network_used;
                kb_short_circuit = true;
                plan
            } else if let Some(plan) = strong_plan_from_similar(&ctx) {
                kb_short_circuit = true;
                plan
            } else {
                let (prompt, compact) = integrations::crash_explain_prompt_for(&settings.ai, &ctx);
                compact_prompt_used = compact;
                let value = integrations::call_ai_crash_explain(&settings.ai, &prompt).await?;
                let raw = serde_json::to_string(&value).unwrap_or_default();
                tuffbox_core::action_plan::parse_action_plan(&raw)?
            }
        }
        tuffbox_core::action_plan::DiagnoseMode::KbOnly => {
            if online_kb {
                network_used = true;
                let req = tuffbox_core::crash_remote::CrashLookupRequest {
                    fingerprint: fingerprint.clone(),
                    excerpt: Some(tuffbox_core::crash_kb::smart_excerpt(&haystack, 2000)),
                    mc_version: Some(ai_ctx.mc_version.clone()),
                    loader: Some(ai_ctx.loader.clone()),
                    limit: 1,
                };
                match swarm_node::lookup_across_transports(&req).await {
                    Some(resp) => {
                        let hit = resp.hits.first().ok_or_else(|| {
                            "no remote KB hits for this fingerprint".to_string()
                        })?;
                        kb_short_circuit = true;
                        tuffbox_core::action_plan::plan_from_launcher_actions(
                            &hit.solution,
                            &hit.suspected_mods,
                            hit.actions.clone(),
                            &hit.id,
                            hit.score,
                        )
                    }
                    None => {
                        if let Some(plan) = integrations::global_capsule_library()
                            .diagnose_best(&fingerprint, &haystack)
                        {
                            kb_short_circuit = true;
                            plan
                        } else {
                            let cases = tuffbox_core::crash_kb::load_all_cases(&project_dir);
                            let similar = tuffbox_core::crash_kb::search_similar(
                                &cases,
                                &fingerprint,
                                &haystack,
                                1,
                            );
                            let hit = similar.first().ok_or_else(|| {
                                "no local KB hits for this fingerprint".to_string()
                            })?;
                            kb_short_circuit = true;
                            tuffbox_core::action_plan::plan_from_kb_hit(
                                &hit.solution,
                                &hit.suspected_mods,
                                &hit.actions,
                                &hit.id,
                                hit.score,
                            )
                        }
                    }
                }
            } else if swarm_on {
                if let Some(plan) = integrations::global_capsule_library()
                    .diagnose_best(&fingerprint, &haystack)
                {
                    network_used = true;
                    kb_short_circuit = true;
                    plan
                } else {
                    let cases = tuffbox_core::crash_kb::load_all_cases(&project_dir);
                    let similar =
                        tuffbox_core::crash_kb::search_similar(&cases, &fingerprint, &haystack, 1);
                    let hit = similar
                        .first()
                        .ok_or_else(|| "no local KB hits for this fingerprint".to_string())?;
                    kb_short_circuit = true;
                    tuffbox_core::action_plan::plan_from_kb_hit(
                        &hit.solution,
                        &hit.suspected_mods,
                        &hit.actions,
                        &hit.id,
                        hit.score,
                    )
                }
            } else {
                let cases = tuffbox_core::crash_kb::load_all_cases(&project_dir);
                let similar =
                    tuffbox_core::crash_kb::search_similar(&cases, &fingerprint, &haystack, 1);
                let hit = similar
                    .first()
                    .ok_or_else(|| "no local KB hits for this fingerprint".to_string())?;
                kb_short_circuit = true;
                tuffbox_core::action_plan::plan_from_kb_hit(
                    &hit.solution,
                    &hit.suspected_mods,
                    &hit.actions,
                    &hit.id,
                    hit.score,
                )
            }
        }
        // Server mode without network → strong KB first, else local LLM.
        tuffbox_core::action_plan::DiagnoseMode::Server => {
            if swarm_on {
                if let Some(plan) = integrations::global_capsule_library()
                    .diagnose_best(&fingerprint, &haystack)
                {
                    if plan.confidence >= tuffbox_core::swarm::STRONG_MATCH_THRESHOLD {
                        network_used = true;
                        kb_short_circuit = true;
                        plan
                    } else if let Some(plan) = strong_plan_from_similar(&ai_ctx) {
                        kb_short_circuit = true;
                        plan
                    } else {
                        let (prompt, compact) =
                            integrations::crash_explain_prompt_for(&settings.ai, &ai_ctx);
                        compact_prompt_used = compact;
                        let value =
                            integrations::call_ai_crash_explain(&settings.ai, &prompt).await?;
                        let raw = serde_json::to_string(&value).unwrap_or_default();
                        tuffbox_core::action_plan::parse_action_plan(&raw)?
                    }
                } else if let Some(plan) = strong_plan_from_similar(&ai_ctx) {
                    kb_short_circuit = true;
                    plan
                } else {
                    let (prompt, compact) =
                        integrations::crash_explain_prompt_for(&settings.ai, &ai_ctx);
                    compact_prompt_used = compact;
                    let value = integrations::call_ai_crash_explain(&settings.ai, &prompt).await?;
                    let raw = serde_json::to_string(&value).unwrap_or_default();
                    tuffbox_core::action_plan::parse_action_plan(&raw)?
                }
            } else if let Some(plan) = strong_plan_from_similar(&ai_ctx) {
                kb_short_circuit = true;
                plan
            } else {
                let (prompt, compact) =
                    integrations::crash_explain_prompt_for(&settings.ai, &ai_ctx);
                compact_prompt_used = compact;
                let value = integrations::call_ai_crash_explain(&settings.ai, &prompt).await?;
                let raw = serde_json::to_string(&value).unwrap_or_default();
                tuffbox_core::action_plan::parse_action_plan(&raw)?
            }
        }
    };

    // Inventory grounding + Crash Assistant overlay (all modes).
    let grounded = tuffbox_core::action_plan::ground_action_plan(
        plan,
        &inventory_ids,
        &missing_ids,
    );
    let normalize_notes = grounded.notes;
    plan = tuffbox_core::action_plan::overlay_crash_assistant_findings(
        grounded.plan,
        &ai_ctx.crash_assistant_findings,
    );
    if plan.source.is_none() {
        plan.source = Some(if kb_short_circuit {
            "kb".into()
        } else {
            "ai".into()
        });
    }

    let pending_path =
        swarm_api::maybe_persist_pending_from_plan(&project_dir, &plan, network_used);
    let validation = tuffbox_core::action_plan::validate_action_plan_with_inventory(
        &plan,
        &inventory_ids,
        &missing_ids,
    );
    let legacy = tuffbox_core::action_plan::plan_to_legacy_ai_actions(&plan);

    Ok(serde_json::json!({
        "schemaVersion": plan.schema_version,
        "humanExplanation": plan.human_explanation,
        "human_explanation": plan.human_explanation,
        "confidence": plan.confidence,
        "suspectedMods": plan.suspected_mods,
        "suspected_mods": plan.suspected_mods,
        "needsUserReview": plan.needs_user_review,
        "needs_user_review": plan.needs_user_review,
        "source": plan.source,
        "matchedCaseIds": plan.matched_case_ids,
        "actions": plan.actions,
        "recommendedActions": legacy,
        "recommended_actions": legacy,
        "additionalContext": plan.additional_context,
        "validation": validation,
        "diagnoseMode": mode.as_str(),
        "provider": settings.ai.provider,
        "model": settings.ai.model,
        "similarCaseCount": similar_count,
        "fingerprintKey": fingerprint.key,
        "swarmEnabled": swarm_on,
        "networkUsed": network_used,
        "compactPromptUsed": compact_prompt_used,
        "kbShortCircuit": kb_short_circuit,
        "normalizeNotes": normalize_notes,
        "pendingPlanPath": pending_path.map(|p| p.to_string_lossy().to_string()),
    }))
}

fn strong_plan_from_similar(
    ctx: &tuffbox_core::ai_explanation::CrashAiContext,
) -> Option<tuffbox_core::action_plan::ActionPlan> {
    let hit = ctx
        .similar_cases
        .iter()
        .max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
    if hit.score < tuffbox_core::swarm::STRONG_MATCH_THRESHOLD {
        return None;
    }
    let mut plan = tuffbox_core::action_plan::plan_from_kb_hit(
        &hit.solution,
        &hit.suspected_mods,
        &hit.actions,
        &hit.id,
        hit.score,
    );
    plan.source = Some("kb".into());
    Some(plan)
}

/// Apply a validated ActionPlan (after user confirm). Runs snapshot once, then each op.
#[tauri::command(rename_all = "camelCase")]
async fn apply_action_plan(
    app: tauri::AppHandle,
    path: String,
    plan: tuffbox_core::action_plan::ActionPlan,
    fingerprint_key: Option<String>,
) -> Result<serde_json::Value, String> {
    let validation = tuffbox_core::action_plan::validate_action_plan(&plan);
    if !validation.ok {
        return Err(format!(
            "ActionPlan validation failed: {}",
            validation.errors.join("; ")
        ));
    }
    let manifest_path = PathBuf::from(&path);
    let snapshot = swarm_api::auto_snapshot_crash_fix(
        &manifest_path,
        &plan,
        fingerprint_key.as_deref(),
    )?;

    let mut applied = Vec::new();
    let mut errors = Vec::new();

    for action in &plan.actions {
        if action.op == "edit_config" {
            match apply_launcher_edit_config(&manifest_path, action) {
                Ok(msg) => applied.push(msg),
                Err(e) => errors.push(e),
            }
            continue;
        }
        if action.op == "change_mod_version" {
            let mod_id = action
                .mod_id
                .clone()
                .unwrap_or_default();
            let version = action.version.clone().unwrap_or_default();
            if mod_id.is_empty() || version.is_empty() {
                errors.push("change_mod_version requires modId and version".into());
                continue;
            }
            let mut manifest =
                ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
            match update_mod_from_modrinth(
                &manifest_path,
                &mut manifest,
                &mod_id,
                Some(version.as_str()),
            ) {
                Ok(()) => {
                    save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
                    applied.push(format!("changed {mod_id} to {version}"));
                }
                Err(e) => errors.push(e.to_string()),
            }
            continue;
        }
        if let Some(fix) = tuffbox_core::action_plan::launcher_action_to_fix_action(action) {
            match apply_fix_action(app.clone(), path.clone(), fix).await {
                Ok(msg) => applied.push(msg),
                Err(e) => errors.push(e),
            }
        } else {
            errors.push(format!("cannot map op '{}' to a fix action", action.op));
        }
    }

    // Record co-occurrence after successful crash-fix apply (local + optional Supabase).
    if errors.is_empty() {
        let _ = swarm_api::record_and_upload_cooccurrence(&path, &[], "crash_fix_apply").await;
    }

    Ok(serde_json::json!({
        "applied": applied,
        "errors": errors,
        "ok": errors.is_empty(),
        "snapshotId": snapshot.id,
        "snapshotTags": snapshot.tags,
    }))
}

fn apply_launcher_edit_config(
    manifest_path: &Path,
    action: &tuffbox_core::action_plan::LauncherAction,
) -> Result<String, String> {
    let relative = action
        .path
        .as_deref()
        .ok_or_else(|| "edit_config missing path".to_string())?;
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest has no parent".to_string())?;
    let target = safe_project_file(project_dir, relative)?;
    let current = if target.is_file() {
        std::fs::read_to_string(&target).map_err(|e| e.to_string())?
    } else {
        String::new()
    };
    let patch_type = action.patch_type.as_deref().unwrap_or("replace_file");
    let patch = action
        .patch
        .as_ref()
        .ok_or_else(|| "edit_config missing patch".to_string())?;
    let new_content =
        tuffbox_core::action_plan::apply_config_patch(&current, relative, patch_type, patch)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&target, new_content).map_err(|e| e.to_string())?;
    Ok(format!("edited config {relative}"))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CrashAiFeedbackPayload {
    helped: bool,
    fingerprint_key: Option<String>,
    human_explanation: Option<String>,
    suspected_mods: Option<Vec<String>>,
    recommended_actions: Option<Vec<tuffbox_core::ai_explanation::AiAction>>,
    report_id: Option<String>,
}

/// Record Helped/Wrong feedback into the project crash knowledge base.
#[tauri::command(rename_all = "camelCase")]
fn record_crash_ai_feedback(
    path: String,
    feedback: CrashAiFeedbackPayload,
) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
    let crash = load_scoped_crash_report(&project_dir, feedback.report_id.as_deref())
        .unwrap_or_default();
    let mut fp = tuffbox_core::crash_kb::fingerprint_from_text(
        &crash,
        &manifest.minecraft.version,
        &loader,
    );
    if let Some(key) = feedback.fingerprint_key.filter(|k| !k.is_empty()) {
        fp.key = key;
    }
    let actions = feedback.recommended_actions.unwrap_or_default();
    let mods = feedback.suspected_mods.unwrap_or_default();
    let path = tuffbox_core::crash_kb::record_feedback(
        &project_dir,
        &fp,
        feedback.helped,
        feedback.human_explanation.as_deref(),
        &actions,
        &mods,
    )?;
    Ok(path.to_string_lossy().to_string())
}

/// Author a private KB case from the current crash + your resolution.
#[tauri::command(rename_all = "camelCase")]
fn save_authored_crash_case(
    path: String,
    input: tuffbox_core::crash_kb::AuthorCaseInput,
) -> Result<tuffbox_core::crash_kb::AuthorCaseSaveResult, String> {
    let project_dir = manifest_parent(&path)?;
    tuffbox_core::crash_kb::save_authored_case(&project_dir, input)
}

/// Prefill author form: fingerprint + optional draft from AI analysis / report.
#[tauri::command(rename_all = "camelCase")]
fn draft_authored_crash_case(
    path: String,
    report_id: Option<String>,
) -> Result<serde_json::Value, String> {
    let project_dir = manifest_parent(&path)?;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
    let crash =
        load_scoped_crash_report(&project_dir, report_id.as_deref()).unwrap_or_default();
    let latest = project_dir.join("logs").join("latest.log");
    let latest_log = if latest.is_file() {
        tuffbox_core::process::read_log_tail(&latest, 900).unwrap_or_default()
    } else {
        String::new()
    };
    let haystack = format!("{crash}\n{latest_log}");
    let fingerprint = tuffbox_core::crash_kb::fingerprint_from_text(
        &haystack,
        &manifest.minecraft.version,
        &loader,
    );
    let symptoms: Vec<String> = [
        fingerprint.exception.clone(),
        fingerprint.mixin.clone().unwrap_or_default(),
        fingerprint.mod_file.clone().unwrap_or_default(),
    ]
    .into_iter()
    .filter(|s| !s.trim().is_empty())
    .collect();

    Ok(serde_json::json!({
        "fingerprint": fingerprint,
        "symptoms": symptoms,
        "mcVersion": manifest.minecraft.version,
        "loader": loader,
        "reportId": report_id,
        "authoredCount": tuffbox_core::crash_kb::list_authored_cases(&project_dir).len(),
    }))
}

#[tauri::command(rename_all = "camelCase")]
fn list_authored_crash_cases(path: String) -> Result<Vec<tuffbox_core::crash_kb::CrashCase>, String> {
    let project_dir = manifest_parent(&path)?;
    Ok(tuffbox_core::crash_kb::list_authored_cases(&project_dir))
}

#[tauri::command(rename_all = "camelCase")]
fn get_authored_case_export(path: String, case_id: String) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let case = tuffbox_core::crash_kb::list_authored_cases(&project_dir)
        .into_iter()
        .find(|c| c.id == case_id)
        .ok_or_else(|| format!("authored case not found: {case_id}"))?;
    let public = tuffbox_core::crash_kb::public_case_for_export(&case);
    serde_json::to_string_pretty(&public).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn open_authored_kb_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let dir = tuffbox_core::crash_kb::author_export_dir(&project_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    use tauri_plugin_shell::ShellExt;
    app.shell()
        .open(dir.to_string_lossy().to_string(), None)
        .map_err(|e| e.to_string())
}

/// ── Mod recommendation engine ─────────────────────────────────────

/// Lowercase alphanumeric-only token so `modernfix-mvus` / `ModernFix mVUS`
/// / `modernfix-neoforge-5.20.9.jar` collapse to comparable forms.
fn compact_mod_token(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn installed_mod_keys(manifest: &ProjectManifest) -> std::collections::HashSet<String> {
    let mut keys = std::collections::HashSet::new();
    for m in &manifest.mods {
        if m.content_type != tuffbox_core::manifest::ContentType::Mod {
            continue;
        }
        keys.insert(m.id.to_lowercase());
        keys.insert(m.name.to_lowercase());
        if let Some(pid) = &m.source.project_id {
            keys.insert(pid.to_lowercase());
        }
        if let Some(file) = &m.file_name {
            keys.insert(file.to_lowercase());
            if let Some(stem) = std::path::Path::new(file)
                .file_stem()
                .and_then(|s| s.to_str())
            {
                keys.insert(stem.to_lowercase());
            }
        }
    }
    keys
}

/// True if any installed mod covers `aliases` — exact slug/name match, or a
/// fork/port whose id/name/jar starts with (or clearly contains) the alias
/// (e.g. `modernfix-mvus` covers `modernfix`).
fn has_installed(keys: &std::collections::HashSet<String>, aliases: &[&str]) -> bool {
    let compact_keys: Vec<String> = keys
        .iter()
        .map(|k| compact_mod_token(k))
        .filter(|k| k.len() >= 3)
        .collect();

    for alias in aliases {
        let lower = alias.to_lowercase();
        if keys.contains(&lower) {
            return true;
        }
        let ac = compact_mod_token(alias);
        if ac.len() < 3 {
            continue;
        }
        for ck in &compact_keys {
            if ck == &ac {
                return true;
            }
            // Short slugs (emi, jei, iris, …) stay exact-only to avoid
            // false positives inside unrelated names.
            if ac.len() < 6 {
                continue;
            }
            // Installed fork/port of the suggested mod.
            if ck.starts_with(&ac) {
                return true;
            }
            // e.g. jar `…-modernfix-…` or name with prefix noise.
            if ck.contains(&ac) {
                return true;
            }
        }
    }
    false
}

/// Known Modrinth slug families for heuristic filtering (ports/forks/loaders).
fn recommendation_aliases(slug: &str) -> Vec<&'static str> {
    match slug {
        "ferrite-core" | "ferritecore" => vec!["ferrite-core", "ferritecore"],
        "entityculling" | "entity-culling" => vec!["entityculling", "entity-culling"],
        "embeddium" => vec!["embeddium", "rubidium", "sodium", "magnesium"],
        "rubidium" => vec!["rubidium", "embeddium", "sodium", "magnesium"],
        "sodium" => vec!["sodium", "embeddium", "rubidium", "magnesium"],
        "iris" => vec!["iris", "oculus"],
        "oculus" => vec!["oculus", "iris"],
        "emi" => vec!["emi", "roughly-enough-items", "jei", "rei"],
        "jei" => vec!["jei", "emi", "roughly-enough-items", "rei"],
        "modernfix" => vec!["modernfix", "modernfix-mvus"],
        "lithium" => vec!["lithium", "radium", "canary"],
        "radium" => vec!["radium", "lithium", "canary"],
        "fabric-api" | "fabric_api" => vec!["fabric-api", "fabric_api"],
        _ => Vec::new(),
    }
}

fn aliases_for_candidate(slug: &'static str) -> Vec<&'static str> {
    let mut aliases = recommendation_aliases(slug);
    if !aliases.iter().any(|a| *a == slug) {
        aliases.insert(0, slug);
    }
    aliases
}

type RecCandidate = (&'static str, &'static str, &'static str, &'static str);

fn optimization_candidates(loader: &str) -> Vec<RecCandidate> {
    match loader {
        "fabric" | "quilt" => vec![
            ("sodium", "Sodium", "Modern rendering engine — large FPS gains", "optimization"),
            ("lithium", "Lithium", "General game-logic / tick optimizations", "optimization"),
            ("ferrite-core", "FerriteCore", "Lowers memory usage of game state", "optimization"),
            ("immediatelyfast", "ImmediatelyFast", "Faster immediate-mode rendering", "optimization"),
            ("modernfix", "ModernFix", "Performance and launch-time bugfixes", "optimization"),
            ("entityculling", "Entity Culling", "Skip rendering of occluded entities", "optimization"),
            ("iris", "Iris", "Shader loader built for Sodium", "optimization"),
            ("indium", "Indium", "Fabric Rendering API bridge for Sodium", "optimization"),
            ("krypton", "Krypton", "Network stack optimizations", "optimization"),
            ("lazydfu", "LazyDFU", "Speeds up DataFixerUpper init", "optimization"),
        ],
        "forge" => vec![
            ("embeddium", "Embeddium", "Sodium-based renderer for Forge", "optimization"),
            ("ferritecore", "FerriteCore", "Memory usage reductions", "optimization"),
            ("modernfix", "ModernFix", "Performance and launch-time bugfixes", "optimization"),
            ("entityculling", "Entity Culling", "Skip rendering of occluded entities", "optimization"),
            ("immediatelyfast", "ImmediatelyFast", "Faster immediate-mode rendering", "optimization"),
            ("oculus", "Oculus", "Iris-like shaders for Forge/Embeddium", "optimization"),
            ("radium", "Radium", "Lithium-like server/tick optimizations for Forge", "optimization"),
        ],
        "neoforge" => vec![
            ("embeddium", "Embeddium", "Sodium-based renderer for NeoForge", "optimization"),
            ("ferritecore", "FerriteCore", "Memory usage reductions", "optimization"),
            ("modernfix", "ModernFix", "Performance and launch-time bugfixes", "optimization"),
            ("entityculling", "Entity Culling", "Skip rendering of occluded entities", "optimization"),
            ("immediatelyfast", "ImmediatelyFast", "Faster immediate-mode rendering", "optimization"),
            ("oculus", "Oculus", "Shader loader compatible with Embeddium", "optimization"),
        ],
        _ => vec![
            ("ferrite-core", "FerriteCore", "Memory usage reductions", "optimization"),
            ("modernfix", "ModernFix", "Performance and launch-time bugfixes", "optimization"),
        ],
    }
}

fn qol_candidates(loader: &str) -> Vec<RecCandidate> {
    let recipe_viewer: RecCandidate = match loader {
        "fabric" | "quilt" => (
            "emi",
            "EMI",
            "Modern recipe viewer (REI/JEI alternative for Fabric)",
            "qol",
        ),
        _ => ("jei", "JEI", "Recipe viewer — essential for modded Minecraft", "qol"),
    };
    vec![
        recipe_viewer,
        (
            "jade",
            "Jade",
            "Shows block/entity info when looking at them",
            "qol",
        ),
        (
            "appleskin",
            "AppleSkin",
            "Hunger and saturation overlay for food",
            "qol",
        ),
        (
            "mouse-tweaks",
            "Mouse Tweaks",
            "Better inventory mouse handling",
            "qol",
        ),
        (
            "controlling",
            "Controlling",
            "Searchable keybind menu",
            "qol",
        ),
    ]
}

fn push_rec(
    out: &mut Vec<serde_json::Value>,
    reason: &str,
    slug: &str,
    name: &str,
    description: &str,
    priority: &str,
) {
    if out.iter().any(|r| r.get("slug").and_then(|v| v.as_str()) == Some(slug)) {
        return;
    }
    out.push(serde_json::json!({
        "reason": reason,
        "slug": slug,
        "name": name,
        "description": description,
        "priority": priority,
    }));
}

fn heuristic_mod_recommendations(manifest: &ProjectManifest) -> Vec<serde_json::Value> {
    let keys = installed_mod_keys(manifest);
    let loader = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind);
    let mc = manifest.minecraft.version.clone();
    let mut recommendations = Vec::new();

    for (slug, name, desc, reason) in optimization_candidates(loader) {
        let aliases = aliases_for_candidate(slug);
        if !has_installed(&keys, &aliases) {
            push_rec(&mut recommendations, reason, slug, name, desc, "high");
        }
    }

    for (slug, name, desc, reason) in qol_candidates(loader) {
        let aliases = aliases_for_candidate(slug);
        if !has_installed(&keys, &aliases) {
            push_rec(&mut recommendations, reason, slug, name, desc, "medium");
        }
    }

    if matches!(loader, "fabric" | "quilt")
        && !has_installed(&keys, &aliases_for_candidate("fabric-api"))
    {
        push_rec(
            &mut recommendations,
            "dependency",
            "fabric-api",
            "Fabric API",
            "Required by most Fabric/Quilt mods",
            "critical",
        );
    }

    if has_installed(&keys, &["create"]) {
        for (slug, name, desc) in [
            (
                "createaddition",
                "Create Crafts & Additions",
                "Electricity and extras for Create",
            ),
            (
                "create-steam-n-rails",
                "Create: Steam 'n' Rails",
                "Trains and advanced rails for Create",
            ),
        ] {
            if !has_installed(&keys, &aliases_for_candidate(slug)) {
                push_rec(&mut recommendations, "synergy", slug, name, desc, "low");
            }
        }
    }

    // Annotate with pack context so the UI can show why these were picked.
    for rec in &mut recommendations {
        if let Some(obj) = rec.as_object_mut() {
            obj.insert("loader".into(), serde_json::json!(loader));
            obj.insert("minecraftVersion".into(), serde_json::json!(mc));
            obj.insert("source".into(), serde_json::json!("heuristic"));
        }
    }

    recommendations
}

fn build_mod_recommendation_prompt(manifest: &ProjectManifest, seed: &[serde_json::Value]) -> String {
    let loader = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind);
    let mc = &manifest.minecraft.version;
    let installed: Vec<String> = manifest
        .mods
        .iter()
        .filter(|m| m.content_type == tuffbox_core::manifest::ContentType::Mod)
        .take(80)
        .map(|m| format!("{} ({})", m.id, m.name))
        .collect();
    let seed_slugs: Vec<String> = seed
        .iter()
        .filter_map(|r| r.get("slug").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();

    format!(
        r#"You are a Minecraft modpack optimization advisor.
Return ONLY valid JSON with this schema:
{{
  "recommendations": [
    {{
      "slug": "modrinth-slug",
      "name": "Display Name",
      "description": "Why this helps this pack",
      "priority": "critical|high|medium|low",
      "reason": "optimization|qol|dependency|synergy"
    }}
  ]
}}

Rules:
- Suggest at most 8 mods.
- Prefer performance / optimization mods that exist on Modrinth for loader "{loader}" and Minecraft {mc}.
- Do NOT suggest mods already installed.
- Do NOT suggest a mod if a fork, port, or unofficial build of it is already installed (e.g. modernfix-mvus covers modernfix; rubidium/embeddium cover sodium).
- Do NOT suggest Fabric-only mods for Forge/NeoForge (e.g. no Sodium on Forge — use Embeddium).
- Do NOT suggest Forge-only mods for Fabric/Quilt.
- Prefer well-known Modrinth slugs (sodium, lithium, embeddium, modernfix, ferrite-core, iris, oculus, jei, emi).
- Skip anything incompatible with {loader} {mc}.

Installed mods:
{}

Heuristic seed suggestions (refine/replace if wrong):
{}
"#,
        installed.join(", "),
        seed_slugs.join(", ")
    )
}

/// Analyzes the current modpack and suggests optimization / QoL mods for the
/// active loader + Minecraft version. Uses heuristics first, then optionally
/// refines via the configured AI provider.
#[tauri::command(rename_all = "camelCase")]
async fn recommend_mods(path: String) -> Result<Vec<serde_json::Value>, String> {
    let path_clone = path.clone();
    let mut recommendations = tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path_clone).map_err(|e| e.to_string())?;
        Ok::<_, String>(heuristic_mod_recommendations(&manifest))
    })
    .await
    .map_err(|e| e.to_string())??;

    // Best-effort AI refinement — never fail the whole command if AI is offline.
    let ai_result = tokio::task::spawn_blocking({
        let path = path.clone();
        let seed = recommendations.clone();
        move || -> Result<Vec<serde_json::Value>, String> {
            let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
            let prompt = build_mod_recommendation_prompt(&manifest, &seed);
            Ok(vec![serde_json::json!({
                "_prompt": prompt,
                "_loader": tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind),
                "_mc": manifest.minecraft.version,
            })])
        }
    })
    .await;

    if let Ok(Ok(meta)) = ai_result {
        if let Some(prompt) = meta
            .first()
            .and_then(|v| v.get("_prompt"))
            .and_then(|v| v.as_str())
        {
            let settings = integrations::get_integration_status().settings;
            if let Ok(ai_json) = integrations::call_ai(&settings.ai, prompt).await {
                let keys = tokio::task::spawn_blocking({
                    let path = path.clone();
                    move || {
                        ProjectManifest::load_from_path(&path)
                            .map(|m| installed_mod_keys(&m))
                            .unwrap_or_default()
                    }
                })
                .await
                .unwrap_or_default();

                let ai_recs = ai_json
                    .get("recommendations")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                let mut merged = Vec::new();
                for rec in ai_recs {
                    let Some(slug) = rec.get("slug").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let mut aliases = recommendation_aliases(slug);
                    aliases.push(slug);
                    if has_installed(&keys, &aliases) {
                        continue;
                    }
                    let name = rec
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(slug);
                    let description = rec
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("AI-suggested optimization mod");
                    let priority = rec
                        .get("priority")
                        .and_then(|v| v.as_str())
                        .unwrap_or("high");
                    let reason = rec
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("optimization");
                    let mut obj = serde_json::json!({
                        "reason": reason,
                        "slug": slug,
                        "name": name,
                        "description": description,
                        "priority": priority,
                        "source": "ai",
                    });
                    if let Some(map) = obj.as_object_mut() {
                        if let Some(m) = meta.first().and_then(|v| v.as_object()) {
                            if let Some(loader) = m.get("_loader") {
                                map.insert("loader".into(), loader.clone());
                            }
                            if let Some(mc) = m.get("_mc") {
                                map.insert("minecraftVersion".into(), mc.clone());
                            }
                        }
                    }
                    merged.push(obj);
                }

                if !merged.is_empty() {
                    // Prefer AI list, then fill gaps from heuristics.
                    for h in recommendations {
                        let slug = h.get("slug").and_then(|v| v.as_str()).unwrap_or("");
                        if !merged
                            .iter()
                            .any(|r| r.get("slug").and_then(|v| v.as_str()) == Some(slug))
                        {
                            merged.push(h);
                        }
                    }
                    recommendations = merged;
                }
            }
        }
    }

    // Cap the list so the panel stays usable.
    recommendations.truncate(12);
    Ok(recommendations)
}

#[cfg(test)]
mod recommend_mod_tests {
    use super::{compact_mod_token, has_installed};
    use std::collections::HashSet;

    #[test]
    fn compact_strips_separators() {
        assert_eq!(compact_mod_token("ModernFix-mVUS"), "modernfixmvus");
        assert_eq!(compact_mod_token("modernfix_neoforge-5.20.9"), "modernfixneoforge5209");
    }

    #[test]
    fn modernfix_mvus_covers_modernfix() {
        let mut keys = HashSet::new();
        keys.insert("modernfix-mvus".into());
        assert!(has_installed(&keys, &["modernfix"]));
    }

    #[test]
    fn modernfix_name_port_covers_modernfix() {
        let mut keys = HashSet::new();
        keys.insert("modernfix mvus".into());
        assert!(has_installed(&keys, &["modernfix"]));
    }

    #[test]
    fn jar_stem_covers_modernfix() {
        let mut keys = HashSet::new();
        keys.insert("modernfix-neoforge-5.20.9".into());
        assert!(has_installed(&keys, &["modernfix"]));
    }

    #[test]
    fn short_slug_stays_exact() {
        let mut keys = HashSet::new();
        keys.insert("something-with-emi-inside".into());
        assert!(!has_installed(&keys, &["emi"]));
        keys.insert("emi".into());
        assert!(has_installed(&keys, &["emi"]));
    }

    #[test]
    fn unrelated_mod_does_not_cover() {
        let mut keys = HashSet::new();
        keys.insert("sodium".into());
        assert!(!has_installed(&keys, &["modernfix"]));
    }
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
    if !backup_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("invalid backup id".into());
    }
    let project_dir = manifest_parent(&path)?;
    let zip_path = project_dir
        .join(".tuffbox")
        .join("backups")
        .join(format!("{}.zip", backup_id));
    if !zip_path.is_file() {
        return Err("backup not found".into());
    }

    // Safety: snapshot before restore
    let manifest_path = PathBuf::from(&path);
    auto_snapshot(&manifest_path, "before-restore").map_err(|e| e.to_string())?;

    let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if name.ends_with('/') {
            continue;
        }
        let target = project_dir.join(&name);
        let canonical = std::fs::canonicalize(&target)
            .or_else(|_| std::fs::canonicalize(target.parent().unwrap_or(&project_dir)))
            .map_err(|e| e.to_string())?;
        if !canonical.starts_with(
            std::fs::canonicalize(&project_dir).map_err(|e| e.to_string())?
        ) {
            return Err(format!("zip entry escapes project directory: {name}"));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut dest = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut dest).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// ── Problematic mods config ─────────────────────────────────────

/// Writes a problematic-mods.json config for mods known to cause crashes.
/// Compatible with Crash Assistant's problematic_mods_config.json format.
#[tauri::command(rename_all = "camelCase")]
fn save_problematic_mods_config(
    path: String,
    entries: Vec<serde_json::Value>,
) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let p = project_dir
        .join("config")
        .join("problematic_mods_config.json");
    if let Some(par) = p.parent() {
        std::fs::create_dir_all(par).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&serde_json::json!({ "mods": entries }))
        .map_err(|e| e.to_string())?;
    std::fs::write(&p, json).map_err(|e| e.to_string())
}

/// Returns the current problematic mods config.
#[tauri::command(rename_all = "camelCase")]
fn get_problematic_mods_config(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let p = project_dir
        .join("config")
        .join("problematic_mods_config.json");
    if !p.is_file() {
        return Ok(vec![]);
    }
    let raw = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    Ok(v.get("mods")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default())
}

/// ── Server launch ────────────────────────────────────────────────

/// Launches the server profile and captures the log. Prepares the instance
/// with server-safe mods, generates server.properties, and starts the JVM.
#[tauri::command(rename_all = "camelCase")]
async fn launch_server(
    app: tauri::AppHandle,
    path: String,
) -> Result<tuffbox_core::LaunchResult, LaunchErrorInfo> {
    record_launch(path.clone()).map_err(|e| {
        LaunchErrorInfo::new(LaunchErrorKind::Unknown, e.to_string())
    })?;
    launch_profile(app, path, "server".into()).await
}

/// Generates a default server.properties file for the project.
#[tauri::command(rename_all = "camelCase")]
fn generate_server_properties(path: String) -> Result<String, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let profile = manifest
        .profiles
        .iter()
        .find(|p| p.id == "server")
        .or_else(|| manifest.profiles.first());

    let mut props = String::new();
    props.push_str(
        "# TuffBox generated server.properties
",
    );
    props.push_str(&format!(
        "server-port=25565
"
    ));
    props.push_str(&format!(
        "max-players=20
"
    ));
    props.push_str(&format!(
        "view-distance=10
"
    ));
    props.push_str(&format!(
        "simulation-distance=10
"
    ));
    props.push_str(&format!(
        "max-world-size=29999984
"
    ));
    props.push_str(&format!(
        "allow-flight=false
"
    ));
    props.push_str(&format!(
        "online-mode=true
"
    ));
    props.push_str(&format!(
        "difficulty=normal
"
    ));
    props.push_str(&format!(
        "gamemode=survival
"
    ));
    props.push_str(&format!(
        "enable-command-block=false
"
    ));
    props.push_str(&format!(
        "spawn-protection=16
"
    ));
    props.push_str(&format!(
        "max-tick-time=60000
"
    ));
    props.push_str(&format!(
        "level-name=world
"
    ));
    props.push_str(&format!(
        "motd=A TuffBox {} Server\n",
        manifest.project.name
    ));

    if let Some(profile) = profile {
        if let Some(mem) = profile.memory_mb {
            props.push_str(&format!(
                "# Memory: {} MB
",
                mem
            ));
        }
    }

    let project_dir = manifest_parent(&path)?;
    let target = project_dir.join("server.properties");
    std::fs::write(&target, &props).map_err(|e| e.to_string())?;
    Ok(props)
}

/// ── Recipe scanner from actual JARs ──────────────────────────────

/// Scans mod JAR / datapack / KubeJS recipes with JEI-style layouts.
#[tauri::command(rename_all = "camelCase")]
async fn scan_mod_recipes(path: String) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let result = tuffbox_core::recipe_scan::scan_project_recipes(Path::new(&path))?;
        serde_json::to_value(result).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn recipe_icon_extra_jars(manifest_path: &Path) -> Result<(PathBuf, Vec<PathBuf>), String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest path has no parent".to_string())?;
    let manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    let mut extra_jars = Vec::new();
    let version = &manifest.minecraft.version;
    let mut roots = Vec::new();
    if let Some(data) = dirs::data_dir() {
        roots.push(data.join("TuffBox"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(&appdata).join("TuffBox"));
        roots.push(PathBuf::from(appdata).join(".minecraft"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".minecraft"));
    }
    for root in roots {
        let client_jar = root
            .join("versions")
            .join(version)
            .join(format!("{version}.jar"));
        if client_jar.is_file() && !extra_jars.iter().any(|p| p == &client_jar) {
            extra_jars.push(client_jar);
        }
    }
    Ok((project_dir.to_path_buf(), extra_jars))
}

/// Returns a cached PNG path for a Minecraft item id (`namespace:path`), extracted
/// from the project mods and the installed vanilla client jar when available.
#[tauri::command(rename_all = "camelCase")]
async fn get_item_icon(path: String, item_id: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let (project_dir, extra_jars) = recipe_icon_extra_jars(&manifest_path)?;
        let icon = tuffbox_core::resolve_item_icon_data_url(&project_dir, &item_id, &extra_jars)?;
        Ok(icon)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Resolves many item icons in one IPC call (opens each mod jar once).
#[tauri::command(rename_all = "camelCase")]
async fn get_item_icons_batch(
    path: String,
    item_ids: Vec<String>,
) -> Result<std::collections::HashMap<String, Option<String>>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let (project_dir, extra_jars) = recipe_icon_extra_jars(&manifest_path)?;
        let icons =
            tuffbox_core::resolve_item_icons_data_urls(&project_dir, &item_ids, &extra_jars)?;
        Ok(icons)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn get_recipe_runtime_status(path: String) -> tuffbox_core::RecipeRuntimeStatus {
    tokio::task::spawn_blocking(move || tuffbox_core::recipe_runtime_status(Path::new(&path)))
        .await
        .unwrap_or(tuffbox_core::RecipeRuntimeStatus {
            connected: false,
            supported: false,
            message: "Failed to check JEI runtime".to_string(),
            minecraft_version: None,
            pid: None,
        })
}

#[tauri::command(rename_all = "camelCase")]
async fn get_recipe_runtime_snapshot(path: String) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        tuffbox_core::fetch_recipe_runtime_snapshot(Path::new(&path))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Append recipe remove lines to kubejs/server_scripts/tuffbox_recipe_removes.js
#[tauri::command(rename_all = "camelCase")]
fn write_kubejs_recipe_removes(path: String, recipe_ids: Vec<String>) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    tuffbox_core::recipe_scan::write_kubejs_remove(&project_dir, &recipe_ids)
}

/// Append a shaped/shapeless/cooking/smithing/stonecutting craft to tuffbox_recipe_adds.js.
#[tauri::command(rename_all = "camelCase")]
fn write_kubejs_craft_recipe(
    path: String,
    draft: tuffbox_core::recipe_scan::CraftDraft,
) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    tuffbox_core::recipe_scan::write_kubejs_craft(&project_dir, &draft)
}

/// Append item-tag edits to kubejs/server_scripts/tuffbox_tag_edits.js.
#[tauri::command(rename_all = "camelCase")]
fn write_kubejs_tag_edits(
    path: String,
    draft: tuffbox_core::recipe_scan::TagDraft,
) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    tuffbox_core::recipe_scan::write_kubejs_tags(&project_dir, &draft)
}

/// List known item tags (`#ns:path`) from the offline tag index for the recipe palette.
#[tauri::command(rename_all = "camelCase")]
async fn list_item_tags(path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let (project_dir, extra_jars) = recipe_icon_extra_jars(&manifest_path)?;
        let loader = tuffbox_core::recipe_scan::loader_kind_from_manifest(&manifest_path)?;
        Ok(tuffbox_core::recipe_scan::list_item_tags(
            &project_dir,
            loader,
            &extra_jars,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Direct members of an item tag (not fully expanded).
#[tauri::command(rename_all = "camelCase")]
async fn get_item_tag_entries(path: String, tag_id: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let (project_dir, extra_jars) = recipe_icon_extra_jars(&manifest_path)?;
        let loader = tuffbox_core::recipe_scan::loader_kind_from_manifest(&manifest_path)?;
        Ok(tuffbox_core::recipe_scan::get_tag_entries(
            &project_dir,
            loader,
            &extra_jars,
            &tag_id,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Generate a KubeJS snippet (does not write to disk).
#[tauri::command(rename_all = "camelCase")]
fn generate_kubejs_recipe_script(
    kind: String,
    recipe_ids: Vec<String>,
    new_item: Option<String>,
    count: Option<u32>,
) -> Result<serde_json::Value, String> {
    let script = match kind.as_str() {
        "remove" => tuffbox_core::recipe_scan::kubejs_remove_script(&recipe_ids),
        "replace_output" => {
            let id = recipe_ids
                .first()
                .ok_or_else(|| "recipe id required".to_string())?;
            let item = new_item.unwrap_or_else(|| "minecraft:air".into());
            tuffbox_core::recipe_scan::kubejs_replace_output(id, &item, count.unwrap_or(1))
        }
        other => return Err(format!("unknown script kind: {other}")),
    };
    serde_json::to_value(script).map_err(|e| e.to_string())
}

/// Load FTB Quests chapters from project config via the SNBT parser.
#[tauri::command(rename_all = "camelCase")]
fn load_quest_book(path: String) -> Result<serde_json::Value, String> {
    let project_dir = manifest_parent(&path)?;
    let book = tuffbox_core::unified::QuestBook::load_from_project(&project_dir)?;
    serde_json::to_value(book).map_err(|e| e.to_string())
}

/// Save a single quest chapter back to disk as SNBT.
#[tauri::command(rename_all = "camelCase")]
fn save_quest_chapter(
    path: String,
    chapter: tuffbox_core::unified::Chapter,
    relative_path: Option<String>,
) -> Result<serde_json::Value, String> {
    let manifest_path = PathBuf::from(&path);
    let project_dir = manifest_parent(&path)?;
    let rel = tuffbox_core::unified::QuestBook::save_chapter(
        &project_dir,
        &chapter,
        relative_path.as_deref(),
    )?;
    auto_snapshot_with_changed_files(&manifest_path, "save-quest-chapter", &[PathBuf::from(&rel)])
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "relativePath": rel, "questCount": chapter.quests.len() }))
}

/// Validate quest book integrity (missing deps, empty tasks).
#[tauri::command(rename_all = "camelCase")]
fn validate_quest_book(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let book = tuffbox_core::unified::QuestBook::load_from_project(&project_dir)?;
    Ok(book
        .validate()
        .into_iter()
        .map(|e| serde_json::json!({ "questId": e.quest_id, "message": e.message }))
        .collect())
}

/// ── World management ────────────────────────────────────────────

/// Lists Minecraft worlds in the project's saves/ folder.
#[tauri::command(rename_all = "camelCase")]
fn list_worlds(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let saves_dir = project_dir.join("saves");
    if !saves_dir.is_dir() {
        return Ok(vec![]);
    }
    let mut worlds = Vec::new();
    for entry in std::fs::read_dir(&saves_dir)
        .into_iter()
        .flatten()
        .flatten()
    {
        let p = entry.path();
        if p.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            let level_dat = p.join("level.dat");
            let mut size: u64 = 0;
            fn dir_size(d: &std::path::Path, s: &mut u64) {
                for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        dir_size(&p, s);
                    } else if let Ok(m) = p.metadata() {
                        *s += m.len();
                    }
                }
            }
            dir_size(&p, &mut size);
            let has_level = level_dat.is_file();
            let size_str = if size < 1048576 {
                format!("{:.1} KB", size as f64 / 1024.0)
            } else if size < 1073741824 {
                format!("{:.1} MB", size as f64 / 1048576.0)
            } else {
                format!("{:.1} GB", size as f64 / 1073741824.0)
            };
            worlds.push(serde_json::json!({"name": name, "size": size, "sizeFormatted": size_str, "hasLevelDat": has_level}));
        }
    }
    worlds.sort_by_key(|w| -(w["size"].as_u64().unwrap_or(0) as i64));
    Ok(worlds)
}

/// Lists resourcepacks or shaderpacks on disk (zip/folders + `.disabled`).
#[tauri::command(rename_all = "camelCase")]
fn list_content_packs(path: String, folder: String) -> Result<Vec<tuffbox_core::content_packs::ContentPackEntry>, String> {
    if folder != "resourcepacks" && folder != "shaderpacks" {
        return Err("folder must be resourcepacks or shaderpacks".into());
    }
    let project_dir = manifest_parent(&path)?;
    tuffbox_core::content_packs::list_content_packs(&project_dir, &folder)
}

#[tauri::command(rename_all = "camelCase")]
fn set_content_pack_enabled(
    path: String,
    folder: String,
    file_name: String,
    enabled: bool,
) -> Result<tuffbox_core::content_packs::ContentPackEntry, String> {
    if folder != "resourcepacks" && folder != "shaderpacks" {
        return Err("folder must be resourcepacks or shaderpacks".into());
    }
    let project_dir = manifest_parent(&path)?;
    tuffbox_core::content_packs::set_content_pack_enabled(&project_dir, &folder, &file_name, enabled)
}

#[tauri::command(rename_all = "camelCase")]
fn list_mc_servers(path: String) -> Result<Vec<tuffbox_core::servers_dat::ServerEntry>, String> {
    let project_dir = manifest_parent(&path)?;
    tuffbox_core::servers_dat::list_servers(&project_dir)
}

#[tauri::command(rename_all = "camelCase")]
fn add_mc_server(
    path: String,
    name: String,
    address: String,
) -> Result<Vec<tuffbox_core::servers_dat::ServerEntry>, String> {
    let project_dir = manifest_parent(&path)?;
    tuffbox_core::servers_dat::add_server(&project_dir, &name, &address)
}

#[tauri::command(rename_all = "camelCase")]
fn remove_mc_server(
    path: String,
    address: String,
) -> Result<Vec<tuffbox_core::servers_dat::ServerEntry>, String> {
    let project_dir = manifest_parent(&path)?;
    tuffbox_core::servers_dat::remove_server(&project_dir, &address)
}

#[tauri::command(rename_all = "camelCase")]
fn ping_mc_server(address: String) -> Result<tuffbox_core::servers_dat::ServerPingResult, String> {
    Ok(tuffbox_core::servers_dat::ping_server_address(&address))
}

/// Backs up a single world as a zip archive.
#[tauri::command(rename_all = "camelCase")]
fn backup_world(path: String, world_name: String) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    if !world_dir.is_dir() {
        return Err("world not found".into());
    }
    let backup_dir = project_dir.join(".tuffbox").join("world-backups");
    std::fs::create_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    let zip_name = format!(
        "{}-{}.zip",
        world_name,
        tuffbox_core::time_util::compact_now()
    );
    let zip_path = backup_dir.join(&zip_name);
    let out = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(out);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    fn add_world(
        zip: &mut zip::ZipWriter<std::fs::File>,
        opts: zip::write::SimpleFileOptions,
        base: &std::path::Path,
        dir: &std::path::Path,
    ) -> Result<(), String> {
        for e in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            let p = e.path();
            if p.is_dir() {
                add_world(zip, opts, base, &p)?;
            } else if p.is_file() {
                let rel = p
                    .strip_prefix(base)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                zip.start_file(rel, opts).map_err(|e| e.to_string())?;
                zip.write_all(&std::fs::read(&p).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    let parent = world_dir.parent()
        .ok_or_else(|| "world path has no parent directory".to_string())?;
    add_world(&mut zip, opts, parent, &world_dir)?;
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
    std::fs::write(
        &p,
        serde_json::to_string_pretty(&template).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// Lists saved modpack templates.
#[tauri::command(rename_all = "camelCase")]
fn list_templates(path: String) -> Result<Vec<serde_json::Value>, String> {
    let project_dir = manifest_parent(&path)?;
    let template_dir = project_dir.join(".tuffbox").join("templates");
    if !template_dir.is_dir() {
        return Ok(vec![]);
    }
    let mut templates = Vec::new();
    for entry in std::fs::read_dir(&template_dir)
        .into_iter()
        .flatten()
        .flatten()
    {
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

static DOWNLOAD_PROGRESS: once_cell::sync::Lazy<
    std::sync::Mutex<std::collections::HashMap<String, (u64, u64)>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

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
    let ext = std::path::Path::new(&relative_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "json" => {
            if let Err(e) = serde_json::from_str::<serde_json::Value>(&content) {
                issues.push(serde_json::json!({"severity":"error","code":"JSON_SYNTAX","message":format!("JSON syntax error: {}", e),"line":null}));
            }
        }
        "properties" | "txt" => {
            let mut seen_keys = std::collections::HashSet::new();
            for (line_no, line) in content.lines().enumerate() {
                let t = line.trim();
                if t.is_empty() || t.starts_with('#') {
                    continue;
                }
                if !t.contains('=') && t.len() > 2 {
                    issues.push(serde_json::json!({"severity":"warning","code":"PROPERTY_NO_EQ","message":"Line without = sign","line":line_no+1}));
                    continue;
                }
                if let Some(eq) = t.find('=') {
                    let key = t[..eq].trim();
                    if key.is_empty() {
                        issues.push(serde_json::json!({"severity":"warning","code":"EMPTY_KEY","message":"Empty key","line":line_no+1}));
                    } else if !seen_keys.insert(key.to_string()) {
                        issues.push(serde_json::json!({"severity":"warning","code":"DUPLICATE_KEY","message":format!("Duplicate key: {}", key),"line":line_no+1}));
                    }
                }
            }
        }
        "toml" => {
            if let Err(e) = toml::from_str::<toml::Value>(&content) {
                issues.push(serde_json::json!({"severity":"error","code":"TOML_SYNTAX","message":format!("TOML syntax error: {}", e),"line":null}));
            }
        }
        _ => {}
    }

    // Check for common performance-sapping server settings
    if content.contains("max-tick-time=-1") {
        issues.push(serde_json::json!({"severity":"warning","code":"MAX_TICK_TIME_DISABLED","message":"max-tick-time is -1 (off). Server won't crash on overload but may freeze indefinitely.","line":null}));
    }
    if content.contains("view-distance=") {
        for line in content.lines() {
            if line.contains("view-distance=") {
                if let Some(v) = line.split('=').last() {
                    if let Ok(n) = v.trim().parse::<u32>() {
                        if n > 16 {
                            issues.push(serde_json::json!({"severity":"warning","code":"HIGH_VIEW_DISTANCE","message":format!("View distance {} may cause lag on modded servers.", n),"line":null}));
                        }
                    }
                }
                break;
            }
        }
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
    let known_files: std::collections::HashSet<String> = manifest
        .mods
        .iter()
        .filter_map(|m| m.file_name.clone())
        .collect();
    if mods_dir.is_dir() {
        for entry in std::fs::read_dir(&mods_dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if entry.path().extension().map_or(false, |e| e == "jar")
                && !known_files.contains(&name)
            {
                let _ = std::fs::remove_file(entry.path());
                cleaned.push(format!("mods/{}", name));
            }
        }
    }

    // Remove old test run logs (older than 30 days)
    let test_runs = project_dir.join(".tuffbox").join("test-runs");
    if test_runs.is_dir() {
        let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86400);
        for entry in std::fs::read_dir(&test_runs)
            .into_iter()
            .flatten()
            .flatten()
        {
            if entry.path().is_dir() {
                if let Ok(meta) = entry.metadata() {
                    if let Ok(mod_time) = meta.modified() {
                        if mod_time < cutoff {
                            let _ = std::fs::remove_dir_all(entry.path());
                            cleaned
                                .push(format!("test-runs/{}", entry.file_name().to_string_lossy()));
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
    Ok(env!("CARGO_PKG_VERSION").to_string())
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

/// ── World map (Anvil region reader) ──────────────────────────

/// Returns a mcaselector-style 2D overview of a world's region files:
/// per-region 32x32 chunk grids with presence, last-modified time and a
/// coarse generation status used for coloring.
#[tauri::command(rename_all = "camelCase")]
fn read_world_map(
    path: String,
    world_name: String,
    dimension: Option<String>,
) -> Result<serde_json::Value, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let map = tuffbox_core::region::read_world_map(&world_dir, dimension.as_deref())?;
    Ok(serde_json::to_value(&map).map_err(|e| e.to_string())?)
}

/// Lists dimensions that have a region folder for the given world.
#[tauri::command(rename_all = "camelCase")]
fn list_world_dimensions(path: String, world_name: String) -> Result<Vec<String>, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    Ok(tuffbox_core::region::list_world_dimensions(&world_dir))
}

/// A region coordinate paired with the local chunk indices (0..1024) to clear.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChunkSelection {
    region_x: i32,
    region_z: i32,
    indices: Vec<usize>,
}

/// Deletes selected chunks from a world's region files, mirroring mcaselector.
/// Each selection maps a region coordinate to the local chunk indices to clear.
#[tauri::command(rename_all = "camelCase")]
fn delete_world_chunks(
    path: String,
    world_name: String,
    selections: Vec<ChunkSelection>,
    dimension: Option<String>,
) -> Result<usize, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let pairs: Vec<(i32, i32, Vec<usize>)> = selections
        .into_iter()
        .map(|s| (s.region_x, s.region_z, s.indices))
        .collect();
    tuffbox_core::region::delete_world_chunks(&world_dir, &pairs, dimension.as_deref())
        .map_err(|e| format!("Failed to delete chunks: {}", e))
}

/// Copies selected chunks from a world's region files to a clipboard payload.
#[tauri::command(rename_all = "camelCase")]
fn copy_world_chunks(
    path: String,
    world_name: String,
    selections: Vec<ChunkSelection>,
    dimension: Option<String>,
) -> Result<tuffbox_core::region::ChunkClipboard, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let pairs: Vec<(i32, i32, Vec<usize>)> = selections
        .into_iter()
        .map(|s| (s.region_x, s.region_z, s.indices))
        .collect();
    tuffbox_core::region::copy_world_chunks(&world_dir, &world_name, &pairs, dimension.as_deref())
        .map_err(|e| format!("Failed to copy chunks: {}", e))
}

/// Pastes chunk data from a clipboard payload into a world's region files.
/// `offset_x` / `offset_z` are **chunk** coordinate offsets (MCA Selector style).
#[tauri::command(rename_all = "camelCase")]
fn paste_world_chunks(
    path: String,
    world_name: String,
    clipboard: tuffbox_core::region::ChunkClipboard,
    offset_x: Option<i32>,
    offset_z: Option<i32>,
    dimension: Option<String>,
) -> Result<usize, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    tuffbox_core::region::paste_world_chunks(
        &world_dir,
        &clipboard,
        offset_x.unwrap_or(0),
        offset_z.unwrap_or(0),
        dimension.as_deref(),
    )
    .map_err(|e| format!("Failed to paste chunks: {}", e))
}

/// Compacts region files (purge orphaned sectors after deletes).
#[tauri::command(rename_all = "camelCase")]
fn purge_world_regions(
    path: String,
    world_name: String,
    dimension: Option<String>,
) -> Result<usize, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    tuffbox_core::region::purge_world_regions(&world_dir, dimension.as_deref())
        .map_err(|e| format!("Failed to purge regions: {}", e))
}

/// Exports selected chunks into a destination folder (mini world).
#[tauri::command(rename_all = "camelCase")]
fn export_world_chunks(
    path: String,
    world_name: String,
    selections: Vec<ChunkSelection>,
    dest_dir: String,
    dimension: Option<String>,
) -> Result<usize, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let pairs: Vec<(i32, i32, Vec<usize>)> = selections
        .into_iter()
        .map(|s| (s.region_x, s.region_z, s.indices))
        .collect();
    tuffbox_core::region::export_world_chunks(
        &world_dir,
        &pairs,
        dimension.as_deref(),
        std::path::Path::new(&dest_dir),
    )
    .map_err(|e| format!("Failed to export chunks: {}", e))
}

/// Import chunks from another world / export folder into the target world.
#[tauri::command(rename_all = "camelCase")]
fn import_world_chunks(
    path: String,
    world_name: String,
    source_dir: String,
    offset_x: Option<i32>,
    offset_z: Option<i32>,
    overwrite: Option<bool>,
    y_offset: Option<i32>,
    sections: Option<String>,
    source_selections: Option<Vec<ChunkSelection>>,
    target_selections: Option<Vec<ChunkSelection>>,
    source_dimension: Option<String>,
    dimension: Option<String>,
) -> Result<usize, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let source_sels: Vec<(i32, i32, Vec<usize>)> = source_selections
        .unwrap_or_default()
        .into_iter()
        .map(|s| (s.region_x, s.region_z, s.indices))
        .collect();
    let target_sels: Option<Vec<(i32, i32, Vec<usize>)>> = target_selections.map(|v| {
        v.into_iter()
            .map(|s| (s.region_x, s.region_z, s.indices))
            .collect()
    });
    let opts = tuffbox_core::region::ImportOptions {
        offset_x: offset_x.unwrap_or(0),
        offset_z: offset_z.unwrap_or(0),
        overwrite: overwrite.unwrap_or(true),
        y_offset: y_offset.unwrap_or(0),
        sections,
    };
    tuffbox_core::region::import_world_chunks(
        &world_dir,
        std::path::Path::new(&source_dir),
        &source_sels,
        source_dimension.as_deref(),
        dimension.as_deref(),
        &opts,
        target_sels.as_deref(),
    )
    .map_err(|e| format!("Failed to import chunks: {}", e))
}

/// Render world map (or selection) to a PNG file on disk.
#[tauri::command(rename_all = "camelCase")]
fn render_world_map_png(
    path: String,
    world_name: String,
    dest_path: String,
    color_mode: Option<String>,
    scale: Option<u32>,
    selections: Option<Vec<ChunkSelection>>,
    dimension: Option<String>,
) -> Result<(u32, u32), String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let pairs: Vec<(i32, i32, Vec<usize>)> = selections
        .unwrap_or_default()
        .into_iter()
        .map(|s| (s.region_x, s.region_z, s.indices))
        .collect();
    let mode = tuffbox_core::region::MapColorMode::parse(color_mode.as_deref().unwrap_or("status"));
    tuffbox_core::region::render_world_map_png(
        &world_dir,
        dimension.as_deref(),
        &pairs,
        mode,
        scale.unwrap_or(4),
        std::path::Path::new(&dest_path),
    )
    .map_err(|e| format!("Failed to render map PNG: {}", e))
}

/// Select chunks by MCA-style map filter query.
#[tauri::command(rename_all = "camelCase")]
fn select_world_by_query(
    path: String,
    world_name: String,
    query: String,
    dimension: Option<String>,
) -> Result<Vec<tuffbox_core::region_edit::ChunkRef>, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    tuffbox_core::region_edit::select_world_by_query(&world_dir, &query, dimension.as_deref())
        .map_err(|e| format!("Failed to select by query: {}", e))
}

/// Warm world map region metadata cache.
#[tauri::command(rename_all = "camelCase")]
fn warm_world_map_cache(
    path: String,
    world_name: String,
    dimension: Option<String>,
) -> Result<usize, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    tuffbox_core::region::warm_world_map_cache(&world_dir, dimension.as_deref())
        .map_err(|e| format!("Failed to warm map cache: {}", e))
}

/// Clear world map region metadata cache.
#[tauri::command(rename_all = "camelCase")]
fn clear_world_map_cache(
    path: String,
    world_name: String,
    dimension: Option<String>,
) -> Result<usize, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    tuffbox_core::region::clear_world_map_cache(&world_dir, dimension.as_deref())
        .map_err(|e| format!("Failed to clear map cache: {}", e))
}

/// Swaps exactly two chunks (useful for repair after corruption).
#[tauri::command(rename_all = "camelCase")]
fn swap_world_chunks(
    path: String,
    world_name: String,
    a: ChunkSelection,
    b: ChunkSelection,
    dimension: Option<String>,
) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let ai = *a.indices.first().ok_or("chunk A needs one index")?;
    let bi = *b.indices.first().ok_or("chunk B needs one index")?;
    tuffbox_core::region::swap_world_chunks(
        &world_dir,
        (a.region_x, a.region_z, ai),
        (b.region_x, b.region_z, bi),
        dimension.as_deref(),
    )
    .map_err(|e| format!("Failed to swap chunks: {}", e))
}

/// Bulk NBT Changer (MCA Selector).
#[tauri::command(rename_all = "camelCase")]
fn change_world_chunks(
    path: String,
    world_name: String,
    selections: Vec<ChunkSelection>,
    change: tuffbox_core::region_edit::NbtChangeRequest,
    dimension: Option<String>,
) -> Result<usize, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let pairs: Vec<(i32, i32, Vec<usize>)> = selections
        .into_iter()
        .map(|s| (s.region_x, s.region_z, s.indices))
        .collect();
    tuffbox_core::region_edit::change_world_chunks(
        &world_dir,
        &pairs,
        &change,
        dimension.as_deref(),
    )
    .map_err(|e| format!("Failed to change chunks: {}", e))
}

/// Read one chunk as an NBT tree for the Chunk Editor.
#[tauri::command(rename_all = "camelCase")]
fn read_chunk_editor(
    path: String,
    world_name: String,
    region_x: i32,
    region_z: i32,
    index: usize,
    dimension: Option<String>,
    layer: Option<String>,
) -> Result<tuffbox_core::region_edit::ChunkEditorData, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    tuffbox_core::region_edit::read_chunk_editor(
        &world_dir,
        region_x,
        region_z,
        index,
        dimension.as_deref(),
        layer.as_deref(),
    )
    .map_err(|e| format!("Failed to read chunk: {}", e))
}

/// Write edited NBT tree back to disk.
#[tauri::command(rename_all = "camelCase")]
fn write_chunk_editor(
    path: String,
    world_name: String,
    data: tuffbox_core::region_edit::ChunkEditorData,
    dimension: Option<String>,
) -> Result<(), String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    tuffbox_core::region_edit::write_chunk_editor(&world_dir, &data, dimension.as_deref())
        .map_err(|e| format!("Failed to write chunk: {}", e))
}

/// Advanced content filter (palette / entities / structures).
#[tauri::command(rename_all = "camelCase")]
fn filter_world_chunks_advanced(
    path: String,
    world_name: String,
    filter: tuffbox_core::region_edit::AdvancedChunkFilter,
    selections: Option<Vec<ChunkSelection>>,
    dimension: Option<String>,
) -> Result<Vec<tuffbox_core::region_edit::ChunkRef>, String> {
    let project_dir = manifest_parent(&path)?;
    let world_dir = project_dir.join("saves").join(&world_name);
    let pairs: Vec<(i32, i32, Vec<usize>)> = selections
        .unwrap_or_default()
        .into_iter()
        .map(|s| (s.region_x, s.region_z, s.indices))
        .collect();
    tuffbox_core::region_edit::filter_world_chunks_advanced(
        &world_dir,
        &pairs,
        &filter,
        dimension.as_deref(),
    )
    .map_err(|e| format!("Failed to filter chunks: {}", e))
}

/// ── Export to GitHub Releases ──────────────────────────────────

/// Generates GitHub Release-compatible changelog and asset manifest.
#[tauri::command(rename_all = "camelCase")]
fn generate_github_release(
    path: String,
    tag: Option<String>,
    target: Option<String>,
) -> Result<serde_json::Value, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let version = tag.unwrap_or_else(|| manifest.project.version.clone());
    let tag_name = format!("v{}", version);
    let changelog = format!(
        "# {} {}\n\n{}",
        manifest.project.name,
        manifest.project.version,
        manifest.project.description.as_deref().unwrap_or("")
    );

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
        manifest.project.name,
        version,
        manifest.minecraft.version,
        format!("{:?}", manifest.loader.kind).to_lowercase(),
        manifest.loader.version,
        changelog,
        manifest.mods.len(),
        manifest
            .mods
            .iter()
            .map(|m| format!("- {} {}", m.name, m.version))
            .take(50)
            .collect::<Vec<_>>()
            .join("\n")
    );

    let release_dir = if let Some(t) = target {
        std::path::PathBuf::from(&t)
    } else {
        project_dir.join("release")
    };
    std::fs::create_dir_all(&release_dir).map_err(|e| e.to_string())?;
    let release_json = release_dir.join("github-release.json");
    let payload = serde_json::json!({
        "tag_name": tag_name, "name": format!("{} {}", manifest.project.name, version),
        "body": body, "draft": true, "prerelease": version.contains("alpha") || version.contains("beta"),
        "artifacts": artifacts,
    });
    std::fs::write(
        &release_json,
        serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "tagName": tag_name, "body": body,
        "releaseJsonPath": release_json.to_string_lossy().to_string(),
        "artifacts": artifacts, "artifactCount": artifacts.len(),
    }))
}

/// ── Localization helper ──────────────────────────────────────────

static L10N: once_cell::sync::Lazy<std::collections::HashMap<&str, &str>> =
    once_cell::sync::Lazy::new(|| {
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
    Ok(L10N
        .iter()
        .map(|(k, v)| serde_json::json!({"key": k, "ru": v.split(" / ").next().unwrap_or(v)}))
        .collect())
}

/// ── Batch operations for CLI/scripting ────────────────────────────

/// Exports the dependency graph as a DOT string (Graphviz format),
/// which can be rendered to PNG/SVG with the `dot` command.
#[tauri::command(rename_all = "camelCase")]
fn export_graph_dot(path: String) -> Result<String, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let graph = DependencyGraph::from_manifest(&manifest);
    let mut dot = String::from(
        "digraph TuffBox {
",
    );
    dot.push_str(
        "  rankdir=LR;
  node [shape=box, style=filled, fillcolor=\"#18181b\", fontcolor=\"#e5e7eb\", color=\"#27272a\"];
",
    );
    dot.push_str(
        "  edge [color=\"#3f3f46\", fontcolor=\"#71717a\"];

",
    );

    for node in &graph.nodes {
        let color = match node.kind {
            tuffbox_core::graph::NodeKind::Mod => "#1bd96a22",
            tuffbox_core::graph::NodeKind::Profile => "#8b5cf622",
            _ => "#f59e0b22",
        };
        let shape = if node.kind == tuffbox_core::graph::NodeKind::Profile {
            "ellipse"
        } else {
            "box"
        };
        dot.push_str(&format!(
            "  \"{}\" [label=\"{}\", shape={}, fillcolor=\"{}\", color=\"{}\"];
",
            node.id.0,
            node.label,
            shape,
            color,
            if color.len() > 9 { &color[..7] } else { color }
        ));
    }

    for edge in &graph.edges {
        let style = if edge.kind == tuffbox_core::graph::EdgeKind::Requires {
            "solid"
        } else if edge.kind == tuffbox_core::graph::EdgeKind::Conflicts {
            "dashed, color=\"#ef4444\""
        } else {
            "dotted"
        };
        dot.push_str(&format!(
            "  \"{}\" -> \"{}\" [label=\"{:?}\", style={}];
",
            edge.from.0, edge.to.0, edge.kind, style
        ));
    }

    dot.push_str(
        "}
",
    );
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

    type ExportFn = Box<
        dyn Fn(
            &ProjectManifest,
            &std::path::Path,
            &std::path::Path,
        ) -> Result<tuffbox_core::ExportResult, tuffbox_core::ExportError>,
    >;
    let exports: Vec<(&str, ExportFn)> = vec![
        (
            "mrpack",
            Box::new(|m, p, o| tuffbox_core::exporter::export_modrinth_pack(m, p, o)),
        ),
        (
            "server-pack",
            Box::new(|m, p, o| tuffbox_core::exporter::export_server_pack(m, p, o)),
        ),
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
fn get_graph(path: String) -> Result<serde_json::Value, String> {
    let manifest_path = PathBuf::from(&path);
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    match tuffbox_core::GraphCache::load_if_current(&manifest_path, &manifest) {
        Ok(Some(cache)) => Ok(graph_payload(
            cache.graph,
            "cache",
            Some(cache.generated_at),
        )),
        _ => {
            let mut local = manifest;
            tuffbox_core::enrich_manifest_from_installed_jars(&manifest_path, &mut local);
            Ok(graph_payload(
                DependencyGraph::from_manifest(&local),
                "local",
                None,
            ))
        }
    }
}

#[tauri::command(rename_all = "camelCase")]
async fn refresh_graph(app: tauri::AppHandle, path: String) -> Result<serde_json::Value, String> {
    use tauri::Emitter;

    let _ = app.emit(
        "graph-refresh-progress",
        serde_json::json!({"phase": "start", "message": "Refreshing dependency metadata"}),
    );
    let app_done = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let base = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let mut enriched = base.clone();
        tuffbox_core::enrich_manifest_from_installed_jars(&manifest_path, &mut enriched);
        enrich_manifest_for_graph(&mut enriched)?;
        let cache = tuffbox_core::GraphCache::new(&base, enriched);
        cache.save(&manifest_path)?;
        Ok::<_, String>(graph_payload(
            cache.graph,
            "network",
            Some(cache.generated_at),
        ))
    })
    .await
    .map_err(|e| e.to_string())?;

    let _ = app_done.emit(
        "graph-refresh-progress",
        match &result {
            Ok(_) => serde_json::json!({"phase": "done", "message": "Dependency graph is current"}),
            Err(error) => serde_json::json!({"phase": "error", "message": error}),
        },
    );
    result
}

fn graph_payload(
    graph: DependencyGraph,
    source: &str,
    generated_at: Option<String>,
) -> serde_json::Value {
    serde_json::json!({
        "nodes": graph.nodes,
        "edges": graph.edges,
        "source": source,
        "generatedAt": generated_at,
    })
}

fn manifest_for_graph(path: &str) -> Result<ProjectManifest, String> {
    let manifest_path = PathBuf::from(path);
    let manifest = ProjectManifest::load_from_path(path).map_err(|e| e.to_string())?;
    Ok(
        tuffbox_core::GraphCache::load_if_current(&manifest_path, &manifest)
            .ok()
            .flatten()
            .map(|cache| cache.enriched_manifest)
            .unwrap_or_else(|| {
                let mut local = manifest;
                tuffbox_core::enrich_manifest_from_installed_jars(&manifest_path, &mut local);
                local
            }),
    )
}

/// Fills Modrinth dependency edges and icon URLs in-memory so the graph view
/// shows real mod-to-mod links. Always refreshes dependency lists from Modrinth
/// (project id → slug normalized) so edges resolve onto installed mod nodes.
fn enrich_manifest_for_graph(manifest: &mut ProjectManifest) -> Result<(), String> {
    use rayon::prelude::*;

    let query = tuffbox_core::ProviderSearchQuery {
        query: None,
        minecraft_version: Some(manifest.minecraft.version.clone()),
        loader: Some(tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string()),
        ..Default::default()
    };

    manifest.mods.par_iter_mut().for_each(|module| {
        if !matches!(
            module.source.kind,
            tuffbox_core::manifest::SourceKind::Modrinth
        ) {
            return;
        }
        let provider = tuffbox_core::ModrinthProvider::new();
        let project_id = module
            .source
            .project_id
            .clone()
            .unwrap_or_else(|| module.id.clone());

        let version_id = if let Some(file_id) = module.source.file_id.clone() {
            Some(file_id)
        } else if let Ok(versions) = provider.get_versions(&project_id, &query) {
            versions.into_iter().next().map(|v| v.id)
        } else {
            None
        };

        if let Some(version_id) = version_id {
            if let Ok(deps) = provider.resolve_dependencies(&version_id) {
                module.dependencies = deps;
            }
        }

        // Fetch the project once to backfill both the icon and the site
        // categories (Modrinth tags). Categories drive the graph clustering,
        // so we refresh them even when the icon is already cached.
        if module.source.icon_url.is_none() || module.source.categories.is_empty() {
            if let Ok(project) = provider.get_project(&project_id) {
                if module.source.icon_url.is_none() {
                    module.source.icon_url = project.icon_url;
                }
                if !project.categories.is_empty() {
                    module.source.categories = project.categories;
                }
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn get_diagnostics(path: String) -> Result<Vec<tuffbox_core::Diagnostic>, String> {
    let manifest = manifest_for_graph(&path)?;
    let graph = DependencyGraph::from_manifest(&manifest);
    Ok(Resolver::analyze_project(&manifest, &graph))
}

#[tauri::command(rename_all = "camelCase")]
fn get_resolve_change_plan(path: String) -> Result<Option<tuffbox_core::ChangePlan>, String> {
    let manifest = manifest_for_graph(&path)?;
    let graph = DependencyGraph::from_manifest(&manifest);
    let diagnostics = Resolver::analyze_project(&manifest, &graph);
    Ok(Resolver::create_fix_plan(&graph, &diagnostics))
}

#[tauri::command(rename_all = "camelCase")]
async fn apply_resolve_action(
    app: tauri::AppHandle,
    path: String,
    action_index: usize,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let mut manifest = manifest_for_graph(&path)?;
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
        apply_change_action(&manifest_path, &mut manifest, action, &mut applied)?;
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods_tracked(&app, &manifest_path, &manifest, None, true);
        Ok(applied)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn apply_resolve_change_plan(
    app: tauri::AppHandle,
    path: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let mut manifest = manifest_for_graph(&path)?;
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
            apply_change_action(&manifest_path, &mut manifest, action, &mut applied)?;
        }
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods_tracked(&app, &manifest_path, &manifest, None, true);
        Ok(applied)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
async fn resolve_missing_dependencies(
    app: tauri::AppHandle,
    path: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        // Use the same cached dependency edges the graph view shows.
        let mut manifest = manifest_for_graph(&path)?;
        let existing_ids = manifest
            .mods
            .iter()
            .map(|module| module.id.clone())
            .collect::<std::collections::HashSet<_>>();
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
        let installed_ids = manifest
            .mods
            .iter()
            .filter(|module| !existing_ids.contains(&module.id))
            .map(|module| module.id.clone())
            .collect::<std::collections::HashSet<_>>();
        download_project_mods_tracked(&app, &manifest_path, &manifest, Some(&installed_ids), true);
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Installs a single missing dependency from the graph. The `mod_id` can be
/// either a Modrinth project ID (e.g. "AANobbMI") or a slug (e.g. "malilib").
/// Used by the graph "Install" button on ghost/missing nodes.
#[tauri::command(rename_all = "camelCase")]
async fn install_graph_dep(
    app: tauri::AppHandle,
    path: String,
    mod_id: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        // Skip if already installed (by slug or project_id)
        if manifest
            .mods
            .iter()
            .any(|m| m.id == mod_id || m.source.project_id.as_deref() == Some(mod_id.as_str()))
        {
            return Ok(Vec::new());
        }
        let existing_ids = manifest
            .mods
            .iter()
            .map(|module| module.id.clone())
            .collect::<std::collections::HashSet<_>>();
        auto_snapshot(&manifest_path, "install-graph-dep").map_err(|e| e.to_string())?;
        // Recursive: install the dep + all its transitive dependencies
        let installed = install_modrinth_with_dependencies(&mut manifest, &[mod_id], "auto");
        if installed.is_empty() {
            return Err(format!(
                "Failed to install dependency: not found on Modrinth or already installed"
            ));
        }
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        let installed_ids = manifest
            .mods
            .iter()
            .filter(|module| !existing_ids.contains(&module.id))
            .map(|module| module.id.clone())
            .collect::<std::collections::HashSet<_>>();
        download_project_mods_tracked(&app, &manifest_path, &manifest, Some(&installed_ids), true);
        Ok(installed)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Downloads files for mods that are in the manifest but whose jar/resource
/// file is missing from disk. Returns the list of mod IDs that were
/// successfully downloaded.
#[tauri::command(rename_all = "camelCase")]
async fn download_missing_files(
    app: tauri::AppHandle,
    path: String,
) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let report = download_project_mods_tracked(&app, &manifest_path, &manifest, None, true);
        Ok(report.downloaded)
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
    let mut diagnosis = tuffbox_core::crash::build_crash_diagnosis(
        &project_dir,
        &manifest,
        report_id.as_deref(),
        snapshots,
    )
    .map_err(|e| e.to_string())?;

    // Merge Crash Assistant log-phrase findings into hints so each detect
    // gets one-by-one FixAction buttons in the Problems / Recommended panels.
    // Skip when the live session is healthy — those detectors often match
    // leftover ERROR lines from a previously fixed crash.
    if !diagnosis.session_healthy {
        if let Ok(assistant) =
            run_crash_assistant_analysis(&path, &manifest, &project_dir, report_id.as_deref())
        {
            for finding in assistant.findings {
                let id = format!("ca:{}", finding.code);
                if diagnosis.hints.iter().any(|h| h.id == id) {
                    continue;
                }
                let mut detail = finding.description.clone();
                if let Some(ev) = finding.evidence.as_ref() {
                    detail.push_str("\n\nLog evidence:\n");
                    detail.push_str(ev);
                }
                let steps = finding
                    .auto_fix
                    .clone()
                    .into_iter()
                    .collect::<Vec<_>>();
                let related: Vec<String> = finding
                    .fixes
                    .iter()
                    .filter_map(|f| f.mod_id.clone())
                    .collect();
                let fix = finding.fixes.first().cloned();
                diagnosis.hints.push(tuffbox_core::crash::DiagnosisHint {
                    id,
                    title: finding.title,
                    severity: finding.severity,
                    detail,
                    steps,
                    related_mods: related,
                    fix,
                    fixes: finding.fixes,
                });
            }
        }
    }

    Ok(diagnosis)
}

fn run_crash_assistant_analysis(
    path: &str,
    manifest: &ProjectManifest,
    project_dir: &Path,
    report_id: Option<&str>,
) -> Result<tuffbox_core::crash_assistant::CrashAnalysisReport, String> {
    // Scope: selected crash report (or newest) + latest.log + current mods.
    // Do not dump every historical crash-report into the analyzer.
    // If latest.log is newer than the crash report (successful relaunch), skip
    // the stale crash text unless the user explicitly selected that report.
    let installed: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    let mut latest_log = String::new();
    let lp = project_dir.join("logs").join("latest.log");
    if lp.is_file() {
        latest_log = tuffbox_core::process::read_log_tail(&lp, 2000).unwrap_or_default();
    }

    let explicit = report_id.filter(|id| !id.is_empty());
    let mut crash_content = Vec::new();
    if let Some(id) = explicit {
        if let Some(text) = load_scoped_crash_report(project_dir, Some(id)) {
            crash_content.push(text);
        }
    } else if let Some((report_path, text)) = load_newest_crash_report(project_dir) {
        let stale = tuffbox_core::crash::latest_log_supersedes_crash(
            project_dir,
            Some(report_path.as_path()),
            &latest_log,
        );
        if !stale {
            crash_content.push(text);
        }
    }

    let mut launcher_log = String::new();
    let la = project_dir.join("logs").join("launcher.log");
    if la.is_file() {
        launcher_log = tuffbox_core::process::read_log_tail(&la, 400).unwrap_or_default();
    }

    let jv = manifest
        .java
        .as_ref()
        .and_then(|j| j.path.clone())
        .unwrap_or_default();
    let java_version = if !jv.is_empty() {
        tuffbox_core::jre::check_java_at_path(&PathBuf::from(&jv))
            .map(|r| r.version)
            .unwrap_or_default()
    } else {
        String::new()
    };

    let ctx = tuffbox_core::crash_assistant::AnalysisCtx {
        crash_content,
        latest_log,
        launcher_log,
        installed_mods: installed,
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
    let _ = path;
    Ok(tuffbox_core::crash_assistant::run_full_analysis(&ctx))
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
async fn apply_crash_fix_plan(
    path: String,
    report_id: Option<String>,
) -> Result<Vec<String>, String> {
    let result = tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let project_dir = manifest_parent(&path)?;
        let diagnosis = get_crash_diagnosis(path.clone(), report_id.clone())?;
        let plan = diagnosis.fix_plan;

        if plan.actions.is_empty() {
            return Ok((path, Vec::new()));
        }

        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let loader = format!("{:?}", manifest.loader.kind).to_lowercase();
        let crash = load_scoped_crash_report(&project_dir, report_id.as_deref()).unwrap_or_default();
        let fingerprint = tuffbox_core::crash_kb::fingerprint_from_text(
            &crash,
            &manifest.minecraft.version,
            &loader,
        );

        let launcher_actions = swarm_api::change_actions_to_launcher(&plan.actions);

        if plan.requires_snapshot {
            swarm_api::auto_snapshot_crash_fix_heuristic(
                &manifest_path,
                Some(fingerprint.key.as_str()),
                &plan.summary,
                report_id.as_deref(),
                launcher_actions.clone(),
            )?;
        }

        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let mut applied = Vec::new();
        for action in plan.actions {
            apply_change_action(&manifest_path, &mut manifest, action, &mut applied)?;
        }
        save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
        download_project_mods(&manifest_path, &manifest);

        let explanation = if applied.is_empty() {
            plan.summary.clone()
        } else {
            format!("Applied fix plan: {}", applied.join("; "))
        };
        let _ = swarm_api::record_user_fix_attempt(
            &manifest_path,
            "crash_assistant",
            &explanation,
            launcher_actions,
            Some(fingerprint.key.as_str()),
        );

        let _ = swarm_api::record_project_cooccurrence(path.clone());
        Ok::<_, String>((path, applied))
    })
    .await
    .map_err(|e| e.to_string())??;
    let (path, applied) = result;
    // Best-effort Supabase upload (local already recorded above).
    let _ = swarm_api::record_and_upload_cooccurrence_opts(
        &path,
        &[],
        "crash_assistant_fix",
        false,
    )
    .await;
    Ok(applied)
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
fn update_history_settings(
    path: String,
    settings: HistorySettings,
) -> Result<HistorySettings, String> {
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

    // Resolved crash fixes (successful relaunch after apply).
    let mut seen_resolution_keys = std::collections::HashSet::new();
    if let Ok(resolutions) = swarm_api::list_crash_resolutions(&project_dir) {
        for rec in resolutions {
            seen_resolution_keys.insert(rec.fingerprint_key.clone());
            let how = if rec.actions_summary.is_empty() {
                rec.human_explanation.clone()
            } else {
                rec.actions_summary.join("; ")
            };
            let summary = if rec.actions_summary.is_empty() {
                rec.human_explanation.clone()
            } else {
                format!(
                    "{}\nHow: {}",
                    rec.human_explanation,
                    rec.actions_summary.join(", ")
                )
            };
            entries.push(ProjectChangeEntry {
                id: rec.id.clone(),
                snapshot_id: rec.snapshot_id.clone(),
                operation: "Crash resolved".to_string(),
                reason: format!("Verified by {} · {}", rec.verified_by, how),
                created_at: rec.resolved_at.clone(),
                path: format!("crash://{}", rec.fingerprint_key),
                category: "Resolutions".to_string(),
                kind: "crash_resolved".to_string(),
                preview: tuffbox_core::crash_kb::truncate_at_char_boundary(&summary, 240)
                    .to_string(),
                diff: summary,
                can_open: false,
                tags: vec!["crash_resolved".into(), "crash_fix".into()],
                crash_fingerprint_key: Some(rec.fingerprint_key),
                plan_source: rec.plan_source,
            });
        }
    }

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

        // Explicit card for crash_resolved snapshots even without file diffs
        // (skip if already covered by resolutions.jsonl).
        if snapshot.tags.iter().any(|t| t == "crash_resolved")
            && snapshot.changed_files.is_empty()
            && snapshot
                .crash_fingerprint_key
                .as_ref()
                .map(|k| !seen_resolution_keys.contains(k))
                .unwrap_or(true)
        {
            entries.push(ProjectChangeEntry {
                id: format!("{}:crash-resolved", snapshot.id),
                snapshot_id: snapshot.id.clone(),
                operation: snapshot.name.clone(),
                reason: snapshot.reason.clone(),
                created_at: snapshot.created_at.clone(),
                path: "crash-resolution".to_string(),
                category: "Resolutions".to_string(),
                kind: "crash_resolved".to_string(),
                preview: snapshot.reason.clone(),
                diff: snapshot.reason.clone(),
                can_open: false,
                tags: snapshot.tags.clone(),
                crash_fingerprint_key: snapshot.crash_fingerprint_key.clone(),
                plan_source: snapshot.plan_source.clone(),
            });
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
                tags: snapshot.tags.clone(),
                crash_fingerprint_key: snapshot.crash_fingerprint_key.clone(),
                plan_source: snapshot.plan_source.clone(),
            });
        }
    }

    entries.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(entries)
}

#[tauri::command(rename_all = "camelCase")]
fn read_project_history_file(
    path: String,
    relative_path: String,
) -> Result<HistoryFileContent, String> {
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
fn create_tracked_history_snapshot(
    path: String,
    roots: Vec<String>,
) -> Result<tuffbox_core::Snapshot, String> {
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
                    collect_tracked_project_files(&project_dir, &dir, &mut changed_files)
                        .map_err(|e| e.to_string())?;
                }
            }
        }
    }
    changed_files.sort();
    changed_files.dedup();
    auto_snapshot_with_changed_files(&manifest_path, "track-history", &changed_files)
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn rollback_history_file(
    path: String,
    snapshot_id: String,
    relative_path: String,
) -> Result<(), String> {
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
    let canonical_parent =
        std::fs::canonicalize(dst.parent().unwrap_or(&project_dir)).map_err(|e| e.to_string())?;
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
fn diff_snapshots(
    project_dir: String,
    from: String,
    to: String,
) -> Result<tuffbox_core::SnapshotDiff, String> {
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
    let base = PathBuf::from(project_dir)
        .join(".tuffbox")
        .join("snapshots");
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
    out.push_str(&format!(
        "# {} {}\n\n",
        manifest.project.name, manifest.project.version
    ));
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
    out.push_str(&format!(
        "- Loader: {:?} {}\n",
        manifest.loader.kind, manifest.loader.version
    ));
    out.push_str(&format!("- Mods: {}\n\n", manifest.mods.len()));
    out.push_str("## Included mods\n\n");
    for module in &manifest.mods {
        out.push_str(&format!(
            "- {} `{}` ({:?})\n",
            module.name, module.version, module.side
        ));
    }
    out.push_str("\n## Diagnostics\n\n");
    if diagnostics.is_empty() {
        out.push_str("- No current diagnostics.\n");
    } else {
        for diagnostic in diagnostics {
            out.push_str(&format!(
                "- {:?}: {} — {}\n",
                diagnostic.severity, diagnostic.code, diagnostic.message
            ));
        }
    }
    out.push_str("\n## Recent snapshots\n\n");
    for snapshot in snapshots.iter().rev().take(5) {
        out.push_str(&format!(
            "- {} — {} ({})\n",
            snapshot.created_at, snapshot.name, snapshot.reason
        ));
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
fn create_release_snapshot(
    path: String,
    changelog: String,
) -> Result<ReleaseSnapshotResult, String> {
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
fn export_modrinth_pack(
    path: String,
    target_path: Option<String>,
) -> Result<tuffbox_core::ExportResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let output = target_path.map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(&path)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!(
                "{}-{}.mrpack",
                manifest.project.id, manifest.project.version
            ))
    });
    let result =
        tuffbox_core::export_modrinth_pack(&manifest, &path, &output).map_err(|e| e.to_string())?;
    append_release_artifact(&path, "mrpack", &result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command(rename_all = "camelCase")]
fn export_server_pack(
    path: String,
    target_path: Option<String>,
) -> Result<tuffbox_core::ExportResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let output = target_path.map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(&path)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!(
                "{}-{}-server.zip",
                manifest.project.id, manifest.project.version
            ))
    });
    let result =
        tuffbox_core::export_server_pack(&manifest, &path, &output).map_err(|e| e.to_string())?;
    append_release_artifact(&path, "server", &result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command(rename_all = "camelCase")]
fn export_prism_instance(
    path: String,
    target_path: Option<String>,
) -> Result<tuffbox_core::ExportResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let output = target_path.map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(&path)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!(
                "{}-{}-prism.zip",
                manifest.project.id, manifest.project.version
            ))
    });
    let result = tuffbox_core::export_prism_instance(&manifest, &path, &output)
        .map_err(|e| e.to_string())?;
    append_release_artifact(&path, "prism", &result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[tauri::command(rename_all = "camelCase")]
fn export_curseforge_pack(
    path: String,
    target_path: Option<String>,
) -> Result<tuffbox_core::ExportResult, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let output = target_path.map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(&path)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!(
                "{}-{}-curseforge.zip",
                manifest.project.id, manifest.project.version
            ))
    });
    let result = tuffbox_core::export_curseforge_pack(&manifest, &path, &output)
        .map_err(|e| e.to_string())?;
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
    markdown.push_str(&format!(
        "# {} {} release draft

",
        manifest.project.name, manifest.project.version
    ));
    markdown.push_str(
        "## Changelog

",
    );
    markdown.push_str(changelog.trim());
    markdown.push_str(
        "

## Artifacts

",
    );
    if artifacts.is_empty() {
        markdown.push_str(
            "- No artifacts exported yet.
",
        );
    } else {
        for artifact in &artifacts {
            markdown.push_str(&format!(
                "- **{}**: `{}` ({} files, {} overrides)
",
                artifact.kind, artifact.path, artifact.file_count, artifact.override_count
            ));
        }
    }
    markdown.push_str(
        "
## Publish checklist

",
    );
    markdown.push_str(
        "- [ ] Upload artifacts to target platform
",
    );
    markdown.push_str(
        "- [ ] Verify game/loader versions
",
    );
    markdown.push_str(
        "- [ ] Verify server pack starts
",
    );
    markdown.push_str(
        "- [ ] Announce known issues
",
    );
    std::fs::write(&draft_path, markdown).map_err(|e| e.to_string())?;

    let artifact_count = artifacts.len();
    let publish_config = integrations::get_publish_config(path.clone()).unwrap_or_default();
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
            "modrinth": {
                "configured": !publish_config.modrinth_project_id.is_empty(),
                "projectId": publish_config.modrinth_project_id,
            },
            "curseforge": {
                "configured": !publish_config.curseforge_project_id.is_empty(),
                "projectId": publish_config.curseforge_project_id,
                "gameVersionIds": publish_config.curseforge_game_version_ids,
            },
            "githubReleases": {
                "configured": !publish_config.github_repository.is_empty(),
                "repository": publish_config.github_repository,
            }
        }
    });
    std::fs::write(
        &metadata_path,
        serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?,
    )
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
        (
            project_dir.join("logs").join("launcher.log"),
            "logs-launcher.log",
        ),
        (
            project_dir.join("logs").join("launcher_log.txt"),
            "logs-launcher_log.txt",
        ),
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
            if entries
                .filter_map(|e| e.ok())
                .any(|e| e.path().extension().map_or(false, |x| x == "txt"))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[tauri::command(rename_all = "camelCase")]
async fn launch_with_quick_play(
    app: tauri::AppHandle,
    path: String,
    profile: String,
    _quick_play_type: Option<String>,
    _quick_play_value: Option<String>,
) -> Result<tuffbox_core::LaunchResult, LaunchErrorInfo> {
    launch_profile(app, path, profile).await
}

#[tauri::command(rename_all = "camelCase")]
async fn launch_profile(
    app: tauri::AppHandle,
    path: String,
    profile: String,
) -> Result<tuffbox_core::LaunchResult, LaunchErrorInfo> {
    let path = resolve_manifest_path(&path).map_err(|e| {
        LaunchErrorInfo::new(LaunchErrorKind::Install, e)
    })?
    .to_string_lossy()
    .to_string();

    let project_dir = PathBuf::from(&path)
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| {
            LaunchErrorInfo::new(
                LaunchErrorKind::Unknown,
                "manifest has no parent directory",
            )
        })?;
    let logs_dir = project_dir.join("logs");
    // Minecraft (log4j) owns `latest.log`. We must NOT truncate it — that wiped
    // real crash evidence and raced the game writer. Console capture goes to a
    // separate TuffBox file; diagnose still reads `logs/latest.log`.
    let console_log = logs_dir.join("tuffbox-console.log");
    let latest_log = logs_dir.join("latest.log");

    {
        use std::io::Write;
        std::fs::create_dir_all(&logs_dir).map_err(|e| {
            LaunchErrorInfo::new(LaunchErrorKind::Unknown, e.to_string())
        })?;
        let mut console = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&console_log)
            .map_err(|e| LaunchErrorInfo::new(LaunchErrorKind::Unknown, e.to_string()))?;
        writeln!(console, "# TuffBox launching profile {profile}").ok();
        let launcher_log = project_dir.join("launcher_log.txt");
        let mut launcher = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&launcher_log)
            .map_err(|e| LaunchErrorInfo::new(LaunchErrorKind::Unknown, e.to_string()))?;
        writeln!(launcher, "# TuffBox launching profile {profile}").ok();
    }

    append_test_run_record(&path, &profile, &latest_log).map_err(|e| {
        LaunchErrorInfo::new(LaunchErrorKind::Unknown, e.to_string())
    })?;

    let console_log_clone = console_log.clone();
    let latest_log_clone = latest_log.clone();
    // Run the (blocking) install + spawn on a blocking thread, then await the
    // result so install/prepare failures surface to the UI as a structured,
    // categorized error instead of being swallowed into the log file.
    let result = tokio::task::spawn_blocking(move || {
        build_and_spawn(path, profile, console_log_clone, latest_log_clone, app)
    })
    .await
    .map_err(|e| {
        LaunchErrorInfo::new(
            LaunchErrorKind::Unknown,
            format!("launch task panicked: {e}"),
        )
        .with_log(&latest_log)
    })?;

    match result {
        Ok(()) => Ok(tuffbox_core::LaunchResult {
            exit_code: None,
            log_path: latest_log,
        }),
        Err(info) => Err(info),
    }
}

fn build_and_spawn(
    path: String,
    profile: String,
    console_log: PathBuf,
    latest_log: PathBuf,
    app: tauri::AppHandle,
) -> Result<(), LaunchErrorInfo> {
    let _instance_id = profile.clone();
    use tuffbox_core::{LaunchOptions, TestLauncher};

    let manifest_path = resolve_manifest_path(&path).map_err(|e| {
        LaunchErrorInfo::new(LaunchErrorKind::Install, e).with_log(&console_log)
    })?;
    let path = manifest_path.to_string_lossy().to_string();

    let manifest = ProjectManifest::load_from_path(&manifest_path).map_err(|e| {
        LaunchErrorInfo::new(LaunchErrorKind::Install, e.to_string()).with_log(&console_log)
    })?;
    let project_profile = manifest
        .profiles
        .iter()
        .find(|p| p.id == profile)
        .ok_or_else(|| {
            LaunchErrorInfo::new(LaunchErrorKind::Install, format!("profile {profile} not found"))
                .with_log(&console_log)
        })?
        .clone();

    let launch_settings = launcher_settings::load_launcher_settings();

    let java_path = manifest
        .java
        .as_ref()
        .and_then(|j| j.path.clone())
        .or_else(|| {
            launch_settings
                .default_java_path
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        });
    let java = if let Some(java_path) = java_path {
        tuffbox_core::jre::check_java_at_path(&PathBuf::from(&java_path)).map_err(|e| {
            LaunchErrorInfo::new(LaunchErrorKind::JavaMissing, e.to_string()).with_log(&console_log)
        })?
    } else {
        // Auto-detect the best Java for this Minecraft version instead of
        // always grabbing whatever JVM happens to be newest on the system
        // — using e.g. Java 21 for Forge 1.20.1 (which needs Java 17)
        // fails deep inside Forge's bootstrap launcher with a confusing
        // module-system error instead of launching at all.
        TestLauncher::find_java_for_minecraft(&manifest.minecraft.version).map_err(|e| {
            let kind = match e {
                tuffbox_core::launcher::LauncherError::JavaNotFound => LaunchErrorKind::JavaMissing,
                _ => LaunchErrorKind::Install,
            };
            LaunchErrorInfo::new(kind, e.to_string()).with_log(&console_log)
        })?
    };

    let progress = tuffbox_core::mc_install::InstallProgress {
        log_path: console_log.clone(),
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
        .ok_or_else(|| {
            LaunchErrorInfo::new(LaunchErrorKind::Unknown, "manifest has no parent directory")
                .with_log(&console_log)
        })?;

    // launcher_dir = shared game data (versions, libraries, assets)
    let launcher_dir = launcher_settings::resolve_runtime_path();

    std::fs::create_dir_all(&launcher_dir).map_err(|e| {
        LaunchErrorInfo::new(LaunchErrorKind::Install, e.to_string()).with_log(&console_log)
    })?;
    std::fs::create_dir_all(&game_dir).map_err(|e| {
        LaunchErrorInfo::new(LaunchErrorKind::Install, e.to_string()).with_log(&console_log)
    })?;

    progress.log(&format!("# Game directory: {}", game_dir.display()));
    progress.log(&format!("# Launcher directory: {}", launcher_dir.display()));

    if let Err(e) = launcher_settings::run_hook(launch_settings.pre_launch_hook.as_deref(), "pre-launch hook") {
        return Err(LaunchErrorInfo::new(LaunchErrorKind::Unknown, e).with_log(&console_log));
    }

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

    let bridge = match tuffbox_core::prepare_recipe_bridge(&manifest, &game_dir) {
        Ok(bridge) => bridge,
        Err(error) => {
            progress.log(&format!("# WARNING: JEI live recipe bridge unavailable: {error}"));
            None
        }
    };
    let mut launch_jvm_args = project_profile.jvm_args.clone();
    launch_jvm_args.extend(launcher_settings::split_custom_jvm_args(
        launch_settings.java_custom_args.as_deref(),
    ));
    let cleanup_paths = if let Some(bridge) = bridge {
        progress.log("# JEI live recipe bridge enabled.");
        launch_jvm_args.extend(bridge.jvm_args);
        bridge.cleanup_paths
    } else {
        Vec::new()
    };

    // Try to load real MC access token / identity from stored auth
    let identity = auth::load_active_launch_identity();
    let (mc_token, auth_uuid, auth_user_type, auth_name) = match &identity {
        Some((uuid, name, token, user_type, _authority)) => (
            Some(token.as_str()),
            Some(uuid.as_str()),
            Some(user_type.as_str()),
            Some(name.as_str()),
        ),
        None => (None, None, None, None),
    };

    // authlib-injector for Yggdrasil accounts
    if let Some((_, _, _, _, Some(authority))) = &identity {
        if let Ok(agent) = ensure_authlib_injector_agent(authority) {
            launch_jvm_args.push(agent);
            progress.log("# authlib-injector enabled for third-party auth.");
        }
    }

    let options = LaunchOptions {
        profile_id: profile.clone(),
        instance_dir: game_dir.clone(),
        memory_mb: project_profile.memory_mb.unwrap_or(4096),
        jvm_args: launch_jvm_args,
    };

    let (mut cmd, _) = TestLauncher::build_command(
        &manifest,
        &project_profile,
        &options,
        &java,
        &launcher_dir,
        &progress,
        mc_token,
        auth_uuid,
        auth_user_type,
        auth_name,
    )
    .map_err(|e| {
        let msg = e.to_string();
        let kind = tuffbox_core::launch_error::classify_build_error_kind(&msg);
        LaunchErrorInfo::new(kind, msg).with_log(&console_log)
    })?;

    if let Some(res) = &launch_settings.game_resolution {
        cmd.arg("--width").arg(res.width.to_string());
        cmd.arg("--height").arg(res.height.to_string());
    }

    let cmd = launcher_settings::wrap_java_command(cmd, launch_settings.wrapper_command.as_deref());

    progress.log("# Starting Java process...");

    // Crash callback + playtime + Discord presence cleanup
    let crash_ctx = CrashExitCtx {
        log_path: latest_log.clone(),
        mc_version: manifest.minecraft.version.clone(),
        java_version: java.version.clone(),
        loader_kind: tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string(),
        loader_version: manifest.loader.version.clone(),
        game_dir: game_dir.clone(),
    };
    let app_for_exit = app.clone();
    let stats_path_for_exit = path.clone();
    let post_exit_hook = launch_settings.post_exit_hook.clone();
    let instance_label = manifest.project.name.clone();
    let _ = presence::set_playing_activity(&instance_label, "In Minecraft");
    let _ = record_launch(path.clone());
    let on_exit: Option<OnExit> = Some(Box::new(move |exit: ProcessExit| {
        let _ = presence::clear_activity();
        if let Some(ref hook) = post_exit_hook {
            let _ = launcher_settings::run_hook(Some(hook), "post-exit hook");
        }
        // Accumulate playtime for every session (including crashes).
        if let Ok(project_dir) = manifest_parent(&stats_path_for_exit) {
            let mut stats = load_stats(&project_dir);
            let entry = stats.instances.entry("client".into()).or_default();
            entry.total_playtime_seconds = entry
                .total_playtime_seconds
                .saturating_add(exit.duration_secs);
            let _ = save_stats(&project_dir, &stats);
        }
        if exit.code == Some(0) {
            return;
        }
        let _ = record_crash(stats_path_for_exit);
        let info = classify_crash(&crash_ctx, exit.code);
        let _ = app_for_exit.emit("launch-crashed", info);
    }));

    // Tee JVM stdout/stderr to TuffBox console log; Minecraft owns logs/latest.log.
    tuffbox_core::process::spawn_and_track_with_cleanup(profile, cmd, &console_log, cleanup_paths, on_exit)
        .map_err(|e| {
            let msg = e.to_string();
            let kind = tuffbox_core::launch_error::classify_build_error_kind(&msg);
            LaunchErrorInfo::new(kind, msg).with_log(&console_log)
        })?;

    Ok(())
}

fn ensure_authlib_injector_agent(authority: &str) -> Result<String, String> {
    let dir = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("authlib");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let jar = dir.join("authlib-injector.jar");
    if !jar.is_file() {
        // Pin a known release so launches stay reproducible offline after first fetch.
        let url = "https://github.com/yushijinhun/authlib-injector/releases/download/v1.2.5/authlib-injector-1.2.5.jar";
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        let bytes = client
            .get(url)
            .send()
            .map_err(|e| format!("authlib-injector download failed: {e}"))?
            .bytes()
            .map_err(|e| e.to_string())?;
        std::fs::write(&jar, &bytes).map_err(|e| e.to_string())?;
    }
    Ok(format!(
        "-javaagent:{}={}",
        jar.to_string_lossy().replace('\\', "/"),
        authority.trim_end_matches('/')
    ))
}

#[tauri::command(rename_all = "camelCase")]
fn get_presence_settings() -> Result<presence::PresenceSettings, String> {
    Ok(presence::load_presence_settings())
}

#[tauri::command(rename_all = "camelCase")]
fn save_presence_settings(settings: presence::PresenceSettings) -> Result<(), String> {
    presence::save_presence_settings(&settings)
}

#[tauri::command(rename_all = "camelCase")]
fn set_discord_presence(details: String, state: String) -> Result<(), String> {
    presence::set_playing_activity(&details, &state)
}

#[tauri::command(rename_all = "camelCase")]
fn clear_discord_presence() -> Result<(), String> {
    presence::clear_activity()
}

/// Context captured at launch time, used to analyze a crash when the JVM
/// exits with a non-zero code.
struct CrashExitCtx {
    log_path: PathBuf,
    mc_version: String,
    java_version: String,
    loader_kind: String,
    loader_version: String,
    game_dir: PathBuf,
}

/// Read the installed mod JAR names from a game directory (best-effort).
fn read_installed_mods(game_dir: &PathBuf) -> Vec<String> {
    std::fs::read_dir(game_dir.join("mods"))
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|name| name.ends_with(".jar"))
                .collect()
        })
        .unwrap_or_default()
}

/// Run the crash-analysis engine over the launch log and produce a
/// user-facing, categorized launch error the UI can surface with a Retry
/// action. The classification logic lives in `tuffbox_core` so it stays
/// unit-testable without linking the Tauri runtime.
fn classify_crash(ctx: &CrashExitCtx, exit_code: Option<i32>) -> LaunchErrorInfo {
    let installed_mods = read_installed_mods(&ctx.game_dir);
    // Prefer Minecraft's own latest.log; fall back to our console capture if
    // log4j never wrote anything (very early JVM death).
    let mut log_path = ctx.log_path.clone();
    let usable = log_path.is_file()
        && std::fs::metadata(&log_path)
            .map(|m| m.len() > 32)
            .unwrap_or(false);
    if !usable {
        if let Some(parent) = ctx.log_path.parent() {
            let console = parent.join("tuffbox-console.log");
            if console.is_file() {
                log_path = console;
            }
        }
    }
    tuffbox_core::crash_assistant::classify_launch_crash(
        &log_path,
        exit_code,
        &ctx.mc_version,
        &ctx.java_version,
        &ctx.loader_kind,
        &ctx.loader_version,
        &installed_mods,
    )
}

#[tauri::command]
fn import_curseforge_project(source: String, target_dir: String) -> Result<String, String> {
    let mut manifest = tuffbox_core::import_curseforge_pack(&source).map_err(|e| e.to_string())?;
    let _ =
        tuffbox_core::resolve_curseforge_pack_files(&mut manifest).map_err(|e| e.to_string())?;
    let target = PathBuf::from(&target_dir);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let overrides_folder =
        tuffbox_core::curseforge_overrides_folder(&source).unwrap_or_else(|_| "overrides".into());
    let _ = tuffbox_core::extract_curseforge_overrides(&source, &target, &overrides_folder);
    let _ = tuffbox_core::stash_curseforge_manifest(&source, &target);
    let manifest_path = target.join(format!("{}.tuffbox.json", manifest.project.id));
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&manifest_path, json).map_err(|e| e.to_string())?;
    Ok(manifest_path.to_string_lossy().to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn import_project(source: String, target_dir: String) -> Result<String, String> {
    use tuffbox_core::{
        import_curseforge_pack, import_folder, import_modrinth_pack, import_prism_instance,
        is_curseforge_pack, resolve_curseforge_pack_files,
    };

    let path = PathBuf::from(&source);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let (mut manifest, is_cf) = if path.is_dir() {
        (import_folder(&source).map_err(|e| e.to_string())?, false)
    } else {
        match ext.as_str() {
            "mrpack" => (
                import_modrinth_pack(&source).map_err(|e| e.to_string())?,
                false,
            ),
            "zip" if is_curseforge_pack(&source) => (
                import_curseforge_pack(&source).map_err(|e| e.to_string())?,
                true,
            ),
            "zip" => (
                import_prism_instance(&source).map_err(|e| e.to_string())?,
                false,
            ),
            _ => return Err(format!("unsupported import format: {ext}")),
        }
    };

    if is_cf {
        let _ = resolve_curseforge_pack_files(&mut manifest).map_err(|e| e.to_string())?;
    }

    let target_root = PathBuf::from(&target_dir).join(&manifest.project.id);
    std::fs::create_dir_all(&target_root).map_err(|e| e.to_string())?;
    if is_cf {
        let overrides_folder = tuffbox_core::curseforge_overrides_folder(&source)
            .unwrap_or_else(|_| "overrides".into());
        let _ =
            tuffbox_core::extract_curseforge_overrides(&source, &target_root, &overrides_folder);
        let _ = tuffbox_core::stash_curseforge_manifest(&source, &target_root);
    }

    let target = target_root.join(format!("{}.tuffbox.json", manifest.project.id));
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&target, json).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Search CurseForge modpacks (classId 4471), Prism FlamePage style.
#[tauri::command(rename_all = "camelCase")]
async fn search_curseforge_modpacks(
    query: String,
    game_version: Option<String>,
    offset: Option<u32>,
) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || {
        let provider = tuffbox_core::CurseForgeProvider::new();
        if !provider.is_configured() {
            return Err("CurseForge API key is not configured".to_string());
        }
        let hits = provider
            .search_modpacks(&query, game_version.as_deref(), offset.unwrap_or(0), 20)
            .map_err(|e| e.to_string())?
            .hits;
        Ok(hits
            .into_iter()
            .map(|h| {
                serde_json::json!({
                    "id": h.id,
                    "slug": h.slug,
                    "name": h.name,
                    "summary": h.summary,
                    "downloadCount": h.download_count,
                    "iconUrl": h.icon_url,
                    "authors": h.authors,
                    "categories": h.categories,
                })
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List files for a CurseForge modpack project.
#[tauri::command(rename_all = "camelCase")]
async fn get_curseforge_modpack_files(
    mod_id: u64,
    game_version: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    tokio::task::spawn_blocking(move || {
        let provider = tuffbox_core::CurseForgeProvider::new();
        let mut files = provider
            .get_mod_files(mod_id, game_version.as_deref())
            .map_err(|e| e.to_string())?;
        // Newest first so Discover "Add" picks a current pack version.
        files.sort_by(|a, b| b.file_date.cmp(&a.file_date).then_with(|| b.id.cmp(&a.id)));
        Ok(files
            .into_iter()
            .map(|f| {
                serde_json::json!({
                    "id": f.id,
                    "modId": f.mod_id,
                    "displayName": f.display_name,
                    "fileName": f.file_name,
                    "downloadUrl": f.resolved_download_url(),
                    "releaseType": f.release_type,
                    "gameVersions": f.game_versions,
                    "fileDate": f.file_date,
                    "blocked": f.blocked && f.resolved_download_url().is_none(),
                })
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Download a CurseForge / Modrinth / local pack and create an instance with
/// resolved mods + download progress (Prism InstanceImportTask flow).
#[tauri::command(rename_all = "camelCase")]
async fn install_modpack(
    app: tauri::AppHandle,
    source: String,
    target_dir: String,
    instance_name: Option<String>,
) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        use tauri::Emitter;
        use tuffbox_core::{
            curseforge_overrides_folder, extract_curseforge_overrides, import_curseforge_pack,
            import_modrinth_pack, import_prism_instance, is_curseforge_pack,
            resolve_curseforge_pack_files, stash_curseforge_manifest, CurseForgeProvider,
        };

        let _ = app.emit(
            "modpack-install-progress",
            serde_json::json!({ "phase": "resolving", "message": "Preparing modpack…" }),
        );
        let task_id = tuffbox_core::task_progress::start_task(
            format!("modpack-{}", tuffbox_core::time_util::compact_now()),
            "Install modpack",
        );
        tuffbox_core::task_progress::set_progress(&task_id, 0.05, Some("Preparing…".into()));

        // Remote CF file: source is "cf:<modId>:<fileId>" or a direct URL.
        let pack_path = if let Some(rest) = source.strip_prefix("cf:") {
            let parts: Vec<&str> = rest.split(':').collect();
            if parts.len() != 2 {
                return Err("expected cf:<modId>:<fileId>".into());
            }
            let mod_id: u64 = parts[0].parse().map_err(|_| "invalid mod id")?;
            let file_id: u64 = parts[1].parse().map_err(|_| "invalid file id")?;
            let provider = CurseForgeProvider::new();
            let file = provider.get_file(mod_id, file_id).map_err(|e| e.to_string())?;
            let urls = file.resolved_download_urls();
            if urls.is_empty() {
                return Err(format!(
                    "CurseForge returned no download URL for {} (file {}). Try importing the zip manually from CurseForge.",
                    file.file_name, file_id
                ));
            }
            let _ = app.emit(
                "modpack-install-progress",
                serde_json::json!({ "phase": "downloading-pack", "message": format!("Downloading {}", file.file_name) }),
            );
            let tmp = std::env::temp_dir().join(format!("tuffbox-pack-{}-{}.zip", mod_id, file_id));
            tuffbox_core::provider::curseforge::download_curseforge_url_candidates(
                &urls,
                &tmp,
                file.hashes.sha1.as_deref(),
            )
            .map_err(|e| format!("pack download failed: {e}"))?;
            tmp
        } else if source.starts_with("http://") || source.starts_with("https://") {
            let _ = app.emit(
                "modpack-install-progress",
                serde_json::json!({ "phase": "downloading-pack", "message": "Downloading pack…" }),
            );
            let tmp = std::env::temp_dir().join(format!(
                "tuffbox-pack-{}.zip",
                tuffbox_core::time_util::compact_now()
            ));
            tuffbox_core::download_with_sha1(&source, &tmp, None)
                .map_err(|e| format!("pack download failed: {e}"))?;
            tmp
        } else {
            PathBuf::from(&source)
        };

        if !pack_path.is_file() {
            return Err(format!("pack not found: {}", pack_path.display()));
        }

        let ext = pack_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let is_cf = is_curseforge_pack(&pack_path);
        let mut manifest = match ext.as_str() {
            "mrpack" => import_modrinth_pack(&pack_path).map_err(|e| e.to_string())?,
            "zip" if is_cf => import_curseforge_pack(&pack_path).map_err(|e| e.to_string())?,
            "zip" => import_prism_instance(&pack_path).map_err(|e| e.to_string())?,
            _ => return Err(format!("unsupported pack format: .{ext}")),
        };

        if let Some(name) = instance_name.filter(|n| !n.trim().is_empty()) {
            manifest.project.name = name.clone();
            manifest.project.id = name
                .to_lowercase()
                .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-")
                .replace("--", "-")
                .trim_matches('-')
                .to_string();
        }

        if is_cf {
            let _ = app.emit(
                "modpack-install-progress",
                serde_json::json!({ "phase": "resolving-files", "message": "Resolving CurseForge files…" }),
            );
            let resolved =
                resolve_curseforge_pack_files(&mut manifest).map_err(|e| e.to_string())?;
            let _ = app.emit(
                "modpack-install-progress",
                serde_json::json!({
                    "phase": "resolving-files",
                    "message": format!("Resolved {resolved} download URLs")
                }),
            );
        }

        let instance_dir = PathBuf::from(&target_dir).join(&manifest.project.id);
        std::fs::create_dir_all(&instance_dir).map_err(|e| e.to_string())?;

        if is_cf {
            let folder =
                curseforge_overrides_folder(&pack_path).unwrap_or_else(|_| "overrides".into());
            let n = extract_curseforge_overrides(&pack_path, &instance_dir, &folder)
                .map_err(|e| format!("failed to extract CurseForge overrides: {e}"))?;
            let _ = stash_curseforge_manifest(&pack_path, &instance_dir);
            let _ = app.emit(
                "modpack-install-progress",
                serde_json::json!({
                    "phase": "overrides",
                    "message": format!("Extracted {n} override files")
                }),
            );
        }

        let manifest_path = instance_dir.join(format!("{}.tuffbox.json", manifest.project.id));
        let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        std::fs::write(&manifest_path, &json).map_err(|e| e.to_string())?;

        let _ = app.emit(
            "modpack-install-progress",
            serde_json::json!({
                "phase": "downloading-mods",
                "message": format!("Downloading {} content files…", manifest.mods.len())
            }),
        );
        let report = download_project_mods_tracked(&app, &manifest_path, &manifest, None, true);

        let _ = app.emit(
            "modpack-install-progress",
            serde_json::json!({
                "phase": "done",
                "message": "Modpack installed",
                "failed": report.failed.len(),
            }),
        );
        tuffbox_core::task_progress::succeed(
            &task_id,
            Some(format!("{} mods", manifest.mods.len())),
        );

        Ok(serde_json::json!({
            "path": manifest_path.to_string_lossy(),
            "name": manifest.project.name,
            "modCount": manifest.mods.len(),
            "download": report,
            "provider": if is_cf { "curseforge" } else if ext == "mrpack" { "modrinth" } else { "prism" },
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Re-download only the mods that failed in the last sync (user Retry).
#[tauri::command(rename_all = "camelCase")]
async fn retry_failed_mod_downloads(
    app: tauri::AppHandle,
    path: String,
    mod_ids: Vec<String>,
) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let manifest_path = PathBuf::from(&path);
        let mut manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        // Re-resolve any CurseForge entries that still lack URLs.
        let needs_cf = manifest.mods.iter().any(|m| {
            mod_ids.contains(&m.id)
                && m.source.kind == SourceKind::Curseforge
                && m.source.url.as_ref().map(|u| u.is_empty()).unwrap_or(true)
        });
        if needs_cf {
            let _ = tuffbox_core::resolve_curseforge_pack_files(&mut manifest);
            let _ = save_manifest(&manifest_path, &manifest);
        }
        let only: std::collections::HashSet<String> = mod_ids.into_iter().collect();
        let report =
            download_project_mods_tracked(&app, &manifest_path, &manifest, Some(&only), true);
        Ok(serde_json::json!({ "download": report }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(deprecated)]
fn open_project_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let dir = PathBuf::from(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "manifest has no parent directory".to_string())?;
    use tauri_plugin_shell::ShellExt;
    app.shell().open(dir, None).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn create_project_desktop_shortcut(path: String) -> Result<String, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let project_dir = manifest_parent(&path)?;
    let desktop = dirs::desktop_dir().ok_or_else(|| "desktop folder was not found".to_string())?;
    let safe_name: String = manifest
        .project
        .name
        .chars()
        .map(|ch| {
            if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                ch
            }
        })
        .collect();

    #[cfg(target_os = "windows")]
    {
        let shortcut = desktop.join(format!("TuffBox - {safe_name}.url"));
        let target = project_dir.to_string_lossy().replace('\\', "/");
        let contents = format!("[InternetShortcut]\r\nURL=file:///{target}\r\nIconIndex=0\r\n");
        std::fs::write(&shortcut, contents).map_err(|e| e.to_string())?;
        return Ok(shortcut.to_string_lossy().to_string());
    }

    #[cfg(not(target_os = "windows"))]
    {
        let shortcut = desktop.join(format!("TuffBox - {safe_name}.desktop"));
        let target = project_dir.to_string_lossy();
        let contents = format!(
            "[Desktop Entry]\nType=Link\nName=TuffBox - {safe_name}\nURL=file://{target}\n"
        );
        std::fs::write(&shortcut, contents).map_err(|e| e.to_string())?;
        Ok(shortcut.to_string_lossy().to_string())
    }
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
        return Err(format!(
            "a folder named '{new_slug}' already exists next to this project"
        ));
    }
    std::fs::create_dir_all(&target_dir).map_err(|e| e.to_string())?;

    for entry_name in [
        "mods",
        "config",
        "defaultconfigs",
        "kubejs",
        "scripts",
        "overrides",
    ] {
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
        Ok(tuffbox_core::ensure_project_mods_downloaded(
            &manifest,
            &instance_dir,
        ))
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
    tokio::task::spawn_blocking(move || {
        tuffbox_core::versions::fetch_loader_versions(&loader, &minecraft_version)
    })
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
    let runtime =
        tuffbox_core::jre::check_java_at_path(&PathBuf::from(path)).map_err(|e| e.to_string())?;
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

fn append_test_run_record(
    manifest_path: &str,
    profile: &str,
    log_path: &Path,
) -> anyhow::Result<()> {
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
            if !p.is_file() {
                return None;
            }
            let meta = p.metadata().ok()?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            Some(serde_json::json!({
                "name": entry.file_name().to_string_lossy(),
                "size": meta.len(),
                "modified": modified,
            }))
        })
        .collect();
    entries.sort_by_key(|e| -(e["modified"].as_u64().unwrap_or(0) as i64));
    Ok(entries)
}

/// Reads a specific log file from the instance's logs/ folder.
#[tauri::command(rename_all = "camelCase")]
fn read_instance_log(path: String, log_name: String) -> Result<String, String> {
    let project_dir = manifest_parent(&path)?;
    let log_path = project_dir.join("logs").join(&log_name);
    if !log_path.exists() {
        return Ok(String::new());
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
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, total);
            } else if let Ok(meta) = p.metadata() {
                *total += meta.len();
            }
        }
    }
    for sub in &[
        "mods",
        "config",
        "resourcepacks",
        "shaderpacks",
        "datapacks",
        "scripts",
        "logs",
    ] {
        walk(&project_dir.join(sub), &mut total);
    }
    // Human-readable size
    if total < 1024 {
        Ok(format!("{} B", total))
    } else if total < 1024 * 1024 {
        Ok(format!("{:.1} KB", total as f64 / 1024.0))
    } else if total < 1024 * 1024 * 1024 {
        Ok(format!("{:.1} MB", total as f64 / 1024.0 / 1024.0))
    } else {
        Ok(format!("{:.1} GB", total as f64 / 1024.0 / 1024.0 / 1024.0))
    }
}

#[tauri::command(rename_all = "camelCase")]
fn get_launch_log(path: String) -> Result<String, String> {
    let project_dir = PathBuf::from(&path)
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "manifest has no parent directory".to_string())?;
    let log_path = resolve_live_launch_log(&project_dir.join("logs"));
    tuffbox_core::process::read_log_tail(&log_path, 2500).map_err(|e| e.to_string())
}

/// Same source the Live tab tails: prefer Minecraft `latest.log` once it has
/// real content, else TuffBox console capture.
fn resolve_live_launch_log(logs_dir: &Path) -> PathBuf {
    let console = logs_dir.join("tuffbox-console.log");
    let latest = logs_dir.join("latest.log");
    let console_len = std::fs::metadata(&console).map(|m| m.len()).unwrap_or(0);
    let latest_len = std::fs::metadata(&latest).map(|m| m.len()).unwrap_or(0);
    if latest_len > 256 {
        latest
    } else if console_len > 0 {
        console
    } else if latest.exists() {
        latest
    } else {
        console
    }
}

/// Upload a crash / instance log to mclo.gs and return the public share URL.
/// - `logName` set → that file under logs/ or crash-reports/
/// - `logName` = `__live__` → same resolution as the Live log tab (latest.log preferred)
/// - `logName` omitted → newest crash-report, then latest.log (post-crash share)
#[tauri::command(rename_all = "camelCase")]
fn share_log_mclogs(
    path: String,
    log_name: Option<String>,
) -> Result<serde_json::Value, String> {
    let project_dir = manifest_parent(&path)?;
    let logs_dir = project_dir.join("logs");
    let crashes_dir = project_dir.join("crash-reports");

    let log_path = match log_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some("__live__") => {
            let p = resolve_live_launch_log(&logs_dir);
            if !p.exists() {
                return Err("no live log found (latest.log / console empty)".into());
            }
            p
        }
        Some(name) => {
            let candidate = if name.starts_with("crash-") || name.ends_with(".txt") {
                crashes_dir.join(name)
            } else {
                logs_dir.join(name)
            };
            if !candidate.exists() {
                return Err(format!("log not found: {name}"));
            }
            candidate
        }
        None => pick_shareable_crash_log(&logs_dir, &crashes_dir)
            .ok_or_else(|| "no crash report or latest.log found to share".to_string())?,
    };

    // Read more than the UI tail so the shared paste has useful context.
    let content = tuffbox_core::process::read_log_tail(&log_path, 20_000)
        .map_err(|e| e.to_string())?;
    if content.trim().is_empty() {
        return Err("log file is empty".into());
    }

    let manifest = ProjectManifest::load_from_path(&path).ok();
    let mut metadata = Vec::new();
    if let Some(m) = &manifest {
        metadata.push(tuffbox_core::mclo_gs::MetadataEntry {
            key: "minecraft".into(),
            value: serde_json::json!(m.minecraft.version),
            label: Some("Minecraft".into()),
            visible: Some(true),
        });
        metadata.push(tuffbox_core::mclo_gs::MetadataEntry {
            key: "loader".into(),
            value: serde_json::json!(format!("{} {}", m.loader.kind.as_str(), m.loader.version)),
            label: Some("Loader".into()),
            visible: Some(true),
        });
        metadata.push(tuffbox_core::mclo_gs::MetadataEntry {
            key: "project".into(),
            value: serde_json::json!(m.project.name),
            label: Some("Project".into()),
            visible: Some(true),
        });
    }
    metadata.push(tuffbox_core::mclo_gs::MetadataEntry {
        key: "file".into(),
        value: serde_json::json!(log_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("log")),
        label: Some("File".into()),
        visible: Some(true),
    });

    let shared = tuffbox_core::mclo_gs::upload_log(&content, "TuffBox IDE", metadata)
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "id": shared.id,
        "url": shared.url,
        "rawUrl": shared.raw_url,
        "lines": shared.lines,
        "size": shared.size,
        // Intentionally omit token from the renderer — keep deletion capability
        // out of the webview attack surface.
        "fileName": log_path.file_name().and_then(|n| n.to_str()),
    }))
}

fn pick_shareable_crash_log(logs_dir: &Path, crashes_dir: &Path) -> Option<PathBuf> {
    // Newest crash-report first (best match for "I just crashed").
    if let Ok(rd) = std::fs::read_dir(crashes_dir) {
        let mut reports: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .is_some_and(|e| e.eq_ignore_ascii_case("txt"))
            })
            .collect();
        reports.sort_by_key(|p| {
            std::cmp::Reverse(
                p.metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            )
        });
        if let Some(first) = reports.into_iter().next() {
            return Some(first);
        }
    }
    let latest = logs_dir.join("latest.log");
    if latest.exists() {
        return Some(latest);
    }
    let console = logs_dir.join("tuffbox-console.log");
    if console.exists() {
        return Some(console);
    }
    None
}

/// Analyze an arbitrary log/console text against the installed mods of a
/// project and return the suspected mods together with the exact line numbers
/// where they were referenced, so the UI can highlight those lines.
#[tauri::command(rename_all = "camelCase")]
fn analyze_log_text(
    path: String,
    text: String,
) -> Result<serde_json::Value, String> {
    use tuffbox_core::crash::analyze_text_for_suspects;
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let (signals, suspected_mods) = analyze_text_for_suspects(&text, "log", &manifest);
    let highlights: Vec<serde_json::Value> = suspected_mods
        .iter()
        .flat_map(|s| {
            s.evidence.iter().map(move |ev| {
                serde_json::json!({
                    "lineNumber": ev.line_number,
                    "modId": s.id,
                    "modName": s.name,
                    "confidence": s.confidence,
                    "kind": format!("{:?}", ev.kind),
                    "text": ev.text,
                })
            })
        })
        .collect();
    Ok(serde_json::json!({
        "signals": signals.len(),
        "suspectedMods": suspected_mods,
        "highlights": highlights,
    }))
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
    let mut manifests = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".tuffbox.json"))
            .unwrap_or(false)
        {
            manifests.push(path);
        }
    }

    if manifests.is_empty() {
        return Err(format!(
            "project manifest not found in project directory: {}",
            dir.display()
        ));
    }

    if manifests.len() == 1 {
        return Ok(manifests.remove(0));
    }

    let state = load_launcher_data(&dir);
    if let Some(ref last_opened) = state.last_opened {
        let preferred = PathBuf::from(last_opened);
        if manifests.iter().any(|path| path == &preferred) {
            return Ok(preferred);
        }
    }

    let default = dir.join("project.tuffbox.json");
    if default.exists() {
        return Ok(default);
    }

    manifests.sort();
    Ok(manifests[0].clone())
}

/// Resolve a project directory or manifest path to the canonical `.tuffbox.json` file.
/// If the given manifest path does not exist, scans the parent folder for any manifest.
fn resolve_manifest_path(path: &str) -> Result<PathBuf, String> {
    let path_buf = PathBuf::from(path);

    if path_buf.is_dir() {
        return find_manifest_in_project_dir(path);
    }

    if path_buf.is_file() {
        return Ok(path_buf);
    }

    if let Some(parent) = path_buf.parent() {
        if parent.is_dir() {
            if let Ok(found) = find_manifest_in_project_dir(&parent.to_string_lossy()) {
                return Ok(found);
            }
        }
    }

    Err(format!(
        "project manifest not found: {}",
        path_buf.display()
    ))
}

pub(crate) fn manifest_parent(path: &str) -> Result<PathBuf, String> {
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
        let relative = match path.strip_prefix(project_dir) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => continue,
        };
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
        "json"
            | "json5"
            | "toml"
            | "properties"
            | "cfg"
            | "conf"
            | "txt"
            | "js"
            | "zs"
            | "yaml"
            | "yml"
            | "md"
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
    std::fs::read_to_string(path)
        .map_err(|_| "# Binary or non-UTF8 file; inline diff unavailable.\n".to_string())
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
    let before_mods: std::collections::HashMap<_, _> =
        before.mods.iter().map(|m| (m.id.as_str(), m)).collect();
    let after_mods: std::collections::HashMap<_, _> =
        after.mods.iter().map(|m| (m.id.as_str(), m)).collect();

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
                preview: format!(
                    "Added {} {} ({:?})",
                    module.name, module.version, module.side
                ),
                diff: format!("+ {} {} ({:?})", module.name, module.version, module.side),
                can_open: false,
                tags: snapshot.tags.clone(),
                crash_fingerprint_key: snapshot.crash_fingerprint_key.clone(),
                plan_source: snapshot.plan_source.clone(),
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
                preview: format!(
                    "Removed {} {} ({:?})",
                    module.name, module.version, module.side
                ),
                diff: format!("- {} {} ({:?})", module.name, module.version, module.side),
                can_open: false,
                tags: snapshot.tags.clone(),
                crash_fingerprint_key: snapshot.crash_fingerprint_key.clone(),
                plan_source: snapshot.plan_source.clone(),
            });
        }
    }

    for (id, before_module) in &before_mods {
        let Some(after_module) = after_mods.get(*id) else {
            continue;
        };
        if before_module.version != after_module.version
            || before_module.file_name != after_module.file_name
            || before_module.side != after_module.side
        {
            entries.push(ProjectChangeEntry {
                id: format!("{}:mod-updated:{id}", snapshot.id),
                snapshot_id: snapshot.id.clone(),
                operation: snapshot.name.clone(),
                reason: snapshot.reason.clone(),
                created_at: snapshot.created_at.clone(),
                path: "project.tuffbox.json".to_string(),
                category: "Mods".to_string(),
                kind: "mod_updated".to_string(),
                preview: format!(
                    "Updated {}: {} → {}",
                    after_module.name, before_module.version, after_module.version
                ),
                diff: format!(
                    "- {} {} ({:?})\n+ {} {} ({:?})",
                    before_module.name,
                    before_module.version,
                    before_module.side,
                    after_module.name,
                    after_module.version,
                    after_module.side
                ),
                can_open: false,
                tags: snapshot.tags.clone(),
                crash_fingerprint_key: snapshot.crash_fingerprint_key.clone(),
                plan_source: snapshot.plan_source.clone(),
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

fn remove_mod_file_from_disk(manifest_path: &Path, removed_mod: &ModSpec) {
    if let Some(file_name) = &removed_mod.file_name {
        if let Some(instance_dir) = tuffbox_core::instance_dir_for_manifest(manifest_path) {
            let content_dir =
                tuffbox_core::content_dir_for(&instance_dir, removed_mod.content_type);
            let _ = std::fs::remove_file(content_dir.join(file_name));
            let _ = std::fs::remove_file(content_dir.join(format!("{file_name}.disabled")));
        }
    }
}

fn mod_file_path(manifest_path: &Path, module: &ModSpec) -> Option<PathBuf> {
    let file_name = module.file_name.as_ref()?;
    let instance_dir = tuffbox_core::instance_dir_for_manifest(manifest_path)?;
    Some(tuffbox_core::content_dir_for(&instance_dir, module.content_type).join(file_name))
}

fn existing_mod_file_path(manifest_path: &Path, module: &ModSpec) -> Option<PathBuf> {
    let normal = mod_file_path(manifest_path, module)?;
    if normal.is_file() {
        return Some(normal);
    }
    let file_name = normal.file_name()?.to_string_lossy();
    let disabled = normal.with_file_name(format!("{file_name}.disabled"));
    disabled.is_file().then_some(disabled)
}

/// Removes jars superseded by an update: the previous filename and any file
/// whose sha1 still matches the pre-update artifact. Filename-only cleanup
/// misses Modrinth renames (`mod-1.0.0.jar` → `mod-1.0.1.jar`) when the
/// manifest path was already out of sync with disk.
fn remove_superseded_mod_files(manifest_path: &Path, old_mod: &ModSpec, new_mod: &ModSpec) {
    let Some(instance_dir) = tuffbox_core::instance_dir_for_manifest(manifest_path) else {
        return;
    };
    let content_dir = tuffbox_core::content_dir_for(&instance_dir, new_mod.content_type);
    let keep_name = new_mod.file_name.as_deref();
    let old_name = old_mod.file_name.as_deref();
    let old_sha1 = old_mod
        .hashes
        .as_ref()
        .and_then(|h| h.sha1.as_ref())
        .filter(|h| !h.is_empty())
        .cloned();

    let Ok(entries) = std::fs::read_dir(&content_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let base = name.strip_suffix(".disabled").unwrap_or(name.as_str());
        if !(base.ends_with(".jar") || base.ends_with(".zip")) {
            continue;
        }
        if keep_name == Some(base) {
            continue;
        }

        let mut remove = old_name == Some(base);
        if !remove {
            if let Some(ref expected) = old_sha1 {
                if let Ok(actual) = tuffbox_core::sha1_file(&path) {
                    if actual.eq_ignore_ascii_case(expected) {
                        remove = true;
                    }
                }
            }
        }
        // Also drop leftover jars that share the mod slug as a filename prefix
        // (e.g. sodium-fabric-0.5.0.jar after updating to sodium-fabric-0.5.8.jar).
        if !remove {
            let id = old_mod.id.to_lowercase().replace('_', "-");
            let base_l = base.to_lowercase();
            if !id.is_empty()
                && (base_l.starts_with(&id) || base_l.starts_with(&format!("{id}-")))
                && keep_name != Some(base)
            {
                remove = true;
            }
        }
        if remove {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn refresh_modrinth_file_metadata(
    manifest: &ProjectManifest,
    module: &mut ModSpec,
) -> Result<(), String> {
    if module.source.kind != SourceKind::Modrinth && module.source.project_id.is_none() {
        return Ok(());
    }
    let Some(version_id) = module.source.file_id.clone() else {
        return Ok(());
    };
    let provider = tuffbox_core::ModrinthProvider::new();
    let version = provider
        .get_version(&version_id)
        .map_err(|e| format!("failed to refresh {} from Modrinth: {e}", module.name))?;
    let loader_slug = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind);
    // Loader must match so we don't install a Forge jar into a Fabric instance.
    // Minecraft mismatch is allowed for intentional cross-version switches from
    // the version picker (user confirms incompatible installs in the UI).
    if !version
        .loaders
        .iter()
        .any(|loader| loader == loader_slug || (loader_slug == "quilt" && loader == "fabric"))
    {
        return Err(format!(
            "{} update has no build for loader {loader_slug} (supports [{}])",
            module.name,
            version.loaders.join(", ")
        ));
    }
    let file =
        ProviderFileInfo::select_file_for_loader(&version, loader_slug).ok_or_else(|| {
            format!(
                "{} update has no downloadable file for loader {loader_slug}",
                module.name
            )
        })?;
    module.source.url = Some(file.url.clone());
    module.file_name = Some(file.filename.clone());
    module.version = version.version_number.clone();
    module.hashes = Some(tuffbox_core::FileHashes {
        sha1: file.hashes.sha1.clone(),
        sha512: file.hashes.sha512.clone(),
    });
    Ok(())
}

/// Downloads and verifies a single replacement before publishing its
/// manifest entry. The previous file is kept until both operations succeed,
/// so a network/hash/manifest error cannot leave metadata pointing at bytes
/// that were never installed.
fn commit_single_mod_update(
    app: &tauri::AppHandle,
    manifest_path: &Path,
    updated_manifest: &mut ProjectManifest,
    old_mod: &ModSpec,
    emit_lifecycle: bool,
) -> Result<tuffbox_core::ModSyncReport, String> {
    let project_id = old_mod.source.project_id.as_deref();
    let mut new_mod = updated_manifest
        .mods
        .iter()
        .find(|module| {
            module.id == old_mod.id
                || (project_id.is_some() && module.source.project_id.as_deref() == project_id)
        })
        .ok_or_else(|| format!("updated mod {} disappeared from manifest", old_mod.id))?
        .clone();
    refresh_modrinth_file_metadata(updated_manifest, &mut new_mod)?;
    if let Some(idx) = updated_manifest
        .mods
        .iter()
        .position(|module| module.id == new_mod.id)
    {
        updated_manifest.mods[idx] = new_mod.clone();
    }
    let old_path = existing_mod_file_path(manifest_path, old_mod);
    let new_path = mod_file_path(manifest_path, &new_mod)
        .ok_or_else(|| format!("updated mod {} has no destination file", new_mod.id))?;

    // Prefer the on-disk hash for cleanup — manifest metadata may already
    // disagree with the jar after a partial prior update.
    let mut old_for_cleanup = old_mod.clone();
    if let Some(path) = old_path.as_ref() {
        if let Ok(hash) = tuffbox_core::sha1_file(path) {
            old_for_cleanup.hashes = Some(tuffbox_core::FileHashes {
                sha1: Some(hash),
                sha512: old_mod
                    .hashes
                    .as_ref()
                    .and_then(|h| h.sha512.clone()),
            });
        }
        if let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) {
            let base = name.strip_suffix(".disabled").unwrap_or(&name).to_string();
            old_for_cleanup.file_name = Some(base);
        }
    }

    let backup = if let Some(path) = old_path.as_ref() {
        let parent = path
            .parent()
            .ok_or_else(|| format!("invalid mod path {}", path.display()))?;
        let staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        std::fs::copy(path, staged.path())
            .map_err(|e| format!("failed to preserve {} before update: {e}", path.display()))?;
        Some(staged)
    } else {
        None
    };

    let restore = || {
        if old_path.as_ref() != Some(&new_path) {
            let _ = std::fs::remove_file(&new_path);
        }
        if let (Some(path), Some(staged)) = (old_path.as_ref(), backup.as_ref()) {
            let _ = std::fs::copy(staged.path(), path);
        }
    };

    let mut download_manifest = updated_manifest.clone();
    download_manifest.mods = vec![new_mod.clone()];
    let only_mod = std::collections::HashSet::from([new_mod.id.clone()]);
    let report = download_project_mods_tracked(
        app,
        manifest_path,
        &download_manifest,
        Some(&only_mod),
        emit_lifecycle,
    );
    if let Some(failure) = report
        .failed
        .iter()
        .find(|failure| failure.mod_id == new_mod.id)
    {
        restore();
        return Err(format!(
            "failed to update {}: {}",
            new_mod.name, failure.error
        ));
    }

    if let Err(error) = save_manifest(manifest_path, updated_manifest) {
        restore();
        return Err(format!(
            "downloaded {}, but could not save manifest: {error}",
            new_mod.name
        ));
    }

    remove_superseded_mod_files(manifest_path, &old_for_cleanup, &new_mod);
    Ok(report)
}

fn apply_change_action(
    manifest_path: &Path,
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
        tuffbox_core::ChangeAction::RemoveMod { node_id } => {
            let mod_id = node_id
                .0
                .strip_prefix("mod:")
                .unwrap_or(&node_id.0)
                .to_string();
            let removed_mod = manifest.mods.iter().find(|m| m.id == mod_id).cloned();
            let before = manifest.mods.len();
            manifest.mods.retain(|m| m.id != mod_id);
            if manifest.mods.len() != before {
                if let Some(removed_mod) = removed_mod {
                    remove_mod_file_from_disk(manifest_path, &removed_mod);
                }
                applied.push(format!("removed {mod_id}"));
            }
        }
        tuffbox_core::ChangeAction::DisableMod { node_id } => {
            let mod_id = node_id
                .0
                .strip_prefix("mod:")
                .unwrap_or(&node_id.0)
                .to_string();
            if let Some(module) = manifest.mods.iter_mut().find(|m| m.id == mod_id) {
                if let Some(file_name) = module.file_name.clone() {
                    if let Some(instance_dir) =
                        tuffbox_core::instance_dir_for_manifest(manifest_path)
                    {
                        let content_dir =
                            tuffbox_core::content_dir_for(&instance_dir, module.content_type);
                        let active = content_dir.join(&file_name);
                        let disabled = content_dir.join(format!("{file_name}.disabled"));
                        if active.is_file() {
                            let _ = std::fs::rename(&active, &disabled);
                        }
                    }
                }
                if !module
                    .status
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case("disabled"))
                {
                    module.status.push("disabled".to_string());
                }
                applied.push(format!("disabled {mod_id}"));
            }
        }
        tuffbox_core::ChangeAction::UpdateMod {
            node_id,
            target_version,
        } => {
            let mod_id = node_id
                .0
                .strip_prefix("mod:")
                .unwrap_or(&node_id.0)
                .to_string();
            let target_version = target_version.trim();
            let version_id = if target_version.is_empty() || target_version == "latest-compatible" {
                None
            } else {
                Some(target_version)
            };
            update_mod_from_modrinth(manifest_path, manifest, &mod_id, version_id)
                .map_err(|e| e.to_string())?;
            applied.push(format!("updated {mod_id}"));
        }
        tuffbox_core::ChangeAction::EditConfig { path, patch } => {
            let envelope: serde_json::Value = serde_json::from_str(&patch).unwrap_or_else(|_| {
                serde_json::json!({
                    "patchType": "replace_file",
                    "patch": patch,
                })
            });
            let patch_type = envelope
                .get("patchType")
                .and_then(|v| v.as_str())
                .unwrap_or("replace_file");
            let patch_value = envelope
                .get("patch")
                .cloned()
                .unwrap_or(serde_json::Value::String(patch.clone()));
            let project_dir = manifest_path
                .parent()
                .ok_or_else(|| "manifest has no parent".to_string())?;
            let target = safe_project_file(project_dir, &path)?;
            let current = if target.is_file() {
                std::fs::read_to_string(&target).map_err(|e| e.to_string())?
            } else {
                String::new()
            };
            let new_content = tuffbox_core::action_plan::apply_config_patch(
                &current,
                &path,
                patch_type,
                &patch_value,
            )?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&target, new_content).map_err(|e| e.to_string())?;
            applied.push(format!("edited config {path}"));
        }
    }
    Ok(())
}

fn install_modrinth_with_dependencies(
    manifest: &mut ProjectManifest,
    mod_ids: &[String],
    side: &str,
) -> Vec<String> {
    install_modrinth_with_dependencies_rounds(manifest, mod_ids, side, 50)
}

pub(crate) fn install_modrinth_with_dependencies_rounds(
    manifest: &mut ProjectManifest,
    mod_ids: &[String],
    side: &str,
    max_rounds: usize,
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
    for _ in 0..max_rounds {
        let missing = manifest
            .mods
            .iter()
            .flat_map(|module| module.dependencies.iter())
            .filter(|dep| dep.kind == tuffbox_core::DependencyKind::Requires)
            .map(|dep| dep.target.clone())
            .filter(|target| {
                !manifest.mods.iter().any(|m| {
                    m.id == *target || m.source.project_id.as_deref() == Some(target.as_str())
                }) && !failed.contains(target)
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

    if manifest.mods.iter().any(|m| {
        m.id == project.slug || m.source.project_id.as_deref() == Some(project.id.as_str())
    }) {
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

    let file = ProviderFileInfo::select_file_for_loader(
        &version,
        &tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind),
    )
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("no primary file for version {}", version.id))?;

    let dependencies = provider.resolve_dependencies(&version.id)?;
    let mod_side = parse_side(side.as_deref(), Some(&project));
    let mod_spec = build_mod_spec(&project, &version, file, dependencies, mod_side);
    manifest.mods.push(mod_spec);
    Ok(())
}

fn add_mod_from_curseforge(
    manifest: &mut ProjectManifest,
    mod_id: &str,
    side: Option<String>,
) -> anyhow::Result<()> {
    let project_id: u64 = mod_id
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid CurseForge project id: {mod_id}"))?;
    let provider = tuffbox_core::CurseForgeProvider::new();
    if !provider.is_configured() {
        anyhow::bail!("CurseForge API key is not configured");
    }
    let hit = provider.get_mod(project_id)?;
    let project_id_str = project_id.to_string();
    let slug = if hit.slug.is_empty() {
        format!("cf-{project_id}")
    } else {
        hit.slug.clone()
    };
    if manifest.mods.iter().any(|m| {
        m.id == slug
            || m.source.project_id.as_deref() == Some(mod_id)
            || m.source.project_id.as_deref() == Some(project_id_str.as_str())
    }) {
        anyhow::bail!("mod {slug} is already in the project");
    }

    let loader = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string();
    let mc = manifest.minecraft.version.clone();
    let mut files = provider.get_mod_files(project_id, Some(&mc))?;
    if files.is_empty() {
        files = provider.get_mod_files(project_id, None)?;
    }
    let chosen = tuffbox_core::CurseForgeProvider::pick_best_file(&files, &mc, &loader)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no compatible CurseForge file for {slug}"))?;
    let mut file = provider.get_file(project_id, chosen.id).unwrap_or(chosen);
    if file
        .download_url
        .as_ref()
        .map(|u| u.is_empty())
        .unwrap_or(true)
    {
        let mut map = std::collections::HashMap::from([(file.id, file.clone())]);
        let _ = provider.apply_modrinth_fallback(&mut map);
        if let Some(resolved) = map.remove(&file.id) {
            file = resolved;
        }
    }
    let download_url = file.resolved_download_url().ok_or_else(|| {
        anyhow::anyhow!(
            "CurseForge withheld the download URL for {slug}. Install the file manually or mirror it via Modrinth."
        )
    })?;

    let content_type = match hit.class_id.unwrap_or(6) {
        12 => tuffbox_core::manifest::ContentType::Resourcepack,
        6552 => tuffbox_core::manifest::ContentType::Shaderpack,
        6945 => tuffbox_core::manifest::ContentType::Datapack,
        _ => tuffbox_core::manifest::ContentType::Mod,
    };
    let mod_side = parse_side(side.as_deref(), None);
    manifest.mods.push(ModSpec {
        id: slug,
        name: hit.name,
        source: ModSource {
            kind: SourceKind::Curseforge,
            project_id: Some(project_id_str),
            file_id: Some(file.id.to_string()),
            url: Some(download_url),
            path: None,
            icon_url: hit.icon_url,
            categories: Vec::new(),
        },
        version: file.display_name.clone(),
        file_name: Some(file.file_name),
        hashes: Some(tuffbox_core::FileHashes {
            sha1: file.hashes.sha1,
            sha512: file.hashes.sha512,
        }),
        side: mod_side,
        dependencies: vec![],
        status: vec!["ok".to_string()],
        content_type,
        authors: hit.authors.clone(),
    });
    Ok(())
}

fn update_mod_from_modrinth(
    _manifest_path: &Path,
    manifest: &mut ProjectManifest,
    mod_id: &str,
    version_id: Option<&str>,
) -> anyhow::Result<()> {
    let provider = tuffbox_core::ModrinthProvider::new();
    let index = manifest
        .mods
        .iter()
        .position(|m| m.id == mod_id || m.source.project_id.as_deref() == Some(mod_id))
        .ok_or_else(|| anyhow::anyhow!("mod {mod_id} not found in project"))?;

    let old_mod = manifest.mods[index].clone();
    let project_id = old_mod
        .source
        .project_id
        .clone()
        .unwrap_or_else(|| mod_id.to_string());
    let project = provider.get_project(&project_id)?;

    let version = if let Some(vid) = version_id.filter(|v| !v.trim().is_empty()) {
        // Prefer the exact version from the update check / change plan.
        match provider.get_version(vid) {
            Ok(v) => v,
            Err(_) => {
                // `target_version` may be a version_number rather than an id.
                let query = ProviderSearchQuery {
                    query: None,
                    minecraft_version: Some(manifest.minecraft.version.clone()),
                    loader: Some(
                        tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string(),
                    ),
                    ..Default::default()
                };
                provider
                    .get_versions(&project_id, &query)?
                    .into_iter()
                    .find(|v| v.version_number == vid || v.id == vid)
                    .ok_or_else(|| anyhow::anyhow!("version {vid} not found for {project_id}"))?
            }
        }
    } else {
        let query = ProviderSearchQuery {
            query: None,
            minecraft_version: Some(manifest.minecraft.version.clone()),
            loader: Some(tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string()),
            ..Default::default()
        };
        provider
            .get_versions(&project_id, &query)?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("no compatible version found for {project_id}"))?
    };

    let file = ProviderFileInfo::select_file_for_loader(
        &version,
        &tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind),
    )
    .cloned()
    .ok_or_else(|| anyhow::anyhow!("no primary file for version {}", version.id))?;

    let dependencies = provider
        .resolve_dependencies(&version.id)
        .unwrap_or_else(|_| old_mod.dependencies.clone());
    let mut new_spec = build_mod_spec(
        &project,
        &version,
        file,
        dependencies,
        infer_project_side(Some(&project)),
    );
    // A manifest id is referenced by UI state, dependency edges, progress
    // scopes and history. Keep it stable even if Modrinth changed the slug.
    new_spec.id = old_mod.id;

    manifest.mods[index] = new_spec;
    Ok(())
}

/// Builds a minimal ProjectInfo from an existing ModSpec — used as a
/// fallback when Modrinth project lookup fails during batch updates.
fn project_info_from_mod(module: &ModSpec) -> tuffbox_core::ProjectInfo {
    let project_type = match module.content_type {
        tuffbox_core::manifest::ContentType::Resourcepack => "resourcepack",
        tuffbox_core::manifest::ContentType::Shaderpack => "shader",
        tuffbox_core::manifest::ContentType::Datapack => "datapack",
        tuffbox_core::manifest::ContentType::Mod => "mod",
    };
    tuffbox_core::ProjectInfo {
        id: module
            .source
            .project_id
            .clone()
            .unwrap_or_else(|| module.id.clone()),
        slug: module.id.clone(),
        name: module.name.clone(),
        description: String::new(),
        project_type: project_type.to_string(),
        icon_url: module.source.icon_url.clone(),
        author: None,
        downloads: None,
        follows: None,
        date_modified: None,
        categories: Vec::new(),
        license: None,
        client_side: None,
        server_side: None,
    }
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
            icon_url: project.icon_url.clone(),
            categories: project.categories.clone(),
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
        content_type: tuffbox_core::manifest::ContentType::from_modrinth_project_type(
            &project.project_type,
        ),
        authors: project
            .author
            .as_ref()
            .map(|a| vec![a.clone()])
            .unwrap_or_default(),
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
    let Some(project) = project else {
        return Side::Unknown;
    };
    Side::from_modrinth(project.client_side.as_deref(), project.server_side.as_deref())
}

pub(crate) fn auto_snapshot(manifest_path: &Path, operation: &str) -> anyhow::Result<Snapshot> {
    auto_snapshot_with_changed_files(manifest_path, operation, &[])
}

fn auto_snapshot_with_changed_files(
    manifest_path: &Path,
    operation: &str,
    changed_files: &[PathBuf],
) -> anyhow::Result<Snapshot> {
    let project_dir = manifest_path.parent().ok_or_else(|| {
        anyhow::anyhow!("manifest path has no parent: {}", manifest_path.display())
    })?;
    let lockfile_path = manifest_path.with_extension("lock.json");
    let lockfile_path = if lockfile_path.exists() {
        Some(lockfile_path)
    } else {
        None
    };
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

pub(crate) fn save_manifest(path: &Path, manifest: &ProjectManifest) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(manifest)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("manifest path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::Builder::new()
        .prefix(".tuffbox-manifest-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    staged.write_all(json.as_bytes())?;
    staged.flush()?;
    staged.as_file().sync_all()?;
    staged
        .persist(path)
        .map_err(|error| anyhow::Error::new(error.error))?;
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
    let from_snapshot = store
        .get(&from_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("snapshot {from_id} not found"))?;
    let to_snapshot = store
        .get(&to_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("snapshot {to_id} not found"))?;

    let from_text = std::fs::read_to_string(&from_snapshot.manifest_path).unwrap_or_default();
    let to_text = std::fs::read_to_string(&to_snapshot.manifest_path).unwrap_or_default();
    let from_json: serde_json::Value = serde_json::from_str(&from_text).unwrap_or_default();
    let to_json: serde_json::Value = serde_json::from_str(&to_text).unwrap_or_default();

    let from_mods: std::collections::HashSet<String> = from_json
        .get("mods")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();
    let to_mods: std::collections::HashSet<String> = to_json
        .get("mods")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();
    let added_mods: Vec<_> = to_mods.difference(&from_mods).collect();
    let removed_mods: Vec<_> = from_mods.difference(&to_mods).collect();
    let from_ver = from_json
        .get("minecraft")
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let to_ver = to_json
        .get("minecraft")
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let from_loader = from_json
        .get("loader")
        .and_then(|l| l.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let to_loader = to_json
        .get("loader")
        .and_then(|l| l.get("version"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

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
        .map(|g| {
            serde_json::json!({
                "instanceId": g.instance_id,
                "startedAt": g.started_at,
            })
        })
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

/// Returns true when the mod's jar is missing or its on-disk SHA1 does not
/// match the manifest — i.e. a download is required.
fn mod_needs_download(instance_dir: &Path, module: &ModSpec) -> bool {
    if module.source.kind == SourceKind::Local {
        return false;
    }
    let Some(file_name) = &module.file_name else {
        return false;
    };
    if module.source.url.is_none() {
        return false;
    }
    let target = tuffbox_core::content_dir_for(instance_dir, module.content_type).join(file_name);
    if !target.is_file() {
        return true;
    }
    match module.hashes.as_ref().and_then(|h| h.sha1.as_deref()) {
        Some(expected) => tuffbox_core::sha1_file(&target)
            .map(|actual| !actual.eq_ignore_ascii_case(expected))
            .unwrap_or(true),
        None => false,
    }
}

/// ────────────────────────────────────────────────────────────────────
fn download_project_mods(
    manifest_path: &Path,
    manifest: &ProjectManifest,
) -> tuffbox_core::ModSyncReport {
    let instance_dir =
        tuffbox_core::instance_dir_for_manifest(manifest_path).unwrap_or_else(|| {
            manifest_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default()
        });
    tuffbox_core::ensure_project_mods_downloaded(manifest, &instance_dir)
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModDownloadProgressPayload {
    id: String,
    name: String,
    downloaded: u64,
    total: u64,
    percent: u32,
    status: String,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModUpdateProgressPayload {
    phase: String,
    message: String,
    current: usize,
    total: usize,
    percent: u32,
    mod_id: Option<String>,
}

fn emit_mod_update_progress(
    app: &tauri::AppHandle,
    phase: &str,
    message: &str,
    current: usize,
    total: usize,
    percent: u32,
    mod_id: Option<&str>,
) {
    use tauri::Emitter;
    let _ = app.emit(
        "mod-update-progress",
        ModUpdateProgressPayload {
            phase: phase.to_string(),
            message: message.to_string(),
            current,
            total,
            percent: percent.min(100),
            mod_id: mod_id.map(str::to_string),
        },
    );
}

fn emit_mod_download_status(
    app: &tauri::AppHandle,
    id: &str,
    name: &str,
    status: &str,
    percent: u32,
) {
    use tauri::Emitter;
    let _ = app.emit(
        "mod-download-progress",
        ModDownloadProgressPayload {
            id: id.to_string(),
            name: name.to_string(),
            downloaded: 0,
            total: 0,
            percent: percent.min(100),
            status: status.to_string(),
        },
    );
}

/// Downloads missing mod files while streaming per-mod byte progress to the
/// frontend via `mod-download-progress` / `mod-download-batch` events and the
/// `DOWNLOAD_PROGRESS` snapshot map.
pub(crate) fn download_project_mods_tracked(
    app: &tauri::AppHandle,
    manifest_path: &Path,
    manifest: &ProjectManifest,
    only_mod_ids: Option<&std::collections::HashSet<String>>,
    emit_lifecycle: bool,
) -> tuffbox_core::ModSyncReport {
    use tauri::Emitter;

    let instance_dir =
        tuffbox_core::instance_dir_for_manifest(manifest_path).unwrap_or_else(|| {
            manifest_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default()
        });

    if let Ok(mut map) = DOWNLOAD_PROGRESS.lock() {
        map.clear();
    }

    let name_map: std::collections::HashMap<String, String> = manifest
        .mods
        .iter()
        .map(|m| (m.id.clone(), m.name.clone()))
        .collect();

    // Only surface mods that actually need a network fetch — already-present
    // jars would otherwise flood the progress UI on every update/install.
    let queue: Vec<ModDownloadProgressPayload> = manifest
        .mods
        .iter()
        .filter(|m| {
            only_mod_ids.map(|ids| ids.contains(&m.id)).unwrap_or(true)
                && mod_needs_download(&instance_dir, m)
        })
        .map(|m| ModDownloadProgressPayload {
            id: m.id.clone(),
            name: m.name.clone(),
            downloaded: 0,
            total: 0,
            percent: 0,
            status: "queued".to_string(),
        })
        .collect();

    let scope_mod_ids: Option<Vec<String>> = only_mod_ids.map(|ids| ids.iter().cloned().collect());

    // Nothing to fetch — still emit a quick start/done so any UI overlay can settle.
    if queue.is_empty() {
        let report = if let Some(ids) = only_mod_ids {
            tuffbox_core::ensure_project_mods_downloaded_with_progress_filtered(
                manifest,
                &instance_dir,
                &tuffbox_core::ProgressCallback::new(),
                Some(ids),
            )
        } else {
            tuffbox_core::ensure_project_mods_downloaded(manifest, &instance_dir)
        };
        if emit_lifecycle {
            let _ = app.emit(
                "mod-download-batch",
                serde_json::json!({
                    "phase": "start",
                    "items": Vec::<ModDownloadProgressPayload>::new(),
                    "scopeModIds": scope_mod_ids,
                }),
            );
            let _ = app.emit(
                "mod-download-batch",
                serde_json::json!({
                    "phase": "done",
                    "downloaded": report.downloaded,
                    "failed": report.failed,
                    "alreadyPresent": report.already_present,
                    "skipped": report.skipped,
                    "scopeModIds": scope_mod_ids,
                    "batchComplete": true,
                }),
            );
        }
        return report;
    }

    if emit_lifecycle {
        let _ = app.emit(
            "mod-download-batch",
            serde_json::json!({
                "phase": "start",
                "items": queue,
                "scopeModIds": scope_mod_ids,
            }),
        );
    }

    let app_for_cb = app.clone();
    let names_for_cb = name_map.clone();
    // Throttle: only emit when percent changes by >= 1 to avoid flooding the UI.
    let last_emitted: std::sync::Mutex<std::collections::HashMap<String, u32>> =
        std::sync::Mutex::new(std::collections::HashMap::new());

    let progress = tuffbox_core::ProgressCallback::with(move |id, done, total| {
        if let Ok(mut map) = DOWNLOAD_PROGRESS.lock() {
            map.insert(id.to_string(), (done, total));
        }
        let percent = if total > 0 {
            ((done as f64 / total as f64) * 100.0).round() as u32
        } else {
            0
        };
        let status = if total > 0 && done >= total {
            "done"
        } else {
            "downloading"
        };

        let should_emit = {
            let mut last = last_emitted.lock().unwrap_or_else(|e| e.into_inner());
            let prev = last.get(id).copied().unwrap_or(u32::MAX);
            if status == "done" || prev == u32::MAX || percent.abs_diff(prev) >= 1 {
                last.insert(id.to_string(), percent);
                true
            } else {
                false
            }
        };

        if should_emit {
            let name = names_for_cb
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.to_string());
            let _ = app_for_cb.emit(
                "mod-download-progress",
                ModDownloadProgressPayload {
                    id: id.to_string(),
                    name,
                    downloaded: done,
                    total,
                    percent,
                    status: status.to_string(),
                },
            );
        }
    });

    let report = tuffbox_core::ensure_project_mods_downloaded_with_progress_filtered(
        manifest,
        &instance_dir,
        &progress,
        only_mod_ids,
    );

    // Mark completed / failed items explicitly so the UI can settle bars.
    for id in &report.downloaded {
        let name = name_map.get(id).cloned().unwrap_or_else(|| id.clone());
        let _ = app.emit(
            "mod-download-progress",
            ModDownloadProgressPayload {
                id: id.clone(),
                name,
                downloaded: 1,
                total: 1,
                percent: 100,
                status: "done".to_string(),
            },
        );
    }
    for id in &report.already_present {
        let name = name_map.get(id).cloned().unwrap_or_else(|| id.clone());
        let _ = app.emit(
            "mod-download-progress",
            ModDownloadProgressPayload {
                id: id.clone(),
                name,
                downloaded: 1,
                total: 1,
                percent: 100,
                status: "skipped".to_string(),
            },
        );
    }
    for fail in &report.failed {
        let name = name_map
            .get(&fail.mod_id)
            .cloned()
            .unwrap_or_else(|| fail.mod_id.clone());
        let _ = app.emit(
            "mod-download-progress",
            ModDownloadProgressPayload {
                id: fail.mod_id.clone(),
                name,
                downloaded: 0,
                total: 0,
                percent: 0,
                status: "failed".to_string(),
            },
        );
    }

    if emit_lifecycle {
        let _ = app.emit(
            "mod-download-batch",
            serde_json::json!({
                "phase": "done",
                "downloaded": report.downloaded,
                "failed": report.failed,
                "alreadyPresent": report.already_present,
                "skipped": report.skipped,
                "scopeModIds": scope_mod_ids,
                "batchComplete": true,
            }),
        );
    }

    if let Ok(mut map) = DOWNLOAD_PROGRESS.lock() {
        map.clear();
    }

    report
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let _ = launcher_settings::load_launcher_settings();
            use tauri::Manager;
            if let Ok(resources) = app.path().resource_dir() {
                std::env::set_var("TUFFBOX_JEI_BRIDGE_DIR", resources.join("jei-bridge"));
            }
            // Size the window to the current screen resolution: 95% of the
            // monitor's width and 94% of its height, so it adapts to whatever
            // display the app is launched on (and re-applies on monitor change).
            fn fit_to_screen(win: &tauri::WebviewWindow) {
                if let Ok(Some(monitor)) = win.current_monitor() {
                    let size = monitor.size();
                    let (mw, mh) = (size.width as f64, size.height as f64);
                    let w = (mw * 0.95).max(1100.0);
                    let h = (mh * 0.94).max(700.0);
                    let _ = win.set_size(tauri::LogicalSize::new(w, h));
                    let _ = win.center();
                }
            }
            if let Some(win) = app.get_webview_window("main") {
                fit_to_screen(&win);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_project_schema_status,
            migrate_project_schema,
            validate_project,
            resolve_project_path,
            get_project_brief,
            update_project_brief,
            list_profiles,
            list_mods,
            sync_mods_folder,
            search_modrinth_mods,
            search_curseforge_mods,
            search_unified_mods,
            preview_modrinth_install,
            get_modrinth_project_icon,
            get_modrinth_project,
            get_catalog_project,
            get_catalog_versions,
            get_modrinth_pack_download,
            get_mod_user_state,
            set_mod_user_state,
            create_mod_list,
            delete_mod_list,
            rename_mod_list,
            add_to_mod_list,
            remove_from_mod_list,
            add_modrinth_mod,
            add_modrinth_mod_with_dependencies,
            add_modrinth_mods_with_dependencies,
            install_steam_bridge,
            create_mode_api::create_mode_chat,
            create_mode_api::assemble_pack_draft,
            create_mode_api::preview_pack_draft,
            create_mode_api::install_pack_draft,
            create_mode_api::list_create_chats,
            create_mode_api::save_create_chat,
            create_mode_api::load_create_chat,
            create_mode_api::delete_create_chat,
            create_mode_api::new_create_chat,
            add_curseforge_mod,
            remove_project_mod,
            disable_project_mod,
            enable_project_mod,
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
            refresh_graph,
            get_diagnostics,
            run_project_validation,
            check_mod_compatibility,
            compare_modpacks,
            create_project_backup,
            list_backups,
            delete_backup,
            build_ai_crash_context,
            analyze_crash_with_ai,
            apply_action_plan,
            record_crash_ai_feedback,
            save_authored_crash_case,
            draft_authored_crash_case,
            list_authored_crash_cases,
            get_authored_case_export,
            open_authored_kb_folder,
            swarm_api::get_pending_action_plan,
            swarm_api::clear_pending_network_plan,
            swarm_api::write_pending_network_plan,
            swarm_api::get_share_prompt_after_launch,
            swarm_api::dismiss_share_prompt,
            swarm_api::confirm_crash_resolution_after_launch,
            swarm_api::confirm_crash_resolution_from_diagnose,
            swarm_api::distill_resolved_crash_plan,
            swarm_api::publish_experience_capsule,
            swarm_api::list_community_crash_capsules,
            swarm_api::vote_community_crash_capsule,
            swarm_api::propose_community_capsule_plan,
            swarm_api::record_project_cooccurrence,
            swarm_api::report_mod_cooccurrence,
            swarm_api::get_local_cooccurrence,
            swarm_api::get_creation_trends,
            swarm_api::suggest_mods_from_trends,
            integrations::complete_swarm_onboarding,
            integrations::get_swarm_settings,
            integrations::set_swarm_enabled,
            integrations::set_swarm_share_prompts,
            integrations::set_swarm_hub_url,
            integrations::set_swarm_supabase_url,
            integrations::set_swarm_p2p,
            swarm_node::get_p2p_node_status,
            swarm_node::ensure_p2p_node,
            task_progress_api::list_background_tasks,
            task_progress_api::dismiss_background_task,
            task_progress_api::start_background_task,
            recommend_mods,
            get_mod_info,
            restore_backup,
            save_problematic_mods_config,
            get_problematic_mods_config,
            launch_server,
            generate_server_properties,
            scan_mod_recipes,
            get_item_icon,
            get_item_icons_batch,
            get_recipe_runtime_status,
            get_recipe_runtime_snapshot,
            write_kubejs_recipe_removes,
            write_kubejs_craft_recipe,
            write_kubejs_tag_edits,
            list_item_tags,
            get_item_tag_entries,
            generate_kubejs_recipe_script,
            load_quest_book,
            save_quest_chapter,
            validate_quest_book,
            list_worlds,
            list_content_packs,
            set_content_pack_enabled,
            list_mc_servers,
            add_mc_server,
            remove_mc_server,
            ping_mc_server,
            backup_world,
            save_as_template,
            list_templates,
            get_download_progress,
            get_keyboard_shortcuts,
            lint_config,
            cleanup_project,
            get_app_version,
            integrations::check_for_app_update,
            integrations::get_integration_status,
            integrations::save_integration_settings,
            integrations::set_integration_secret,
            integrations::clear_integration_secret,
            integrations::test_integration,
            integrations::list_ollama_models,
            integrations::detect_ollama,
            integrations::pull_ollama_model,
            integrations::import_ollama_gguf,
            integrations::ensure_ollama_model,
            integrations::get_publish_config,
            integrations::save_publish_config,
            integrations::publish_release,
            read_world_info,
            read_world_map,
            list_world_dimensions,
            delete_world_chunks,
            copy_world_chunks,
            paste_world_chunks,
            purge_world_regions,
            export_world_chunks,
            import_world_chunks,
            select_world_by_query,
            render_world_map_png,
            warm_world_map_cache,
            clear_world_map_cache,
            swap_world_chunks,
            change_world_chunks,
            read_chunk_editor,
            write_chunk_editor,
            filter_world_chunks_advanced,
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
            install_graph_dep,
            download_missing_files,
            get_crash_diagnosis,
            create_crash_fix_plan,
            apply_crash_fix_plan,
            apply_fix_action,
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
            search_curseforge_modpacks,
            get_curseforge_modpack_files,
            install_modpack,
            retry_failed_mod_downloads,
            has_crashed,
            open_project_folder,
            create_project_desktop_shortcut,
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
            share_log_mclogs,
            analyze_log_text,
            list_instance_logs,
            read_instance_log,
            get_instance_size,
            pin_project,
            is_project_pinned,
            set_last_opened_project,
            get_last_opened_project,
            update_project_settings,
            auth::mc_start_device_code,
            auth::mc_poll_device_code,
            auth::mc_get_auth_status,
            auth::mc_logout,
            auth::mc_refresh_profile,
            auth::mc_get_skin_path,
            auth::mc_fetch_skin_url,
            auth::mc_offline_login,
            auth::mc_fetch_skin_for_username,
            auth::mc_set_skin_source,
            auth::mc_list_accounts,
            auth::mc_switch_account,
            auth::mc_remove_account,
            auth::mc_apply_skin,
            auth::mc_upload_skin,
            auth::mc_upload_skin_file,
            auth::mc_apply_cape,
            auth::mc_list_capes,
            auth::mc_set_cape_provider,
            auth::mc_list_yggdrasil_presets,
            auth::mc_yggdrasil_login,
            auth::mc_check_entitlement,
            auth::mc_get_skin_base64,
            get_presence_settings,
            save_presence_settings,
            launcher_settings::get_launcher_settings,
            launcher_settings::save_launcher_settings_cmd,
            launcher_settings::get_runtime_path_info,
            launcher_settings::get_instances_path_info,
            launcher_settings::validate_runtime_path_cmd,
            launcher_settings::validate_instances_path_cmd,
            set_discord_presence,
            clear_discord_presence,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
