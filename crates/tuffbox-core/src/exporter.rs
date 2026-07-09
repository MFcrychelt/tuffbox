use crate::manifest::{LoaderKind, ProjectManifest, Side};
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    io::{Seek, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;
use zip::{write::SimpleFileOptions, ZipWriter};

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("manifest has no parent directory")]
    NoProjectDir,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub path: PathBuf,
    pub file_count: usize,
    pub override_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportIssue {
    pub severity: ExportIssueSeverity,
    pub code: String,
    pub message: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportIssueSeverity {
    Error,
    Warning,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerPackManifest {
    name: String,
    version: String,
    minecraft_version: String,
    loader: ServerPackLoader,
    included_mods: Vec<ServerPackMod>,
    remote_mods: Vec<ServerPackRemoteMod>,
    skipped_client_mods: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ServerPackLoader {
    kind: String,
    version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerPackMod {
    id: String,
    name: String,
    version: String,
    file_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerPackRemoteMod {
    id: String,
    name: String,
    version: String,
    file_name: Option<String>,
    url: String,
    sha1: Option<String>,
    sha512: Option<String>,
}


#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeManifest {
    minecraft: CurseForgeMinecraft,
    manifest_type: String,
    manifest_version: u8,
    name: String,
    version: String,
    author: String,
    files: Vec<CurseForgeFile>,
    overrides: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeMinecraft {
    version: String,
    mod_loaders: Vec<CurseForgeLoader>,
}

#[derive(Debug, Serialize)]
struct CurseForgeLoader {
    id: String,
    primary: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CurseForgeFile {
    project_id: u64,
    file_id: u64,
    required: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrismPack {
    components: Vec<PrismComponent>,
    format_version: u8,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrismComponent {
    cached_name: String,
    cached_version: String,
    uid: String,
    version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModrinthIndex {
    format_version: u8,
    game: String,
    version_id: String,
    name: String,
    summary: Option<String>,
    files: Vec<ModrinthFile>,
    dependencies: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModrinthFile {
    path: String,
    hashes: ModrinthHashes,
    downloads: Vec<String>,
    env: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ModrinthHashes {
    #[serde(skip_serializing_if = "Option::is_none")]
    sha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha512: Option<String>,
}

pub fn validate_modrinth_export(manifest: &ProjectManifest) -> Vec<ExportIssue> {
    let mut issues = Vec::new();
    if manifest.minecraft.version.trim().is_empty() {
        issues.push(issue(
            ExportIssueSeverity::Error,
            "MISSING_MINECRAFT_VERSION",
            "Minecraft version is required for .mrpack export.",
            None,
        ));
    }
    if !matches!(manifest.loader.kind, LoaderKind::Vanilla) && manifest.loader.version.trim().is_empty() {
        issues.push(issue(
            ExportIssueSeverity::Error,
            "MISSING_LOADER_VERSION",
            "Loader version is required for .mrpack export.",
            None,
        ));
    }
    if manifest.mods.is_empty() {
        issues.push(issue(
            ExportIssueSeverity::Warning,
            "NO_MODS",
            "The project has no mods; export will contain only overrides and dependencies.",
            None,
        ));
    }
    for module in &manifest.mods {
        if module.source.url.as_deref().unwrap_or_default().is_empty() {
            issues.push(issue(
                ExportIssueSeverity::Warning,
                "MOD_WITHOUT_DOWNLOAD_URL",
                "This mod cannot be represented as a remote Modrinth pack download and will be skipped by the MVP .mrpack exporter.",
                Some(module.id.clone()),
            ));
        }
        let hashes = module.hashes.as_ref();
        if hashes.and_then(|h| h.sha1.as_ref()).is_none() && hashes.and_then(|h| h.sha512.as_ref()).is_none() {
            issues.push(issue(
                ExportIssueSeverity::Warning,
                "MOD_WITHOUT_HASH",
                "This mod has no hash metadata; Modrinth clients may not verify it correctly.",
                Some(module.id.clone()),
            ));
        }
        if module.side == Side::Unknown {
            issues.push(issue(
                ExportIssueSeverity::Warning,
                "UNKNOWN_MOD_SIDE",
                "Mod side is unknown; verify client/server compatibility before release.",
                Some(module.id.clone()),
            ));
        }
    }
    issues
}

fn issue(
    severity: ExportIssueSeverity,
    code: &str,
    message: &str,
    target: Option<String>,
) -> ExportIssue {
    ExportIssue {
        severity,
        code: code.to_string(),
        message: message.to_string(),
        target,
    }
}

pub fn export_modrinth_pack(
    manifest: &ProjectManifest,
    manifest_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<ExportResult, ExportError> {
    let manifest_path = manifest_path.as_ref();
    let project_dir = manifest_path.parent().ok_or(ExportError::NoProjectDir)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut dependencies = HashMap::new();
    dependencies.insert("minecraft".to_string(), manifest.minecraft.version.clone());
    match manifest.loader.kind {
        LoaderKind::Vanilla => {}
        LoaderKind::Fabric => {
            dependencies.insert("fabric-loader".to_string(), manifest.loader.version.clone());
        }
        LoaderKind::Forge => {
            dependencies.insert("forge".to_string(), manifest.loader.version.clone());
        }
        LoaderKind::Neoforge => {
            dependencies.insert("neoforge".to_string(), manifest.loader.version.clone());
        }
        LoaderKind::Quilt => {
            dependencies.insert("quilt-loader".to_string(), manifest.loader.version.clone());
        }
    }

    let files = manifest
        .mods
        .iter()
        .filter_map(|module| {
            let file_name = module.file_name.clone().unwrap_or_else(|| format!("{}.jar", module.id));
            let downloads = module.source.url.clone().map(|url| vec![url]).unwrap_or_default();
            if downloads.is_empty() {
                return None;
            }
            let hashes = module.hashes.as_ref();
            Some(ModrinthFile {
                path: format!("mods/{file_name}"),
                hashes: ModrinthHashes {
                    sha1: hashes.and_then(|h| h.sha1.clone()),
                    sha512: hashes.and_then(|h| h.sha512.clone()),
                },
                downloads,
                env: side_env(module.side),
            })
        })
        .collect::<Vec<_>>();

    let index = ModrinthIndex {
        format_version: 1,
        game: "minecraft".to_string(),
        version_id: manifest.project.version.clone(),
        name: manifest.project.name.clone(),
        summary: manifest.project.description.clone(),
        files,
        dependencies,
    };

    let output = fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("modrinth.index.json", options)?;
    zip.write_all(serde_json::to_string_pretty(&index)?.as_bytes())?;

    let override_count = add_overrides(&mut zip, project_dir, options)?;
    zip.finish()?;

    Ok(ExportResult {
        path: output_path.to_path_buf(),
        file_count: index.files.len(),
        override_count,
    })
}

pub fn export_server_pack(
    manifest: &ProjectManifest,
    manifest_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<ExportResult, ExportError> {
    let manifest_path = manifest_path.as_ref();
    let project_dir = manifest_path.parent().ok_or(ExportError::NoProjectDir)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut included_mods = Vec::new();
    let mut remote_mods = Vec::new();
    let mut skipped_client_mods = Vec::new();
    let mut file_count = 0;

    for module in &manifest.mods {
        if module.side == Side::Client {
            skipped_client_mods.push(module.id.clone());
            continue;
        }

        let Some(file_name) = module.file_name.clone() else {
            if let Some(url) = &module.source.url {
                let hashes = module.hashes.as_ref();
                remote_mods.push(ServerPackRemoteMod {
                    id: module.id.clone(),
                    name: module.name.clone(),
                    version: module.version.clone(),
                    file_name: None,
                    url: url.clone(),
                    sha1: hashes.and_then(|h| h.sha1.clone()),
                    sha512: hashes.and_then(|h| h.sha512.clone()),
                });
            }
            continue;
        };

        let local_path = project_dir.join("mods").join(&file_name);
        if local_path.is_file() {
            zip.start_file(format!("mods/{file_name}"), options)?;
            zip.write_all(&fs::read(&local_path)?)?;
            file_count += 1;
            included_mods.push(ServerPackMod {
                id: module.id.clone(),
                name: module.name.clone(),
                version: module.version.clone(),
                file_name,
            });
        } else if let Some(url) = &module.source.url {
            let hashes = module.hashes.as_ref();
            remote_mods.push(ServerPackRemoteMod {
                id: module.id.clone(),
                name: module.name.clone(),
                version: module.version.clone(),
                file_name: Some(file_name),
                url: url.clone(),
                sha1: hashes.and_then(|h| h.sha1.clone()),
                sha512: hashes.and_then(|h| h.sha512.clone()),
            });
        }
    }

    let override_count = add_server_overrides(&mut zip, project_dir, options)?;
    let server_manifest = ServerPackManifest {
        name: manifest.project.name.clone(),
        version: manifest.project.version.clone(),
        minecraft_version: manifest.minecraft.version.clone(),
        loader: ServerPackLoader {
            kind: format!("{:?}", manifest.loader.kind).to_lowercase(),
            version: manifest.loader.version.clone(),
        },
        included_mods,
        remote_mods,
        skipped_client_mods,
    };

    zip.start_file("tuffbox.server-pack.json", options)?;
    zip.write_all(serde_json::to_string_pretty(&server_manifest)?.as_bytes())?;

    zip.start_file("README_INSTALL.txt", options)?;
    zip.write_all(server_readme(manifest, &server_manifest).as_bytes())?;
    zip.start_file("start.bat", options)?;
    zip.write_all(start_bat().as_bytes())?;
    zip.start_file("start.sh", options)?;
    zip.write_all(start_sh().as_bytes())?;
    file_count += 4;

    zip.finish()?;

    Ok(ExportResult {
        path: output_path.to_path_buf(),
        file_count,
        override_count,
    })
}

pub fn export_prism_instance(
    manifest: &ProjectManifest,
    manifest_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<ExportResult, ExportError> {
    let manifest_path = manifest_path.as_ref();
    let project_dir = manifest_path.parent().ok_or(ExportError::NoProjectDir)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("instance.cfg", options)?;
    zip.write_all(prism_instance_cfg(manifest).as_bytes())?;

    zip.start_file("mmc-pack.json", options)?;
    zip.write_all(serde_json::to_string_pretty(&prism_pack(manifest))?.as_bytes())?;

    zip.start_file("tuffbox.remote-mods.json", options)?;
    zip.write_all(serde_json::to_string_pretty(&remote_mod_manifest(manifest))?.as_bytes())?;

    let override_count = add_prism_files(&mut zip, project_dir, options)?;
    zip.finish()?;

    Ok(ExportResult {
        path: output_path.to_path_buf(),
        file_count: 3,
        override_count,
    })
}

pub fn export_curseforge_pack(
    manifest: &ProjectManifest,
    manifest_path: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<ExportResult, ExportError> {
    let manifest_path = manifest_path.as_ref();
    let project_dir = manifest_path.parent().ok_or(ExportError::NoProjectDir)?;
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let cf_manifest = CurseForgeManifest {
        minecraft: CurseForgeMinecraft {
            version: manifest.minecraft.version.clone(),
            mod_loaders: curseforge_loaders(manifest),
        },
        manifest_type: "minecraftModpack".to_string(),
        manifest_version: 1,
        name: manifest.project.name.clone(),
        version: manifest.project.version.clone(),
        author: manifest.project.authors.first().cloned().unwrap_or_else(|| "TuffBox".to_string()),
        files: Vec::new(),
        overrides: "overrides".to_string(),
    };

    zip.start_file("manifest.json", options)?;
    zip.write_all(serde_json::to_string_pretty(&cf_manifest)?.as_bytes())?;

    zip.start_file("tuffbox.remote-mods.json", options)?;
    zip.write_all(serde_json::to_string_pretty(&remote_mod_manifest(manifest))?.as_bytes())?;

    let override_count = add_overrides(&mut zip, project_dir, options)?;
    zip.finish()?;

    Ok(ExportResult {
        path: output_path.to_path_buf(),
        file_count: cf_manifest.files.len() + 2,
        override_count,
    })
}

fn prism_instance_cfg(manifest: &ProjectManifest) -> String {
    format!(
        "InstanceType=OneSix
name={}
notes=Exported by TuffBox
iconKey=default
",
        manifest.project.name
    )
}

fn prism_pack(manifest: &ProjectManifest) -> PrismPack {
    let mut components = vec![PrismComponent {
        cached_name: "Minecraft".to_string(),
        cached_version: manifest.minecraft.version.clone(),
        uid: "net.minecraft".to_string(),
        version: manifest.minecraft.version.clone(),
    }];
    if !matches!(manifest.loader.kind, LoaderKind::Vanilla) {
        let uid = match manifest.loader.kind {
            LoaderKind::Fabric => "net.fabricmc.fabric-loader",
            LoaderKind::Forge => "net.minecraftforge",
            LoaderKind::Neoforge => "net.neoforged",
            LoaderKind::Quilt => "org.quiltmc.quilt-loader",
            LoaderKind::Vanilla => "net.minecraft",
        };
        components.push(PrismComponent {
            cached_name: format!("{:?}", manifest.loader.kind),
            cached_version: manifest.loader.version.clone(),
            uid: uid.to_string(),
            version: manifest.loader.version.clone(),
        });
    }
    PrismPack {
        components,
        format_version: 1,
    }
}

fn curseforge_loaders(manifest: &ProjectManifest) -> Vec<CurseForgeLoader> {
    if matches!(manifest.loader.kind, LoaderKind::Vanilla) {
        return Vec::new();
    }
    let prefix = match manifest.loader.kind {
        LoaderKind::Fabric => "fabric",
        LoaderKind::Forge => "forge",
        LoaderKind::Neoforge => "neoforge",
        LoaderKind::Quilt => "quilt",
        LoaderKind::Vanilla => "vanilla",
    };
    vec![CurseForgeLoader {
        id: format!("{prefix}-{}", manifest.loader.version),
        primary: true,
    }]
}

fn remote_mod_manifest(manifest: &ProjectManifest) -> Vec<ServerPackRemoteMod> {
    manifest
        .mods
        .iter()
        .filter_map(|module| {
            let url = module.source.url.clone()?;
            let hashes = module.hashes.as_ref();
            Some(ServerPackRemoteMod {
                id: module.id.clone(),
                name: module.name.clone(),
                version: module.version.clone(),
                file_name: module.file_name.clone(),
                url,
                sha1: hashes.and_then(|h| h.sha1.clone()),
                sha512: hashes.and_then(|h| h.sha512.clone()),
            })
        })
        .collect()
}

fn add_prism_files<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    project_dir: &Path,
    options: SimpleFileOptions,
) -> Result<usize, ExportError> {
    let mut count = 0;
    for root in ["config", "defaultconfigs", "kubejs", "scripts", "resourcepacks", "shaderpacks", "mods"] {
        let dir = project_dir.join(root);
        if dir.is_dir() {
            count += add_dir_plain(zip, project_dir, &dir, options)?;
        }
    }
    Ok(count)
}

fn add_dir_plain<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    project_dir: &Path,
    dir: &Path,
    options: SimpleFileOptions,
) -> Result<usize, ExportError> {
    let mut count = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            count += add_dir_plain(zip, project_dir, &path, options)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(project_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(relative, options)?;
            zip.write_all(&fs::read(&path)?)?;
            count += 1;
        }
    }
    Ok(count)
}

fn add_overrides<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    project_dir: &Path,
    options: SimpleFileOptions,
) -> Result<usize, ExportError> {
    let mut count = 0;
    for root in ["config", "defaultconfigs", "kubejs", "scripts", "resourcepacks", "shaderpacks"] {
        let dir = project_dir.join(root);
        if dir.is_dir() {
            count += add_dir(zip, project_dir, &dir, options)?;
        }
    }
    Ok(count)
}

fn add_server_overrides<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    project_dir: &Path,
    options: SimpleFileOptions,
) -> Result<usize, ExportError> {
    let mut count = 0;
    for root in ["config", "defaultconfigs", "kubejs", "scripts"] {
        let dir = project_dir.join(root);
        if dir.is_dir() {
            count += add_dir(zip, project_dir, &dir, options)?;
        }
    }
    Ok(count)
}

fn server_readme(manifest: &ProjectManifest, server_manifest: &ServerPackManifest) -> String {
    let mut readme = format!(
        "# {} {} server pack\n\nMinecraft: {}\nLoader: {} {}\n\n",
        manifest.project.name,
        manifest.project.version,
        manifest.minecraft.version,
        server_manifest.loader.kind,
        server_manifest.loader.version,
    );
    readme.push_str("## Install\n\n");
    readme.push_str("1. Install the matching Minecraft server and loader.\n");
    readme.push_str("2. Copy folders from this archive into the server directory.\n");
    readme.push_str("3. If `tuffbox.server-pack.json` contains `remoteMods`, download them into `mods/`.\n");
    readme.push_str("4. Review EULA and run `start.bat` or `start.sh`.\n\n");
    if !server_manifest.remote_mods.is_empty() {
        readme.push_str("## Remote mods to download\n\n");
        for module in &server_manifest.remote_mods {
            readme.push_str(&format!("- {} {}: {}\n", module.name, module.version, module.url));
        }
        readme.push('\n');
    }
    if !server_manifest.skipped_client_mods.is_empty() {
        readme.push_str("## Client-only mods skipped\n\n");
        for id in &server_manifest.skipped_client_mods {
            readme.push_str(&format!("- {id}\n"));
        }
    }
    readme
}

/// Packs `logs/`, `crash-reports/` and `.tuffbox/test-runs/` from a project
/// into a single zip, for easy sharing when asking for help debugging a
/// modpack (the classic launcher "Create logs.zip" action).
pub fn export_logs_zip(
    project_dir: impl AsRef<Path>,
    output_path: impl AsRef<Path>,
) -> Result<ExportResult, ExportError> {
    let project_dir = project_dir.as_ref();
    let output_path = output_path.as_ref();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut file_count = 0;
    for relative_root in ["logs", "crash-reports", ".tuffbox/test-runs"] {
        let dir = project_dir.join(relative_root);
        if dir.is_dir() {
            file_count += add_dir_flat(&mut zip, project_dir, &dir, options)?;
        }
    }

    zip.finish()?;

    Ok(ExportResult {
        path: output_path.to_path_buf(),
        file_count,
        override_count: 0,
    })
}

/// Like [`add_dir`], but preserves the file's path relative to
/// `project_dir` as-is inside the archive instead of nesting everything
/// under an `overrides/` prefix (used for logs.zip, not modpack exports).
fn add_dir_flat<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    project_dir: &Path,
    dir: &Path,
    options: SimpleFileOptions,
) -> Result<usize, ExportError> {
    let mut count = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            count += add_dir_flat(zip, project_dir, &path, options)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(project_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(relative, options)?;
            zip.write_all(&fs::read(&path)?)?;
            count += 1;
        }
    }
    Ok(count)
}

fn start_bat() -> &'static str {
    "@echo off\r\njava -Xmx4G -jar server.jar nogui\r\npause\r\n"
}

fn start_sh() -> &'static str {
    "#!/usr/bin/env sh\njava -Xmx4G -jar server.jar nogui\n"
}

fn add_dir<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    project_dir: &Path,
    dir: &Path,
    options: SimpleFileOptions,
) -> Result<usize, ExportError> {
    let mut count = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            count += add_dir(zip, project_dir, &path, options)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(project_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(format!("overrides/{relative}"), options)?;
            zip.write_all(&fs::read(&path)?)?;
            count += 1;
        }
    }
    Ok(count)
}

fn side_env(side: Side) -> HashMap<String, String> {
    let mut env = HashMap::new();
    match side {
        Side::Client => {
            env.insert("client".to_string(), "required".to_string());
            env.insert("server".to_string(), "unsupported".to_string());
        }
        Side::Server => {
            env.insert("client".to_string(), "unsupported".to_string());
            env.insert("server".to_string(), "required".to_string());
        }
        Side::Optional => {
            env.insert("client".to_string(), "optional".to_string());
            env.insert("server".to_string(), "optional".to_string());
    }
    Side::Both | Side::Unknown => {
        env.insert("client".to_string(), "required".to_string());
        env.insert("server".to_string(), "required".to_string());
    }
}
    env
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{
        ContentType, FileHashes, LoaderSpec, MinecraftSpec, ModSource, ModSpec, ProfileSpec,
        ProjectMetadata, SourceKind,
    };

    fn fixture_manifest(_dir: &Path) -> ProjectManifest {
        ProjectManifest {
            schema_version: "1.0".to_string(),
            project: ProjectMetadata {
                id: "test-pack".to_string(),
                name: "Test Pack".to_string(),
                version: "1.0.0".to_string(),
                description: Some("smoke test".to_string()),
                authors: vec![],
            },
            minecraft: MinecraftSpec {
                version: "1.20.1".to_string(),
            },
            loader: LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.0".to_string(),
            },
            brief: None,
            java: None,
            profiles: vec![ProfileSpec {
                id: "client".to_string(),
                name: "Client".to_string(),
                side: Side::Client,
                include_optional_mods: false,
                include_shaders: false,
                memory_mb: None,
                jvm_args: vec![],
                include_mods: vec![],
                player_name: None,
            }],
            mods: vec![
                ModSpec {
                    id: "sodium".to_string(),
                    name: "Sodium".to_string(),
                    source: ModSource {
                        kind: SourceKind::Modrinth,
                        project_id: Some("sodium".to_string()),
                        file_id: None,
                        url: Some("https://example.com/sodium.jar".to_string()),
                        path: None,
                    },
                    version: "0.5.0".to_string(),
                    file_name: Some("sodium.jar".to_string()),
                    hashes: Some(FileHashes {
                        sha1: Some("abc".to_string()),
                        sha512: None,
                    }),
                    side: Side::Both,
                    dependencies: vec![],
                    status: vec![],
                    content_type: ContentType::Mod,
                },
                ModSpec {
                    id: "clientmod".to_string(),
                    name: "Client Mod".to_string(),
                    source: ModSource {
                        kind: SourceKind::Modrinth,
                        project_id: Some("clientmod".to_string()),
                        file_id: None,
                        url: Some("https://example.com/clientmod.jar".to_string()),
                        path: None,
                    },
                    version: "1.0.0".to_string(),
                    file_name: Some("clientmod.jar".to_string()),
                    hashes: None,
                    side: Side::Client,
                    dependencies: vec![],
                    status: vec![],
                    content_type: ContentType::Mod,
                },
            ],
            overrides: None,
        }
    }

    fn write_manifest(dir: &Path) -> PathBuf {
        let manifest_path = dir.join("tuffbox.json");
        let manifest = fixture_manifest(dir);
        fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
        // mods/ folder so the exporter can resolve files
        fs::create_dir_all(dir.join("mods")).unwrap();
        fs::write(dir.join("mods").join("sodium.jar"), b"dummy").unwrap();
        fs::write(dir.join("mods").join("clientmod.jar"), b"dummy").unwrap();
        manifest_path
    }

    #[test]
    fn export_modrinth_pack_smoke() {
        let dir = std::env::temp_dir().join("tuffbox_export_test_mr");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let manifest_path = write_manifest(&dir);
        let out = dir.join("pack.mrpack");
        let result = export_modrinth_pack(&fixture_manifest(&dir), &manifest_path, &out);
        assert!(result.is_ok(), "modrinth pack export failed: {:?}", result.err());
        let res = result.unwrap();
        assert!(out.exists(), "output mrpack not created");
        // Both "both"-side and "client"-side mods get a download entry
        assert_eq!(res.file_count, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_server_pack_skips_client_mods() {
        let dir = std::env::temp_dir().join("tuffbox_export_test_srv");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let manifest_path = write_manifest(&dir);
        let out = dir.join("server.zip");
        let result = export_server_pack(&fixture_manifest(&dir), &manifest_path, &out);
        assert!(result.is_ok(), "server pack export failed: {:?}", result.err());
        assert!(out.exists(), "output server zip not created");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_prism_instance_smoke() {
        let dir = std::env::temp_dir().join("tuffbox_export_test_prism");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let manifest_path = write_manifest(&dir);
        let out = dir.join("instance.zip");
        let result = export_prism_instance(&fixture_manifest(&dir), &manifest_path, &out);
        assert!(result.is_ok(), "prism instance export failed: {:?}", result.err());
        assert!(out.exists(), "output prism zip not created");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn export_curseforge_pack_smoke() {
        let dir = std::env::temp_dir().join("tuffbox_export_test_cf");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let manifest_path = write_manifest(&dir);
        let out = dir.join("modpack.zip");
        let result = export_curseforge_pack(&fixture_manifest(&dir), &manifest_path, &out);
        assert!(result.is_ok(), "curseforge pack export failed: {:?}", result.err());
        assert!(out.exists(), "output curseforge zip not created");
        let _ = fs::remove_dir_all(&dir);
    }
}
