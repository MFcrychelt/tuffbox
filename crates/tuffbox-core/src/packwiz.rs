//! Import packwiz packs (`pack.toml` + `index.toml` + `.pw.toml` metafiles).
//!
//! No packwiz binary — parses the on-disk TOML format into [`ProjectManifest`].

use crate::manifest::{
    ContentType, FileHashes, JavaSpec, LoaderKind, LoaderSpec, MinecraftSpec, ModOption, ModSource,
    ModSpec, ProfileSpec, ProjectManifest, ProjectMetadata, Side, SourceKind,
};
use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackwizImportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("missing pack.toml")]
    MissingPackToml,
    #[error("missing index file: {0}")]
    MissingIndex(String),
    #[error("unsupported loader in pack.toml versions")]
    UnknownLoader,
}

#[derive(Debug, Deserialize)]
struct PackToml {
    name: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "pack-format")]
    pack_format: Option<String>,
    index: PackIndexRef,
    #[serde(default)]
    versions: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct PackIndexRef {
    file: String,
}

#[derive(Debug, Deserialize)]
struct IndexToml {
    #[serde(default)]
    files: Vec<IndexFile>,
}

#[derive(Debug, Deserialize)]
struct IndexFile {
    file: String,
    #[serde(default)]
    metafile: bool,
}

#[derive(Debug, Deserialize)]
struct PwToml {
    name: String,
    filename: String,
    #[serde(default)]
    side: Option<String>,
    #[serde(default)]
    pin: bool,
    #[serde(default)]
    download: Option<PwDownload>,
    #[serde(default)]
    update: Option<PwUpdate>,
    #[serde(default)]
    option: Option<PwOption>,
}

#[derive(Debug, Deserialize)]
struct PwDownload {
    #[serde(default)]
    url: Option<String>,
    #[serde(default, rename = "hash-format")]
    hash_format: Option<String>,
    #[serde(default)]
    hash: Option<String>,
    #[serde(default)]
    mode: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PwUpdate {
    #[serde(default)]
    modrinth: Option<PwModrinthUpdate>,
    #[serde(default)]
    curseforge: Option<PwCurseforgeUpdate>,
    #[serde(default)]
    github: Option<PwGithubUpdate>,
}

#[derive(Debug, Deserialize)]
struct PwModrinthUpdate {
    #[serde(rename = "mod-id")]
    mod_id: String,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PwCurseforgeUpdate {
    #[serde(rename = "project-id")]
    project_id: u64,
    #[serde(default, rename = "file-id")]
    file_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct PwGithubUpdate {
    #[serde(default)]
    slug: Option<String>, // "owner/repo"
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    tag: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PwOption {
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    default: Option<bool>,
}

/// True when `path` is a directory containing `pack.toml`.
pub fn is_packwiz_pack(path: impl AsRef<Path>) -> bool {
    path.as_ref().join("pack.toml").is_file()
}

/// Import a packwiz pack directory into a [`ProjectManifest`].
pub fn import_packwiz_pack(path: impl AsRef<Path>) -> Result<ProjectManifest, PackwizImportError> {
    let root = path.as_ref();
    let pack_path = root.join("pack.toml");
    if !pack_path.is_file() {
        return Err(PackwizImportError::MissingPackToml);
    }
    let pack: PackToml = toml::from_str(&fs::read_to_string(&pack_path)?)?;
    let _ = pack.pack_format; // accepted for forward-compat; unused

    let index_path = root.join(&pack.index.file);
    if !index_path.is_file() {
        return Err(PackwizImportError::MissingIndex(pack.index.file.clone()));
    }
    let index: IndexToml = toml::from_str(&fs::read_to_string(&index_path)?)?;

    let (loader_kind, loader_version) = detect_loader(&pack.versions)?;
    let mc_version = pack
        .versions
        .get("minecraft")
        .cloned()
        .unwrap_or_default();

    let mut mods = Vec::new();
    for entry in &index.files {
        if !entry.metafile {
            continue;
        }
        let meta_path = index_path
            .parent()
            .unwrap_or(root)
            .join(&entry.file);
        if !meta_path.is_file() {
            continue;
        }
        let pw: PwToml = match toml::from_str(&fs::read_to_string(&meta_path)?) {
            Ok(v) => v,
            Err(_) => continue,
        };
        mods.push(pw_to_modspec(&pw));
    }

    let project_id = slugify(&pack.name);
    Ok(ProjectManifest {
        schema_version: crate::manifest::CURRENT_PROJECT_SCHEMA_VERSION.into(),
        project: ProjectMetadata {
            id: project_id,
            name: pack.name,
            version: pack.version.unwrap_or_else(|| "1.0.0".into()),
            description: pack.description,
            authors: pack.author.map(|a| vec![a]).unwrap_or_default(),
        },
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

fn detect_loader(versions: &HashMap<String, String>) -> Result<(LoaderKind, String), PackwizImportError> {
    for (key, kind) in [
        ("fabric", LoaderKind::Fabric),
        ("quilt", LoaderKind::Quilt),
        ("neoforge", LoaderKind::Neoforge),
        ("forge", LoaderKind::Forge),
    ] {
        if let Some(ver) = versions.get(key) {
            return Ok((kind, ver.clone()));
        }
    }
    if versions.contains_key("minecraft") {
        return Ok((LoaderKind::Vanilla, "none".into()));
    }
    Err(PackwizImportError::UnknownLoader)
}

fn pw_to_modspec(pw: &PwToml) -> ModSpec {
    let optional = pw.option.as_ref().map(|o| o.optional).unwrap_or(false);
    let side = if optional {
        Side::Optional
    } else {
        match pw.side.as_deref().unwrap_or("both") {
            "client" => Side::Client,
            "server" => Side::Server,
            _ => Side::Both,
        }
    };

    let (kind, project_id, file_id) = resolve_source(pw);
    let url = pw.download.as_ref().and_then(|d| d.url.clone());
    let hashes = pw.download.as_ref().and_then(|d| {
        let hash = d.hash.clone()?;
        match d.hash_format.as_deref() {
            Some("sha1") => Some(FileHashes {
                sha1: Some(hash),
                sha512: None,
            }),
            Some("sha512") => Some(FileHashes {
                sha1: None,
                sha512: Some(hash),
            }),
            _ => Some(FileHashes {
                sha1: None,
                sha512: Some(hash),
            }),
        }
    });

    let mut status = vec!["imported-packwiz".into()];
    if pw.pin {
        status.push("pinned".into());
    }

    let option = pw.option.as_ref().map(|o| ModOption {
        description: o.description.clone(),
        default: o.default.unwrap_or(true),
    });

    let id = slugify(&pw.name);
    ModSpec {
        id,
        name: pw.name.clone(),
        source: ModSource {
            kind,
            project_id,
            file_id,
            url,
            path: None,
            icon_url: None,
            categories: Vec::new(),
        },
        version: pw
            .update
            .as_ref()
            .and_then(|u| u.modrinth.as_ref())
            .and_then(|m| m.version.clone())
            .unwrap_or_else(|| "unknown".into()),
        file_name: Some(pw.filename.clone()),
        hashes,
        side,
        dependencies: vec![],
        status,
        content_type: content_type_from_filename(&pw.filename),
        authors: Vec::new(),
        option,
    }
}

fn resolve_source(pw: &PwToml) -> (SourceKind, Option<String>, Option<String>) {
    if let Some(update) = &pw.update {
        if let Some(mr) = &update.modrinth {
            return (
                SourceKind::Modrinth,
                Some(mr.mod_id.clone()),
                mr.version.clone(),
            );
        }
        if let Some(cf) = &update.curseforge {
            return (
                SourceKind::Curseforge,
                Some(cf.project_id.to_string()),
                cf.file_id.map(|id| id.to_string()),
            );
        }
        if let Some(gh) = &update.github {
            let slug = gh
                .slug
                .clone()
                .or_else(|| match (&gh.owner, &gh.repo) {
                    (Some(o), Some(r)) => Some(format!("{o}/{r}")),
                    _ => None,
                });
            return (SourceKind::Github, slug, gh.tag.clone());
        }
    }
    if pw
        .download
        .as_ref()
        .and_then(|d| d.mode.as_deref())
        .is_some_and(|m| m.contains("curseforge"))
    {
        return (SourceKind::Curseforge, None, None);
    }
    if pw.download.as_ref().and_then(|d| d.url.as_ref()).is_some() {
        return (SourceKind::Direct, None, None);
    }
    (SourceKind::Local, None, None)
}

fn content_type_from_filename(name: &str) -> ContentType {
    let lower = name.to_ascii_lowercase();
    if lower.contains("resourcepack") || lower.ends_with(".zip") && lower.contains("resource") {
        ContentType::Resourcepack
    } else if lower.contains("shader") {
        ContentType::Shaderpack
    } else if lower.contains("datapack") {
        ContentType::Datapack
    } else {
        ContentType::Mod
    }
}

fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn import_minimal_packwiz_pack() {
        let dir = std::env::temp_dir().join("tuffbox_packwiz_import_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("mods")).unwrap();

        fs::write(
            dir.join("pack.toml"),
            r#"
name = "Demo Pack"
author = "Tester"
version = "0.1.0"
pack-format = "packwiz:1.1.0"

[index]
file = "index.toml"
hash-format = "sha256"
hash = "00"

[versions]
minecraft = "1.20.1"
fabric = "0.15.0"
"#,
        )
        .unwrap();

        fs::write(
            dir.join("index.toml"),
            r#"
hash-format = "sha256"

[[files]]
file = "mods/sodium.pw.toml"
hash = "00"
metafile = true
"#,
        )
        .unwrap();

        fs::write(
            dir.join("mods/sodium.pw.toml"),
            r#"
name = "Sodium"
filename = "sodium.jar"
side = "client"

[download]
url = "https://example.com/sodium.jar"
hash-format = "sha1"
hash = "deadbeef"

[update.modrinth]
mod-id = "AANobbMI"
version = "mc1.20.1-0.5.8"

[option]
optional = true
description = "Pretty rendering"
default = true
"#,
        )
        .unwrap();

        let manifest = import_packwiz_pack(&dir).unwrap();
        assert_eq!(manifest.project.name, "Demo Pack");
        assert_eq!(manifest.minecraft.version, "1.20.1");
        assert_eq!(manifest.loader.kind, LoaderKind::Fabric);
        assert_eq!(manifest.mods.len(), 1);
        let m = &manifest.mods[0];
        assert_eq!(m.source.kind, SourceKind::Modrinth);
        assert_eq!(m.source.project_id.as_deref(), Some("AANobbMI"));
        assert_eq!(m.side, Side::Optional);
        assert_eq!(
            m.option.as_ref().and_then(|o| o.description.as_deref()),
            Some("Pretty rendering")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_packwiz_detects_pack_toml() {
        let dir = std::env::temp_dir().join("tuffbox_packwiz_detect");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(!is_packwiz_pack(&dir));
        let mut f = fs::File::create(dir.join("pack.toml")).unwrap();
        writeln!(f, "name = \"x\"").unwrap();
        assert!(is_packwiz_pack(&dir));
        let _ = fs::remove_dir_all(&dir);
    }
}
