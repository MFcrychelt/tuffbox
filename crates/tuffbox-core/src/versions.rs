use serde::Deserialize;
use std::collections::HashSet;
use thiserror::Error;

const POPULAR_MINECRAFT: &[&str] = &["1.12.2", "1.16.5", "1.20.1", "1.21.1", "26.1.2", "26.2"];
const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Error)]
pub enum VersionsError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("parse error: {0}")]
    ParseError(String),
}

#[derive(Debug, Deserialize)]
struct Manifest {
    versions: Vec<ManifestVersion>,
}

#[derive(Debug, Deserialize)]
struct ManifestVersion {
    id: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MinecraftVersion {
    pub id: String,
    pub popular: bool,
}

pub fn fetch_minecraft_versions() -> Result<Vec<MinecraftVersion>, VersionsError> {
    let popular: HashSet<String> = POPULAR_MINECRAFT.iter().map(|s| s.to_string()).collect();
    let mut popular_versions: Vec<MinecraftVersion> = POPULAR_MINECRAFT
        .iter()
        .map(|id| MinecraftVersion {
            id: id.to_string(),
            popular: true,
        })
        .collect();

    let manifest: Manifest = crate::http::get_json(VERSION_MANIFEST_URL)?;
    let mut rest: Vec<MinecraftVersion> = manifest
        .versions
        .into_iter()
        .filter(|v| v.kind == "release" && !popular.contains(&v.id))
        .map(|v| MinecraftVersion {
            id: v.id,
            popular: false,
        })
        .collect();

    rest.sort_by(|a, b| compare_versions(&b.id, &a.id));

    popular_versions.append(&mut rest);
    Ok(popular_versions)
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| {
        s.split('.')
            .filter_map(|p| p.parse::<u32>().ok())
            .collect::<Vec<_>>()
    };
    parse(a).cmp(&parse(b))
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LoaderVersion {
    pub id: String,
    pub stable: bool,
}

pub fn fetch_loader_versions(
    loader: &str,
    minecraft_version: &str,
) -> Result<Vec<LoaderVersion>, VersionsError> {
    match loader {
        "fabric" => fetch_fabric_versions(minecraft_version),
        "quilt" => fetch_quilt_versions(minecraft_version),
        "forge" => fetch_forge_versions(minecraft_version),
        "neoforge" => fetch_neoforge_versions(minecraft_version),
        _ => Ok(Vec::new()),
    }
}

fn fetch_fabric_versions(mc: &str) -> Result<Vec<LoaderVersion>, VersionsError> {
    let url = format!("https://meta.fabricmc.net/v2/versions/loader/{mc}");
    let entries: Vec<FabricLoaderEntry> = crate::http::get_json(&url)?;
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersion {
            id: e.loader.version,
            stable: e.loader.stable,
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoader,
}

#[derive(Debug, Deserialize)]
struct FabricLoader {
    version: String,
    stable: bool,
}

fn fetch_quilt_versions(mc: &str) -> Result<Vec<LoaderVersion>, VersionsError> {
    let url = format!("https://meta.quiltmc.org/v3/versions/loader/{mc}");
    let entries: Vec<QuiltLoaderEntry> = crate::http::get_json(&url)?;
    Ok(entries
        .into_iter()
        .map(|e| LoaderVersion {
            id: e.loader.version,
            stable: true,
        })
        .collect())
}

#[derive(Debug, Deserialize)]
struct QuiltLoaderEntry {
    loader: QuiltLoader,
}

#[derive(Debug, Deserialize)]
struct QuiltLoader {
    version: String,
}

#[derive(Debug, Deserialize)]
struct ModrinthLoaderManifest {
    game_versions: Vec<ModrinthGameVersion>,
}

#[derive(Debug, Deserialize)]
struct ModrinthGameVersion {
    id: String,
    #[serde(default)]
    #[allow(dead_code)]
    stable: bool,
    loaders: Vec<ModrinthLoaderEntry>,
}

#[derive(Debug, Deserialize)]
struct ModrinthLoaderEntry {
    id: String,
    #[allow(dead_code)]
    url: String,
    #[serde(default)]
    #[allow(dead_code)]
    stable: bool,
}

fn fetch_forge_versions(mc: &str) -> Result<Vec<LoaderVersion>, VersionsError> {
    let url = "https://launcher-meta.modrinth.com/forge/v0/manifest.json";
    let manifest: ModrinthLoaderManifest = crate::http::get_json(url)?;
    Ok(manifest
        .game_versions
        .into_iter()
        .find(|v| v.id == mc)
        .map(|v| {
            v.loaders
                .into_iter()
                .map(|e| LoaderVersion {
                    id: e.id,
                    stable: e.stable,
                })
                .collect()
        })
        .unwrap_or_default())
}

fn fetch_neoforge_versions(mc: &str) -> Result<Vec<LoaderVersion>, VersionsError> {
    let prefix = neoforge_prefix(mc)?;
    let url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
    let text = crate::http::get_text(url)?;
    let mut versions: Vec<LoaderVersion> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if let Some(start) = line.find("<version>") {
                let end = line.find("</version>")?;
                let ver = &line[start + 9..end];
                if ver.starts_with(&prefix) {
                    return Some(LoaderVersion {
                        id: ver.to_string(),
                        stable: !ver.contains("-beta"),
                    });
                }
            }
            None
        })
        .collect();
    versions.reverse();
    Ok(versions)
}

fn neoforge_prefix(mc: &str) -> Result<String, VersionsError> {
    let parts: Vec<&str> = mc.split('.').collect();
    if parts.len() < 2 {
        return Err(VersionsError::ParseError(format!(
            "Cannot derive NeoForge prefix from Minecraft version {mc}"
        )));
    }
    let minor = parts[1].parse::<u32>().map_err(|_| {
        VersionsError::ParseError(format!("Invalid Minecraft minor version in {mc}"))
    })?;
    let patch = parts
        .get(2)
        .and_then(|p| p.parse::<u32>().ok())
        .unwrap_or(0);
    Ok(format!("{minor}.{patch}."))
}
