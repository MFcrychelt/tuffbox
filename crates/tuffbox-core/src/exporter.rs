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
