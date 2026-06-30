use std::path::{Path, PathBuf};
use tuffbox_core::{
    ContentProvider, DependencyGraph, ModSource, ModSpec, ProjectManifest, ProviderFileInfo,
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
            })
        })
        .collect();
    Ok(mods)
}

#[tauri::command(rename_all = "camelCase")]
async fn search_modrinth_mods(
    path: String,
    query: String,
) -> Result<Vec<tuffbox_core::ProjectInfo>, String> {
    tokio::task::spawn_blocking(move || {
        let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
        let provider = tuffbox_core::ModrinthProvider::new();
        provider
            .search(&ProviderSearchQuery {
                query: Some(query),
                minecraft_version: Some(manifest.minecraft.version.clone()),
                loader: Some(tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind).to_string()),
            })
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
    let manifest_path = PathBuf::from(&project_dir).join("project.tuffbox.json");
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

#[tauri::command]
fn generate_lockfile(path: String) -> Result<TuffboxLockfile, String> {
    let manifest = ProjectManifest::load_from_path(&path).map_err(|e| e.to_string())?;
    let graph = DependencyGraph::from_manifest(&manifest);
    Ok(TuffboxLockfile::from_manifest_and_graph(&manifest, &graph))
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
    }

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

fn manifest_parent(path: &str) -> Result<PathBuf, String> {
    PathBuf::from(path)
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "manifest has no parent directory".to_string())
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
        "json" | "json5" | "toml" | "properties" | "cfg" | "conf" | "txt" | "js" | "zs" | "yaml" | "yml"
    )
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
    let mod_spec = build_mod_spec(&project, &version, file, dependencies, parse_side(side.as_deref()));
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

fn parse_side(side: Option<&str>) -> Side {
    match side {
        Some("client") => Side::Client,
        Some("server") => Side::Server,
        Some("both") => Side::Both,
        _ => Side::Both,
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
            list_mods,
            search_modrinth_mods,
            add_modrinth_mod,
            remove_project_mod,
            update_project_mod,
            list_config_files,
            read_config_file,
            write_config_file,
            get_graph,
            get_diagnostics,
            get_project_dir,
            list_snapshots,
            create_snapshot,
            generate_lockfile,
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
