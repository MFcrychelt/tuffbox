use crate::{graph::DependencyGraph, manifest::ProjectManifest};
use serde::{Deserialize, Serialize};

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
            schema_version: "0.1.0".to_string(),
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

fn rfc3339_now() -> String {
    // Use a deterministic fallback if std::time is unavailable in tests.
    // In production this should be replaced with chrono or time crate.
    "2026-06-29T00:00:00Z".to_string()
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
