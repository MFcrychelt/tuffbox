use crate::adapters::{FabricAdapter, ForgeAdapter, LoaderAdapter, NeoForgeAdapter};
use crate::{DependencyGraph, DependencyKind, LoaderKind, ModDependencySpec, ProjectManifest};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphCache {
    pub manifest_fingerprint: String,
    pub generated_at: String,
    pub enriched_manifest: ProjectManifest,
    pub graph: DependencyGraph,
}

impl GraphCache {
    pub fn new(base_manifest: &ProjectManifest, enriched_manifest: ProjectManifest) -> Self {
        let graph = DependencyGraph::from_manifest(&enriched_manifest);
        Self {
            manifest_fingerprint: manifest_fingerprint(base_manifest),
            generated_at: crate::time_util::rfc3339_now(),
            enriched_manifest,
            graph,
        }
    }

    pub fn load_if_current(
        manifest_path: &Path,
        manifest: &ProjectManifest,
    ) -> Result<Option<Self>, String> {
        let path = graph_cache_path(manifest_path)?;
        if !path.is_file() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path)
            .map_err(|error| format!("failed to read graph cache {}: {error}", path.display()))?;
        let cache: Self = serde_json::from_str(&raw)
            .map_err(|error| format!("failed to parse graph cache {}: {error}", path.display()))?;
        Ok((cache.manifest_fingerprint == manifest_fingerprint(manifest)).then_some(cache))
    }

    pub fn save(&self, manifest_path: &Path) -> Result<PathBuf, String> {
        let path = graph_cache_path(manifest_path)?;
        let parent = path
            .parent()
            .ok_or_else(|| format!("graph cache path has no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let mut staged = tempfile::Builder::new()
            .prefix(".dependency-graph-")
            .suffix(".tmp")
            .tempfile_in(parent)
            .map_err(|error| error.to_string())?;
        serde_json::to_writer_pretty(&mut staged, self).map_err(|error| error.to_string())?;
        staged.flush().map_err(|error| error.to_string())?;
        staged
            .persist(&path)
            .map_err(|error| error.error.to_string())?;
        Ok(path)
    }
}

pub fn manifest_fingerprint(manifest: &ProjectManifest) -> String {
    let bytes = serde_json::to_vec(manifest).unwrap_or_default();
    format!("{:x}", Sha1::digest(bytes))
}

pub fn graph_cache_path(manifest_path: &Path) -> Result<PathBuf, String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| format!("manifest path has no parent: {}", manifest_path.display()))?;
    Ok(project_dir
        .join(".tuffbox")
        .join("cache")
        .join("dependency-graph.json"))
}

/// Adds dependency metadata directly from installed mod jars. Network data can
/// replace this later, but the graph remains useful offline and for local mods.
pub fn enrich_manifest_from_installed_jars(manifest_path: &Path, manifest: &mut ProjectManifest) {
    let Some(instance_dir) = crate::instance_dir_for_manifest(manifest_path) else {
        return;
    };
    let adapter: Box<dyn LoaderAdapter> = match manifest.loader.kind {
        LoaderKind::Fabric | LoaderKind::Quilt => Box::new(FabricAdapter),
        LoaderKind::Forge => Box::new(ForgeAdapter),
        LoaderKind::Neoforge => Box::new(NeoForgeAdapter),
        LoaderKind::Vanilla => return,
    };
    for module in &mut manifest.mods {
        if !module.dependencies.is_empty() {
            continue;
        }
        let Some(file_name) = module.file_name.as_ref() else {
            continue;
        };
        let path = instance_dir.join("mods").join(file_name);
        let Ok(file) = std::fs::File::open(path) else {
            continue;
        };
        let Ok(mut archive) = zip::ZipArchive::new(file) else {
            continue;
        };
        let Ok(metadata) = adapter.extract_metadata(&mut archive) else {
            continue;
        };
        module.dependencies = metadata
            .dependencies
            .into_iter()
            .filter(|dependency| dependency.mod_id != module.id)
            .map(|dependency| ModDependencySpec {
                target: dependency.mod_id,
                kind: if dependency.required {
                    DependencyKind::Requires
                } else {
                    DependencyKind::Optional
                },
                version_constraint: None,
                reason: Some("Read from installed mod metadata".to_string()),
            })
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_is_invalidated_when_manifest_changes() {
        let manifest: ProjectManifest = serde_json::from_str(include_str!(
            "../../../examples/sample-project.tuffbox.json"
        ))
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("project.tuffbox.json");
        let cache = GraphCache::new(&manifest, manifest.clone());
        cache.save(&manifest_path).unwrap();
        assert!(GraphCache::load_if_current(&manifest_path, &manifest)
            .unwrap()
            .is_some());

        let mut changed = manifest;
        changed.project.version.push_str("-changed");
        assert!(GraphCache::load_if_current(&manifest_path, &changed)
            .unwrap()
            .is_none());
    }
}
