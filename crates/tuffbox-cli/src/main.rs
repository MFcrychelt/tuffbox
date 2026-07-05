use anyhow::Context;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tuffbox_core::{
    ContentProvider, DependencyGraph, ModSource, ModSpec, ProjectManifest, ProviderFileInfo,
    ProviderSearchQuery, Resolver, Side, Snapshot, SnapshotStore, SourceKind, TuffboxLockfile,
};

#[derive(Debug, Parser)]
#[command(name = "tuffbox")]
#[command(about = "TuffBox IDE developer harness", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Project manifest commands.
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Dependency graph commands.
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    /// Resolver commands.
    Resolve { manifest: PathBuf },
    /// Snapshot commands.
    Snapshot {
        #[command(subcommand)]
        command: SnapshotCommand,
    },
    /// Modrinth provider commands.
    Modrinth {
        #[command(subcommand)]
        command: ModrinthCommand,
    },
    /// Launch a test instance.
    Launch {
        manifest: PathBuf,
        #[arg(short, long)]
        profile: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    /// Validate a TuffBox project manifest.
    Validate { manifest: PathBuf },
    /// Create and print lockfile JSON.
    Lock { manifest: PathBuf },
    /// Add a mod from Modrinth to the project manifest.
    AddMod {
        manifest: PathBuf,
        mod_id: String,
        #[arg(short, long)]
        side: Option<String>,
    },
    /// Remove a mod from the project manifest.
    RemoveMod { manifest: PathBuf, mod_id: String },
    /// Update a mod in the project manifest to the latest compatible version.
    UpdateMod { manifest: PathBuf, mod_id: String },
}

#[derive(Debug, Subcommand)]
enum GraphCommand {
    /// Print graph as JSON.
    Print { manifest: PathBuf },
    /// Print diagnostics as JSON.
    Diagnostics { manifest: PathBuf },
}

#[derive(Debug, Subcommand)]
enum SnapshotCommand {
    /// Create a snapshot of the current project state.
    Create {
        project_dir: PathBuf,
        #[arg(short, long)]
        name: String,
        #[arg(short, long, default_value = "manual")]
        reason: String,
        #[arg(short, long)]
        manifest: Option<PathBuf>,
    },
    /// List existing snapshots.
    List { project_dir: PathBuf },
    /// Show diff between two snapshots.
    Diff {
        project_dir: PathBuf,
        from: String,
        to: String,
    },
    /// Rollback project to a snapshot.
    Rollback { project_dir: PathBuf, id: String },
}

#[derive(Debug, Subcommand)]
enum ModrinthCommand {
    /// Search for projects on Modrinth.
    Search {
        query: String,
        #[arg(long)]
        mc: Option<String>,
        #[arg(long)]
        loader: Option<String>,
    },
    /// Show available versions for a project.
    Versions {
        project_id: String,
        #[arg(long)]
        mc: Option<String>,
        #[arg(long)]
        loader: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Project { command } => match command {
            ProjectCommand::Validate { manifest } => {
                let manifest = load_manifest(manifest)?;
                manifest.validate_basic()?;
                println!("OK: {} {}", manifest.project.name, manifest.project.version);
            }
            ProjectCommand::Lock { manifest } => {
                let manifest = load_manifest(manifest)?;
                manifest.validate_basic()?;
                let graph = DependencyGraph::from_manifest(&manifest);
                let lockfile = TuffboxLockfile::from_manifest_and_graph(&manifest, &graph);
                println!("{}", serde_json::to_string_pretty(&lockfile)?);
            }
            ProjectCommand::AddMod {
                manifest,
                mod_id,
                side,
            } => {
                let manifest_path = manifest.clone();
                let snapshot = auto_snapshot(&manifest_path, "add-mod")?;
                let mut manifest = load_manifest(manifest_path.clone())?;
                add_mod_from_modrinth(&mut manifest, &mod_id, side)?;
                save_manifest(&manifest_path, &manifest)?;
                download_project_mods(&manifest_path, &manifest);
                println!(
                    "Added mod {} to {} (snapshot {})",
                    mod_id,
                    manifest_path.display(),
                    snapshot.id
                );
            }
            ProjectCommand::RemoveMod { manifest, mod_id } => {
                let manifest_path = manifest.clone();
                let snapshot = auto_snapshot(&manifest_path, "remove-mod")?;
                let mut manifest = load_manifest(manifest_path.clone())?;
                let removed = manifest.mods.iter().find(|m| m.id == mod_id).cloned();
                remove_mod(&mut manifest, &mod_id)?;
                save_manifest(&manifest_path, &manifest)?;
                if let Some(removed) = removed {
                    if let Some(file_name) = removed.file_name {
                        if let Some(instance_dir) = tuffbox_core::instance_dir_for_manifest(&manifest_path) {
                            let content_dir = tuffbox_core::content_dir_for(&instance_dir, removed.content_type);
                            let _ = std::fs::remove_file(content_dir.join(file_name));
                        }
                    }
                }
                println!(
                    "Removed mod {} from {} (snapshot {})",
                    mod_id,
                    manifest_path.display(),
                    snapshot.id
                );
            }
            ProjectCommand::UpdateMod { manifest, mod_id } => {
                let manifest_path = manifest.clone();
                let snapshot = auto_snapshot(&manifest_path, "update-mod")?;
                let mut manifest = load_manifest(manifest_path.clone())?;
                update_mod_from_modrinth(&mut manifest, &mod_id)?;
                save_manifest(&manifest_path, &manifest)?;
                download_project_mods(&manifest_path, &manifest);
                println!(
                    "Updated mod {} in {} (snapshot {})",
                    mod_id,
                    manifest_path.display(),
                    snapshot.id
                );
            }
        },
        Command::Graph { command } => match command {
            GraphCommand::Print { manifest } => {
                let manifest = load_manifest(manifest)?;
                let graph = DependencyGraph::from_manifest(&manifest);
                println!("{}", serde_json::to_string_pretty(&graph)?);
            }
            GraphCommand::Diagnostics { manifest } => {
                let manifest = load_manifest(manifest)?;
                let graph = DependencyGraph::from_manifest(&manifest);
                let diagnostics = Resolver::analyze_project(&manifest, &graph);
                println!("{}", serde_json::to_string_pretty(&diagnostics)?);
            }
        },
        Command::Resolve { manifest } => {
            let manifest = load_manifest(manifest)?;
            let graph = DependencyGraph::from_manifest(&manifest);
            let diagnostics = Resolver::analyze_project(&manifest, &graph);
            let plan = Resolver::create_fix_plan(&graph, &diagnostics);
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
        Command::Snapshot { command } => match command {
            SnapshotCommand::Create {
                project_dir,
                name,
                reason,
                manifest,
            } => {
                let manifest_path =
                    manifest.unwrap_or_else(|| project_dir.join("project.tuffbox.json"));
                let lockfile_path = manifest_path.with_extension("lock.json");
                let lockfile_path = if lockfile_path.exists() {
                    Some(lockfile_path)
                } else {
                    None
                };
                let store = SnapshotStore::new(&project_dir);
                let snapshot = store.create(
                    &name,
                    &reason,
                    &manifest_path,
                    lockfile_path.as_ref(),
                    &[] as &[&PathBuf],
                )?;
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            }
            SnapshotCommand::List { project_dir } => {
                let store = SnapshotStore::new(&project_dir);
                let snapshots = store.list()?;
                println!("{}", serde_json::to_string_pretty(&snapshots)?);
            }
            SnapshotCommand::Diff {
                project_dir,
                from,
                to,
            } => {
                let store = SnapshotStore::new(&project_dir);
                let diff = store.diff(&from, &to)?;
                println!("{}", serde_json::to_string_pretty(&diff)?);
            }
            SnapshotCommand::Rollback { project_dir, id } => {
                let store = SnapshotStore::new(&project_dir);
                let snapshot = store.rollback(&id)?;
                println!("Rolled back to snapshot {}", snapshot.id);
            }
        },
        Command::Modrinth { command } => match command {
            ModrinthCommand::Search { query, mc, loader } => {
                let provider = tuffbox_core::ModrinthProvider::new();
                let results = provider.search(&ProviderSearchQuery {
                    query: Some(query),
                    minecraft_version: mc,
                    loader,
                    ..Default::default()
                })?;
                println!("{}", serde_json::to_string_pretty(&results)?);
            }
            ModrinthCommand::Versions {
                project_id,
                mc,
                loader,
            } => {
                let provider = tuffbox_core::ModrinthProvider::new();
                let versions = provider.get_versions(
                    &project_id,
                    &ProviderSearchQuery {
                        query: None,
                        minecraft_version: mc,
                        loader,
                        ..Default::default()
                    },
                )?;
                println!("{}", serde_json::to_string_pretty(&versions)?);
            }
        },
        Command::Launch { manifest, profile } => {
            let manifest_path = manifest.clone();
            let manifest = load_manifest(manifest)?;
            let profile_id = profile.unwrap_or_else(|| "client".to_string());
            let profile = manifest
                .profiles
                .iter()
                .find(|p| p.id == profile_id)
                .with_context(|| format!("profile {profile_id} not found"))?;

            let java = if let Some(java_path) = manifest.java.as_ref().and_then(|j| j.path.clone()) {
                tuffbox_core::jre::check_java_at_path(&PathBuf::from(&java_path))?
            } else {
                tuffbox_core::TestLauncher::find_java_for_minecraft(&manifest.minecraft.version)?
            };
            let game_dir = manifest_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let launcher_dir = dirs::data_dir()
                .or_else(dirs::home_dir)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("TuffBox");
            std::fs::create_dir_all(&launcher_dir)?;

            let progress = tuffbox_core::mc_install::InstallProgress {
                log_path: game_dir.join("logs").join("latest.log"),
            };

            // Same safety net as the desktop launcher: verify every
            // manifest-declared mod actually has its jar on disk before we
            // launch, so `tuffbox launch` never silently starts vanilla.
            let sync_report = tuffbox_core::ensure_project_mods_downloaded(&manifest, &game_dir);
            if !sync_report.downloaded.is_empty() {
                println!("Downloaded {} missing mod file(s).", sync_report.downloaded.len());
            }
            for failure in &sync_report.failed {
                eprintln!("warning: failed to prepare mod '{}': {}", failure.mod_id, failure.error);
            }

            let options = tuffbox_core::LaunchOptions {
                profile_id: profile_id.clone(),
                instance_dir: game_dir,
                memory_mb: profile.memory_mb.unwrap_or(4096),
                jvm_args: profile.jvm_args.clone(),
            };
            let (mut cmd, log_path) =
                tuffbox_core::TestLauncher::build_command(&manifest, profile, &options, &java, &launcher_dir, &progress)?;
            let mut child = cmd.spawn()?;
            let status = child.wait()?;
            let result = tuffbox_core::LaunchResult {
                exit_code: status.code(),
                log_path,
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }

    Ok(())
}

fn add_mod_from_modrinth(
    manifest: &mut ProjectManifest,
    mod_id: &str,
    side: Option<String>,
) -> anyhow::Result<()> {
    let provider = tuffbox_core::ModrinthProvider::new();
    let project = provider.get_project(mod_id)?;

    if manifest.mods.iter().any(|m| m.id == project.slug) {
        anyhow::bail!("mod {} is already in the project", project.slug);
    }

    let query = ProviderSearchQuery {
        query: None,
        minecraft_version: Some(manifest.minecraft.version.clone()),
        loader: Some(loader_slug(&manifest.loader.kind)),
        ..Default::default()
    };
    let versions = provider.get_versions(mod_id, &query)?;
    let version = versions
        .into_iter()
        .next()
        .with_context(|| format!("no compatible version found for {mod_id}"))?;

    let file = ProviderFileInfo::primary_file(&version)
        .cloned()
        .with_context(|| format!("no primary file for version {}", version.id))?;

    let dependencies = provider.resolve_dependencies(&version.id)?;

    let side = parse_side(side.as_deref());

    let mod_spec = build_mod_spec(&project, &version, file, dependencies, side);
    manifest.mods.push(mod_spec);
    Ok(())
}

fn remove_mod(manifest: &mut ProjectManifest, mod_id: &str) -> anyhow::Result<()> {
    let original_len = manifest.mods.len();
    manifest.mods.retain(|m| m.id != mod_id);
    if manifest.mods.len() == original_len {
        anyhow::bail!("mod {} not found in project", mod_id);
    }
    Ok(())
}

fn update_mod_from_modrinth(manifest: &mut ProjectManifest, mod_id: &str) -> anyhow::Result<()> {
    let provider = tuffbox_core::ModrinthProvider::new();

    let index = manifest
        .mods
        .iter()
        .position(|m| m.id == mod_id)
        .with_context(|| format!("mod {mod_id} not found in project"))?;

    let project_id = manifest.mods[index]
        .source
        .project_id
        .clone()
        .unwrap_or_else(|| mod_id.to_string());
    let project = provider.get_project(&project_id)?;

    let query = ProviderSearchQuery {
        query: None,
        minecraft_version: Some(manifest.minecraft.version.clone()),
        loader: Some(loader_slug(&manifest.loader.kind)),
        ..Default::default()
    };
    let versions = provider.get_versions(&project_id, &query)?;
    let version = versions
        .into_iter()
        .next()
        .with_context(|| format!("no compatible version found for {project_id}"))?;

    let file = ProviderFileInfo::primary_file(&version)
        .cloned()
        .with_context(|| format!("no primary file for version {}", version.id))?;

    let dependencies = provider.resolve_dependencies(&version.id)?;
    let side = manifest.mods[index].side;

    let mod_spec = build_mod_spec(&project, &version, file, dependencies, side);
    manifest.mods[index] = mod_spec;
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
        content_type: tuffbox_core::manifest::ContentType::from_modrinth_project_type(&project.project_type),
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
    let project_dir = manifest_path
        .parent()
        .with_context(|| format!("manifest path has no parent: {}", manifest_path.display()))?;
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
        &[] as &[&PathBuf],
    )?)
}

fn save_manifest(path: &Path, manifest: &ProjectManifest) -> anyhow::Result<()> {
    let updated = serde_json::to_string_pretty(manifest)?;
    std::fs::write(path, updated)?;
    Ok(())
}

/// Downloads every manifest-declared entry that isn't already present with
/// a matching hash into its content-type-appropriate folder. Best-effort:
/// failures are printed but don't abort the CLI command, since the
/// manifest write already succeeded and diagnostics/graph will still flag
/// missing files.
fn download_project_mods(manifest_path: &Path, manifest: &ProjectManifest) {
    let instance_dir = tuffbox_core::instance_dir_for_manifest(manifest_path)
        .unwrap_or_else(|| manifest_path.parent().map(|p| p.to_path_buf()).unwrap_or_default());
    let report = tuffbox_core::ensure_project_mods_downloaded(manifest, &instance_dir);
    for failure in &report.failed {
        eprintln!("warning: failed to download mod {}: {}", failure.mod_id, failure.error);
    }
}

fn loader_slug(kind: &tuffbox_core::LoaderKind) -> String {
    tuffbox_core::graph::loader_kind_slug(kind).to_string()
}

fn load_manifest(path: PathBuf) -> anyhow::Result<ProjectManifest> {
    ProjectManifest::load_from_path(&path)
        .with_context(|| format!("failed to load manifest {}", path.display()))
}
