use crate::{graph::DependencyGraph, manifest::ProjectManifest};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use thiserror::Error;

pub const CURRENT_LOCKFILE_SCHEMA_VERSION: &str = "0.1.0";
pub const SUPPORTED_LOCKFILE_SCHEMA_VERSIONS: &[&str] = &["0.1.0", "0.1"];

#[derive(Debug, Error)]
pub enum LockfileError {
    #[error("failed to read lockfile {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse lockfile {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported lockfile schema version {version}; supported versions: {supported}")]
    UnsupportedSchemaVersion { version: String, supported: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuffboxLockfile {
    pub schema_version: String,
    pub project_id: String,
    pub project_version: String,
    pub minecraft_version: String,
    pub loader: LockedLoader,
    #[serde(default)]
    pub java_major: Option<u16>,
    pub mods: Vec<LockedMod>,
    pub graph: LockedGraph,
    pub generated_at: String,
}

impl TuffboxLockfile {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, LockfileError> {
        let path_ref = path.as_ref();
        let path_string = path_ref.display().to_string();
        let raw = fs::read_to_string(path_ref).map_err(|source| LockfileError::Read {
            path: path_string.clone(),
            source,
        })?;
        let mut value: serde_json::Value = serde_json::from_str(&raw).map_err(|source| LockfileError::Parse {
            path: path_string.clone(),
            source,
        })?;
        migrate_lockfile_value(&mut value)?;
        serde_json::from_value(value).map_err(|source| LockfileError::Parse {
            path: path_string,
            source,
        })
    }

    pub fn migrate_to_current_schema(&mut self) {
        self.schema_version = CURRENT_LOCKFILE_SCHEMA_VERSION.to_string();
    }

    pub fn from_manifest_and_graph(manifest: &ProjectManifest, graph: &DependencyGraph) -> Self {
        let mut mods: Vec<LockedMod> = manifest
            .mods
            .iter()
            .map(|module| LockedMod {
                id: module.id.clone(),
                name: module.name.clone(),
                version: module.version.clone(),
                source: LockedSource {
                    kind: format!("{:?}", module.source.kind).to_lowercase(),
                    project_id: module.source.project_id.clone(),
                    file_id: module.source.file_id.clone(),
                    url: module.source.url.clone(),
                    path: module.source.path.clone(),
                },
                file_name: module.file_name.clone(),
                hashes: LockedHashes {
                    sha1: module
                        .hashes
                        .as_ref()
                        .and_then(|hashes| hashes.sha1.clone()),
                    sha512: module
                        .hashes
                        .as_ref()
                        .and_then(|hashes| hashes.sha512.clone()),
                },
                side: format!("{:?}", module.side).to_lowercase(),
            })
            .collect();

        mods.sort_by(|a, b| a.id.cmp(&b.id));

        let edges = graph
            .edges
            .iter()
            .map(|edge| LockedEdge {
                from: edge.from.0.clone(),
                to: edge.to.0.clone(),
                kind: format!("{:?}", edge.kind),
                constraint: edge.constraint.clone(),
                reason: edge.reason.clone(),
            })
            .collect();

        Self {
            schema_version: CURRENT_LOCKFILE_SCHEMA_VERSION.to_string(),
            project_id: manifest.project.id.clone(),
            project_version: manifest.project.version.clone(),
            minecraft_version: manifest.minecraft.version.clone(),
            loader: LockedLoader {
                kind: format!("{:?}", manifest.loader.kind).to_lowercase(),
                version: manifest.loader.version.clone(),
            },
            java_major: manifest.java.as_ref().and_then(|java| java.major),
            mods,
            graph: LockedGraph {
                node_count: graph.nodes.len(),
                edge_count: graph.edges.len(),
                edges,
            },
            generated_at: rfc3339_now(),
        }
    }
}

pub fn migrate_lockfile_value(value: &mut serde_json::Value) -> Result<(), LockfileError> {
    let version = value
        .get("schemaVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("0.1.0")
        .to_string();

    if !SUPPORTED_LOCKFILE_SCHEMA_VERSIONS.contains(&version.as_str()) {
        return Err(LockfileError::UnsupportedSchemaVersion {
            version,
            supported: SUPPORTED_LOCKFILE_SCHEMA_VERSIONS.join(", "),
        });
    }

    if let Some(object) = value.as_object_mut() {
        object.insert(
            "schemaVersion".to_string(),
            serde_json::Value::String(CURRENT_LOCKFILE_SCHEMA_VERSION.to_string()),
        );
        object
            .entry("mods".to_string())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    }

    Ok(())
}

fn rfc3339_now() -> String {
    crate::time_util::rfc3339_now()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedLoader {
    pub kind: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedMod {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: LockedSource,
    pub file_name: Option<String>,
    pub hashes: LockedHashes,
    pub side: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedSource {
    pub kind: String,
    pub project_id: Option<String>,
    pub file_id: Option<String>,
    pub url: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedHashes {
    pub sha1: Option<String>,
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LockedGraph {
    pub node_count: usize,
    pub edge_count: usize,
    pub edges: Vec<LockedEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub constraint: Option<String>,
    pub reason: Option<String>,
}
