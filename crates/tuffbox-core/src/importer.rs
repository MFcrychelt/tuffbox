use crate::manifest::{
    FileHashes, JavaSpec, LoaderKind, LoaderSpec, MinecraftSpec, ModSource, ProfileSpec,
    ProjectManifest, ProjectMetadata, Side, SourceKind,
};
use serde::Deserialize;
use std::{collections::HashMap, fs, io::Read, path::Path};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("unsupported archive format: {0}")]
    UnsupportedFormat(String),
    #[error("missing modrinth.index.json")]
    MissingModrinthIndex,
    #[error("missing instance.cfg")]
    MissingInstanceCfg,
    #[error("unknown loader: {0}")]
    UnknownLoader(String),
}

#[derive(Debug, Deserialize)]
struct ModrinthIndex {
    name: String,
    #[serde(default)]
    version_id: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    files: Vec<ModrinthFile>,
    dependencies: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    path: String,
    #[serde(default)]
    hashes: ModrinthFileHashes,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
struct ModrinthFileHashes {
    sha1: Option<String>,
    sha512: Option<String>,
}

pub fn import_modrinth_pack(path: impl AsRef<Path>) -> Result<ProjectManifest, ImportError> {
    let file = fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut index_raw = String::new();
    archive
        .by_name("modrinth.index.json")?
        .read_to_string(&mut index_raw)?;
    let index: ModrinthIndex = serde_json::from_str(&index_raw)?;

    let minecraft_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .unwrap_or_default();
    let (loader_kind, loader_version) = detect_loader(&index.dependencies)?;

    // `.mrpack` archives can declare files under `mods/`, `resourcepacks/`,
    // `shaderpacks/` and (less commonly) `datapacks/`. Previously only
    // `mods/` entries were kept, so any resourcepack/shaderpack bundled in
    // an imported modpack was silently dropped instead of being tracked
    // and reinstalled into the right folder.
    let mods: Vec<crate::manifest::ModSpec> = index
        .files
        .into_iter()
        .filter_map(|f| {
            let content_type = if f.path.starts_with("mods/") {
                crate::manifest::ContentType::Mod
            } else if f.path.starts_with("resourcepacks/") {
                crate::manifest::ContentType::Resourcepack
            } else if f.path.starts_with("shaderpacks/") {
                crate::manifest::ContentType::Shaderpack
            } else if f.path.starts_with("datapacks/") {
                crate::manifest::ContentType::Datapack
            } else {
                return None;
            };

            let file_name = Path::new(&f.path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| f.path.clone());
            let id = file_name
                .trim_start_matches("mods/")
                .trim_end_matches(".jar")
                .trim_end_matches(".zip")
                .to_string();
            Some(crate::manifest::ModSpec {
                id: id.clone(),
                name: id,
                source: ModSource {
                    kind: SourceKind::Direct,
                    project_id: None,
                    file_id: None,
                    url: f.downloads.first().cloned(),
                    path: Some(f.path),
                    icon_url: None,
                },
                version: "unknown".to_string(),
                file_name: Some(file_name),
                hashes: Some(FileHashes {
                    sha1: f.hashes.sha1,
                    sha512: f.hashes.sha512,
                }),
                side: parse_env(&f.env),
                dependencies: Vec::new(),
                status: vec!["ok".to_string()],
                content_type,
            })
        })
        .collect();

    Ok(ProjectManifest {
        schema_version: "0.1.0".to_string(),
        project: ProjectMetadata {
            id: slugify(&index.name),
            name: index.name,
            version: index.version_id.unwrap_or_else(|| "1.0.0".to_string()),
            description: index.summary,
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
        profiles: vec![
            ProfileSpec {
                id: "client".to_string(),
                name: "Client".to_string(),
                side: Side::Client,
                include_optional_mods: false,
                include_shaders: true,
                memory_mb: Some(4096),
                jvm_args: vec!["-XX:+UseG1GC".to_string()],
                include_mods: Vec::new(),
                player_name: None,
            },
            ProfileSpec {
                id: "server".to_string(),
                name: "Server".to_string(),
                side: Side::Server,
                include_optional_mods: false,
                include_shaders: false,
                memory_mb: Some(4096),
                jvm_args: vec!["-XX:+UseG1GC".to_string()],
                include_mods: Vec::new(),
                player_name: None,
            },
        ],
        mods,
        overrides: None,
    })
}

/// ── CurseForge modpack import ────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CurseForgeManifest {
    name: String,
    version: Option<String>,
    author: Option<String>,
    #[serde(default)]
    minecraft: CurseForgeMinecraft,
    #[serde(default, rename = "manifestType")]
    manifest_type: Option<String>,
    #[serde(default, rename = "manifestVersion")]
    manifest_version: Option<i32>,
    #[serde(default)]
    files: Vec<CurseForgeFile>,
    #[serde(default)]
    overrides: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct CurseForgeMinecraft {
    version: Option<String>,
    #[serde(default, rename = "modLoaders")]
    mod_loaders: Vec<CurseForgeModLoader>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CurseForgeModLoader {
    id: String,
    primary: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct CurseForgeFile {
    #[serde(default, rename = "projectID")]
    project_id: u64,
    #[serde(default, rename = "fileID")]
    file_id: u64,
    required: Option<bool>,
}

/// Returns true if the zip looks like a CurseForge Minecraft modpack
/// (`manifest.json` with `manifestType == minecraftModpack`).
pub fn is_curseforge_pack(path: impl AsRef<Path>) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return false;
    };
    let mut raw = String::new();
    for name in ["manifest.json", "Manifest.json"] {
        if let Ok(mut entry) = archive.by_name(name) {
            if entry.read_to_string(&mut raw).is_ok() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
                    return v.get("manifestType").and_then(|t| t.as_str())
                        == Some("minecraftModpack");
                }
            }
        }
    }
    false
}

/// Import a CurseForge modpack zip (manifest.json in root + overrides folder).
/// File download URLs are left empty until [`resolve_curseforge_pack_files`] runs.
pub fn import_curseforge_pack(path: impl AsRef<Path>) -> Result<ProjectManifest, ImportError> {
    let file = fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut manifest_raw = String::new();
    let manifest_name = if archive.by_name("manifest.json").is_ok() {
        "manifest.json"
    } else {
        "Manifest.json"
    };
    let mut entry = archive.by_name(manifest_name).map_err(|_| {
        ImportError::UnsupportedFormat("no manifest.json in CurseForge pack".into())
    })?;
    entry.read_to_string(&mut manifest_raw)?;
    let cf: CurseForgeManifest = serde_json::from_str(&manifest_raw)?;

    if cf.manifest_type.as_deref().unwrap_or("minecraftModpack") != "minecraftModpack" {
        return Err(ImportError::UnsupportedFormat(format!(
            "unexpected CurseForge manifestType {:?}",
            cf.manifest_type
        )));
    }

    let mc_version = cf.minecraft.version.unwrap_or_default();
    let (loader_kind, loader_version) = detect_curseforge_loader(&cf.minecraft.mod_loaders);

    let mods: Vec<crate::manifest::ModSpec> = cf
        .files
        .iter()
        .filter(|f| f.project_id != 0 && f.file_id != 0)
        .map(|f| {
            let id = format!("cf-{}-{}", f.project_id, f.file_id);
            crate::manifest::ModSpec {
                id: id.clone(),
                name: format!("CurseForge {}", f.project_id),
                source: crate::manifest::ModSource {
                    kind: SourceKind::Curseforge,
                    project_id: Some(f.project_id.to_string()),
                    file_id: Some(f.file_id.to_string()),
                    url: None,
                    path: None,
                    icon_url: None,
                },
                version: f.file_id.to_string(),
                file_name: Some(format!("cf-{}.jar", f.file_id)),
                hashes: None,
                side: if f.required.unwrap_or(true) {
                    Side::Both
                } else {
                    Side::Optional
                },
                dependencies: vec![],
                status: vec!["imported-curseforge".into()],
                content_type: crate::manifest::ContentType::Mod,
            }
        })
        .collect();

    let project_id = ProjectMetadata {
        id: slugify(&cf.name),
        name: cf.name,
        version: cf.version.unwrap_or_else(|| "1.0.0".into()),
        description: None,
        authors: cf.author.map(|a| vec![a]).unwrap_or_default(),
    };

    Ok(ProjectManifest {
        schema_version: crate::manifest::CURRENT_PROJECT_SCHEMA_VERSION.into(),
        project: project_id,
        minecraft: MinecraftSpec {
            version: mc_version,
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
        profiles: vec![
            ProfileSpec {
                id: "client".into(),
                name: "Client".into(),
                side: Side::Client,
                include_optional_mods: true,
                include_shaders: true,
                memory_mb: Some(4096),
                jvm_args: vec!["-XX:+UseG1GC".into()],
                include_mods: Vec::new(),
                player_name: Some("Player".into()),
            },
            ProfileSpec {
                id: "server".into(),
                name: "Server".into(),
                side: Side::Server,
                include_optional_mods: false,
                include_shaders: false,
                memory_mb: Some(4096),
                jvm_args: vec!["-XX:+UseG1GC".into()],
                include_mods: Vec::new(),
                player_name: None,
            },
        ],
        mods,
        overrides: None,
    })
}

/// Resolve CurseForge `projectID`/`fileID` entries into downloadable URLs
/// (batch `POST /mods/files` + optional Modrinth SHA1 fallback).
pub fn resolve_curseforge_pack_files(
    manifest: &mut ProjectManifest,
) -> Result<usize, crate::provider::ProviderError> {
    use crate::provider::CurseForgeProvider;

    let provider = CurseForgeProvider::new();
    let mut file_ids = Vec::new();
    let mut file_to_project: HashMap<u64, u64> = HashMap::new();
    for m in &manifest.mods {
        if m.source.kind != SourceKind::Curseforge {
            continue;
        }
        let (Some(pid), Some(fid)) = (
            m.source.project_id.as_ref().and_then(|s| s.parse().ok()),
            m.source.file_id.as_ref().and_then(|s| s.parse().ok()),
        ) else {
            continue;
        };
        file_ids.push(fid);
        file_to_project.insert(fid, pid);
    }
    if file_ids.is_empty() {
        return Ok(0);
    }

    let mut files = provider.get_files(&file_ids)?;
    let _ = provider.apply_modrinth_fallback(&mut files);

    let project_ids: Vec<u64> = file_to_project.values().copied().collect();
    let projects = provider.get_mods(&project_ids).unwrap_or_default();

    let mut resolved = 0usize;
    for module in &mut manifest.mods {
        if module.source.kind != SourceKind::Curseforge {
            continue;
        }
        let Some(fid) = module
            .source
            .file_id
            .as_ref()
            .and_then(|s| s.parse::<u64>().ok())
        else {
            continue;
        };
        let Some(info) = files.get(&fid) else {
            continue;
        };
        let project_id = file_to_project.get(&fid).copied().unwrap_or(info.mod_id);
        if let Some(proj) = projects.get(&project_id) {
            module.name = proj.name.clone();
            module.source.icon_url = proj.icon_url.clone();
        }
        module.file_name = Some(info.file_name.clone());
        module.version = info.display_name.clone();
        module.source.url = info.download_url.clone();
        module.hashes = Some(FileHashes {
            sha1: info.hashes.sha1.clone(),
            sha512: info.hashes.sha512.clone(),
        });
        module.content_type = match info.content_folder() {
            "resourcepacks" => crate::manifest::ContentType::Resourcepack,
            "shaderpacks" => crate::manifest::ContentType::Shaderpack,
            "datapacks" => crate::manifest::ContentType::Datapack,
            _ => crate::manifest::ContentType::Mod,
        };
        if info.download_url.is_some() {
            resolved += 1;
            module.status = vec!["ok".into()];
        } else {
            module.status = vec!["blocked-download".into()];
        }
    }
    Ok(resolved)
}

/// Extract the CurseForge `overrides/` folder into `instance_dir` (→ minecraft root).
pub fn extract_curseforge_overrides(
    pack_zip: impl AsRef<Path>,
    instance_dir: impl AsRef<Path>,
    overrides_folder: &str,
) -> Result<usize, ImportError> {
    let file = fs::File::open(pack_zip)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let prefix = format!("{}/", overrides_folder.trim_matches('/'));
    let mut count = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().replace('\\', "/");
        if !name.starts_with(&prefix) || name.ends_with('/') {
            continue;
        }
        let rel = &name[prefix.len()..];
        if rel.is_empty() {
            continue;
        }
        let dest = instance_dir.as_ref().join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&dest)?;
        std::io::copy(&mut entry, &mut out)?;
        count += 1;
    }
    Ok(count)
}

/// Read the overrides folder name from a CurseForge pack (`"overrides"` by default).
pub fn curseforge_overrides_folder(pack_zip: impl AsRef<Path>) -> Result<String, ImportError> {
    let file = fs::File::open(pack_zip)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut raw = String::new();
    let manifest_name = if archive.by_name("manifest.json").is_ok() {
        "manifest.json"
    } else if archive.by_name("Manifest.json").is_ok() {
        "Manifest.json"
    } else {
        return Err(ImportError::UnsupportedFormat("no manifest.json".into()));
    };
    let mut entry = archive.by_name(manifest_name)?;
    entry.read_to_string(&mut raw)?;
    let cf: CurseForgeManifest = serde_json::from_str(&raw)?;
    Ok(cf
        .overrides
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "overrides".into()))
}

/// Copy `manifest.json` into `instance_dir/curseforge/manifest.json` for future updates.
pub fn stash_curseforge_manifest(
    pack_zip: impl AsRef<Path>,
    instance_dir: impl AsRef<Path>,
) -> Result<(), ImportError> {
    let file = fs::File::open(pack_zip)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut raw = String::new();
    let manifest_name = if archive.by_name("manifest.json").is_ok() {
        "manifest.json"
    } else if archive.by_name("Manifest.json").is_ok() {
        "Manifest.json"
    } else {
        return Err(ImportError::UnsupportedFormat("no manifest.json".into()));
    };
    let mut entry = archive.by_name(manifest_name)?;
    entry.read_to_string(&mut raw)?;
    let dir = instance_dir.as_ref().join("curseforge");
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("manifest.json"), raw)?;
    Ok(())
}

fn detect_curseforge_loader(loaders: &[CurseForgeModLoader]) -> (LoaderKind, String) {
    // Prefer primary loader when flagged.
    let mut ordered: Vec<&CurseForgeModLoader> = loaders.iter().collect();
    ordered.sort_by_key(|l| !l.primary.unwrap_or(false));
    for loader in ordered {
        let id = loader.id.to_lowercase();
        if let Some(rest) = id.strip_prefix("neoforge-") {
            return (
                LoaderKind::Neoforge,
                if rest.is_empty() {
                    "latest".into()
                } else {
                    rest.to_string()
                },
            );
        }
        if let Some(rest) = id.strip_prefix("forge-") {
            return (
                LoaderKind::Forge,
                if rest.is_empty() {
                    "latest".into()
                } else {
                    rest.to_string()
                },
            );
        }
        if let Some(rest) = id.strip_prefix("fabric-") {
            return (
                LoaderKind::Fabric,
                if rest.is_empty() {
                    "latest".into()
                } else {
                    rest.to_string()
                },
            );
        }
        if let Some(rest) = id.strip_prefix("quilt-") {
            return (
                LoaderKind::Quilt,
                if rest.is_empty() {
                    "latest".into()
                } else {
                    rest.to_string()
                },
            );
        }
    }
    (LoaderKind::Vanilla, String::new())
}

fn slugify(s: &str) -> String {
    s.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

pub fn import_folder(path: impl AsRef<Path>) -> Result<ProjectManifest, ImportError> {
    let path = path.as_ref();
    if !path.is_dir() {
        return Err(ImportError::UnsupportedFormat(format!(
            "not a directory: {}",
            path.display()
        )));
    }

    let mods_dir = path.join("mods");
    if !mods_dir.exists() && !path.join("config").exists() {
        return Err(ImportError::UnsupportedFormat(
            "folder does not look like a Minecraft instance".to_string(),
        ));
    }

    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Minecraft Instance".to_string());

    let (loader_kind, loader_version, minecraft_version) = detect_instance(&mods_dir, path)?;

    Ok(ProjectManifest {
        schema_version: "0.1.0".to_string(),
        project: ProjectMetadata {
            id: slugify(&name),
            name,
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
    })
}

fn detect_instance(
    mods_dir: &Path,
    instance_dir: &Path,
) -> Result<(LoaderKind, String, String), ImportError> {
    // Fabric leaves a .fabric directory with loader metadata.
    let fabric_dir = instance_dir.join(".fabric");
    let has_fabric_marker = fabric_dir.exists();

    if has_fabric_marker {
        let version = detect_fabric_version(mods_dir).unwrap_or_default();
        return Ok((LoaderKind::Fabric, String::new(), version));
    }

    if mods_dir.exists() {
        if let Some(version) = detect_fabric_version(mods_dir) {
            return Ok((LoaderKind::Fabric, String::new(), version));
        }
        if let Some((loader, version)) = detect_forge_version(mods_dir) {
            return Ok((loader, String::new(), version));
        }
    }

    Ok((LoaderKind::Vanilla, String::new(), String::new()))
}

fn detect_fabric_version(mods_dir: &Path) -> Option<String> {
    if !mods_dir.exists() {
        return None;
    }
    for entry in fs::read_dir(mods_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension()?.to_str()? != "jar" {
            continue;
        }
        if let Some(version) = fabric_version_from_jar(&path) {
            return Some(version);
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct FabricModJson {
    #[serde(default)]
    depends: HashMap<String, String>,
}

fn fabric_version_from_jar(jar: &Path) -> Option<String> {
    let file = fs::File::open(jar).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut raw = String::new();
    archive
        .by_name("fabric.mod.json")
        .ok()?
        .read_to_string(&mut raw)
        .ok()?;
    let json: FabricModJson = serde_json::from_str(&raw).ok()?;
    json.depends
        .get("minecraft")
        .map(|s| extract_first_version(s))
}

fn detect_forge_version(mods_dir: &Path) -> Option<(LoaderKind, String)> {
    if !mods_dir.exists() {
        return None;
    }
    for entry in fs::read_dir(mods_dir).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if path.extension()?.to_str()? != "jar" {
            continue;
        }
        if let Some(version) = forge_version_from_jar(&path) {
            let loader = if path
                .file_name()
                .map(|s| s.to_string_lossy().to_lowercase().contains("neoforge"))
                .unwrap_or(false)
            {
                LoaderKind::Neoforge
            } else {
                LoaderKind::Forge
            };
            return Some((loader, version));
        }
    }
    None
}

fn forge_version_from_jar(jar: &Path) -> Option<String> {
    let file = fs::File::open(jar).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut raw = String::new();
    archive
        .by_name("META-INF/mods.toml")
        .ok()?
        .read_to_string(&mut raw)
        .ok()?;
    let doc: toml::Table = raw.parse().ok()?;
    let deps = doc.get("dependencies")?.as_table()?;
    for (_, dep_table) in deps {
        let arr = dep_table.as_array()?;
        for entry in arr {
            let t = entry.as_table()?;
            if t.get("modId")?.as_str()? == "minecraft" {
                let range = t.get("versionRange")?.as_str()?;
                return Some(extract_first_version(range));
            }
        }
    }
    None
}

fn extract_first_version(range: &str) -> String {
    let digits_dot = range
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>();
    if digits_dot.is_empty() {
        range.to_string()
    } else {
        digits_dot
    }
}

pub fn import_prism_instance(path: impl AsRef<Path>) -> Result<ProjectManifest, ImportError> {
    let file = fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut cfg_raw = String::new();
    archive
        .by_name("instance.cfg")?
        .read_to_string(&mut cfg_raw)?;

    let cfg = parse_ini(&cfg_raw);
    let name = cfg
        .get("name")
        .cloned()
        .unwrap_or_else(|| "Prism Instance".to_string());
    let minecraft_version = cfg.get("MinecraftVersion").cloned().unwrap_or_default();
    let loader_version = cfg.get("LoaderVersion").cloned().unwrap_or_default();
    let loader_kind = match cfg.get("Loader").map(String::as_str) {
        Some("fabric") => LoaderKind::Fabric,
        Some("forge") => LoaderKind::Forge,
        Some("quilt") => LoaderKind::Quilt,
        Some("neoforge") => LoaderKind::Neoforge,
        _ => LoaderKind::Vanilla,
    };

    Ok(ProjectManifest {
        schema_version: "0.1.0".to_string(),
        project: ProjectMetadata {
            id: slugify(&name),
            name,
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
    })
}

fn detect_loader(deps: &HashMap<String, String>) -> Result<(LoaderKind, String), ImportError> {
    if let Some(v) = deps.get("fabric-loader") {
        return Ok((LoaderKind::Fabric, v.clone()));
    }
    if let Some(v) = deps.get("quilt-loader") {
        return Ok((LoaderKind::Quilt, v.clone()));
    }
    if let Some(v) = deps.get("forge") {
        return Ok((LoaderKind::Forge, v.clone()));
    }
    if let Some(v) = deps.get("neoforge") {
        return Ok((LoaderKind::Neoforge, v.clone()));
    }
    Err(ImportError::UnknownLoader("no loader found".to_string()))
}

fn parse_env(env: &HashMap<String, String>) -> Side {
    let client = env.get("client").map(String::as_str);
    let server = env.get("server").map(String::as_str);
    match (client, server) {
        (Some("required"), Some("required")) => Side::Both,
        (Some("required"), _) => Side::Client,
        (_, Some("required")) => Side::Server,
        _ => Side::Both,
    }
}

fn _slugify2(name: &str) -> String {
    name.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "-")
        .replace("--", "-")
        .trim_matches('-')
        .to_string()
}

fn parse_ini(raw: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in raw.lines() {
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    map
}
