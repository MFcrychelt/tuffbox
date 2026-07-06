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
    #[serde(default)]
    manifest_type: Option<String>,
    #[serde(default)]
    files: Vec<CurseForgeFile>,
    #[serde(default)]
    overrides: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct CurseForgeMinecraft {
    version: Option<String>,
    #[serde(default)]
    mod_loaders: Vec<CurseForgeModLoader>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CurseForgeModLoader {
    id: String,
    primary: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct CurseForgeFile {
    #[serde(default)]
    project_id: u64,
    #[serde(default)]
    file_id: u64,
    required: Option<bool>,
}

/// Import a CurseForge modpack zip (manifest.json in root + overrides folder).
pub fn import_curseforge_pack(path: impl AsRef<Path>) -> Result<ProjectManifest, ImportError> {
    let file = fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut manifest_raw = String::new();
    let manifest_name = if archive.by_name("manifest.json").is_ok() { "manifest.json" } else { "Manifest.json" };
    let mut entry = archive.by_name(manifest_name).map_err(|_| ImportError::UnsupportedFormat("no manifest.json in CurseForge pack".into()))?;
    entry.read_to_string(&mut manifest_raw)?;
    let cf: CurseForgeManifest = serde_json::from_str(&manifest_raw)?;

    let mc_version = cf.minecraft.version.unwrap_or_default();
    let (loader_kind, loader_version) = detect_curseforge_loader(&cf.minecraft.mod_loaders);

    let mods: Vec<crate::manifest::ModSpec> = cf
        .files
        .iter()
        .map(|f| {
            let id = format!("cf-{}", f.project_id);
            crate::manifest::ModSpec {
                id: id.clone(),
                name: format!("CurseForge mod {}", f.project_id),
                source: crate::manifest::ModSource {
                    kind: crate::manifest::SourceKind::Modrinth,
                    project_id: Some(f.project_id.to_string()),
                    file_id: Some(f.file_id.to_string()),
                    url: Some(format!("https://www.curseforge.com/minecraft/mc-mods/{}", f.project_id)),
                    path: None,
                },
                version: "unknown".into(),
                file_name: Some(format!("cf-{}.jar", f.project_id)),
                hashes: None,
                side: crate::manifest::Side::Both,
                dependencies: vec![],
                status: vec!["imported-curseforge".into()],
                content_type: crate::manifest::ContentType::Mod,
            }
        })
        .collect();

    let project_id = crate::manifest::ProjectMetadata {
        id: slugify(&cf.name),
        name: cf.name,
        version: cf.version.unwrap_or_else(|| "1.0.0".into()),
        description: None,
        authors: cf.author.map(|a| vec![a]).unwrap_or_default(),
    };

    Ok(ProjectManifest {
        schema_version: crate::manifest::CURRENT_PROJECT_SCHEMA_VERSION.into(),
        project: project_id,
        minecraft: crate::manifest::MinecraftSpec { version: mc_version },
        loader: crate::manifest::LoaderSpec { kind: loader_kind, version: loader_version },
        brief: None,
        java: None,
        profiles: vec![crate::manifest::ProfileSpec {
            id: "client".into(),
            name: "Client".into(),
            side: crate::manifest::Side::Both,
            include_optional_mods: false,
            include_shaders: false,
            memory_mb: Some(4096),
            jvm_args: vec!["-XX:+UseG1GC".into(), "-Xmx4G".into()],
            include_mods: mods.iter().map(|m| m.id.clone()).collect(),
            player_name: Some("Player".into()),
        }],
        mods,
        overrides: None,
    })
}

fn detect_curseforge_loader(loaders: &[CurseForgeModLoader]) -> (LoaderKind, String) {
    for loader in loaders {
        let id = loader.id.to_lowercase();
        if id.contains("forge") && !id.contains("neo") {
            let version = id.replace("forge-", "").replace("forge", "");
            return (LoaderKind::Forge, if version.is_empty() { "latest".into() } else { version });
        }
        if id.contains("neoforge") {
            let version = id.replace("neoforge-", "").replace("neoforge", "");
            return (LoaderKind::Neoforge, if version.is_empty() { "latest".into() } else { version });
        }
        if id.contains("fabric") {
            let version = id.replace("fabric-", "").replace("fabric", "");
            return (LoaderKind::Fabric, if version.is_empty() { "latest".into() } else { version });
        }
        if id.contains("quilt") {
            let version = id.replace("quilt-", "").replace("quilt", "");
            return (LoaderKind::Quilt, if version.is_empty() { "latest".into() } else { version });
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
    archive.by_name("fabric.mod.json").ok()?.read_to_string(&mut raw).ok()?;
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
