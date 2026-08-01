//! Full item catalog from vanilla + mod JARs (models + lang), not only recipe I/O.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::manifest::ProjectManifest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItemEntry {
    pub id: String,
    pub name: String,
    pub mod_ns: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogCache {
    fingerprint: String,
    items: Vec<CatalogItemEntry>,
}

/// Build a catalog of item ids from jar `assets/*/models/item/*.json` plus lang names.
/// Merges any extra ids (e.g. from recipes) that do not appear in models.
pub fn build_item_catalog(
    project_dir: &Path,
    mc_version: &str,
    extra_ids: impl IntoIterator<Item = String>,
) -> Result<Vec<CatalogItemEntry>, String> {
    let jars = catalog_jar_sources(project_dir, mc_version);
    let fingerprint = catalog_fingerprint(mc_version, &jars);
    let cache_path = project_dir.join(".tuffbox").join("cache").join("item-catalog-v1.json");

    let mut by_id: BTreeMap<String, CatalogItemEntry> = BTreeMap::new();

    if let Ok(raw) = std::fs::read_to_string(&cache_path) {
        if let Ok(cached) = serde_json::from_str::<CatalogCache>(&raw) {
            if cached.fingerprint == fingerprint {
                by_id = cached
                    .items
                    .into_iter()
                    .map(|e| (e.id.clone(), e))
                    .collect();
            }
        }
    }

    if by_id.is_empty() {
        let mut lang: HashMap<String, String> = HashMap::new();
        for jar in &jars {
            let Ok(file) = std::fs::File::open(jar) else {
                continue;
            };
            let Ok(mut archive) = ZipArchive::new(file) else {
                continue;
            };
            collect_lang_from_archive(&mut archive, &mut lang);
            collect_models_from_archive(&mut archive, &mut by_id, &lang);
        }

        if let Some(parent) = cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let payload = CatalogCache {
            fingerprint: fingerprint.clone(),
            items: by_id.values().cloned().collect(),
        };
        if let Ok(json) = serde_json::to_string(&payload) {
            let _ = std::fs::write(&cache_path, json);
        }
    }

    for id in extra_ids {
        let id = id.trim().to_string();
        if id.is_empty() || id.starts_with('#') || id == "unknown:unknown" {
            continue;
        }
        by_id.entry(id.clone()).or_insert_with(|| CatalogItemEntry {
            name: prettify_item_id(&id),
            mod_ns: namespace_of(&id),
            id,
        });
    }

    Ok(by_id.into_values().collect())
}

/// Resolve project dir + mc version from a manifest path, then build the catalog.
pub fn build_item_catalog_for_manifest(
    manifest_path: &Path,
    extra_ids: impl IntoIterator<Item = String>,
) -> Result<Vec<CatalogItemEntry>, String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest path has no parent".to_string())?;
    let manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    build_item_catalog(project_dir, &manifest.minecraft.version, extra_ids)
}

fn catalog_jar_sources(project_dir: &Path, mc_version: &str) -> Vec<PathBuf> {
    let mut jars = vanilla_client_jars(mc_version);
    let mods_dir = project_dir.join("mods");
    if mods_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with(".jar") && !name.ends_with(".disabled") {
                    jars.push(path);
                }
            }
        }
    }
    jars
}

fn vanilla_client_jars(mc_version: &str) -> Vec<PathBuf> {
    let mut jars = Vec::new();
    let mut roots = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(&appdata).join("TuffBox"));
        roots.push(PathBuf::from(&appdata).join(".minecraft"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".local/share/TuffBox"));
        roots.push(PathBuf::from(&home).join(".minecraft"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("TuffBox"));
    }
    if let Some(data) = dirs::data_dir() {
        roots.push(data.join("TuffBox"));
    }
    for root in roots {
        let jar = root
            .join("versions")
            .join(mc_version)
            .join(format!("{mc_version}.jar"));
        if jar.is_file() && !jars.iter().any(|p| p == &jar) {
            jars.push(jar);
        }
    }
    jars
}

fn catalog_fingerprint(mc_version: &str, jars: &[PathBuf]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(mc_version.as_bytes());
    for jar in jars {
        hasher.update(jar.to_string_lossy().as_bytes());
        if let Ok(meta) = std::fs::metadata(jar) {
            hasher.update(meta.len().to_le_bytes());
            if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                    hasher.update(dur.as_secs().to_le_bytes());
                }
            }
        }
    }
    format!("{:x}", hasher.finalize())
}

fn collect_lang_from_archive(archive: &mut ZipArchive<std::fs::File>, lang: &mut HashMap<String, String>) {
    let names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
    for name in names {
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with("/lang/en_us.json") || lower.ends_with("\\lang\\en_us.json")) {
            continue;
        }
        let Ok(mut entry) = archive.by_name(&name) else {
            continue;
        };
        let mut content = String::new();
        if entry.read_to_string(&mut content).is_err() || content.len() > 8 * 1024 * 1024 {
            continue;
        }
        let Ok(map) = serde_json::from_str::<HashMap<String, String>>(&content) else {
            continue;
        };
        for (k, v) in map {
            if k.starts_with("item.") || k.starts_with("block.") {
                lang.entry(k).or_insert(v);
            }
        }
    }
}

fn collect_models_from_archive(
    archive: &mut ZipArchive<std::fs::File>,
    by_id: &mut BTreeMap<String, CatalogItemEntry>,
    lang: &HashMap<String, String>,
) {
    let names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
    for name in names {
        // assets/<ns>/models/item/<path>.json
        let parts: Vec<&str> = name.split('/').collect();
        if parts.len() < 5 || parts[0] != "assets" || parts[2] != "models" || parts[3] != "item" {
            continue;
        }
        if !name.ends_with(".json") {
            continue;
        }
        let ns = parts[1];
        if ns.is_empty() || ns == "_generated" {
            continue;
        }
        let path = parts[4..]
            .join("/")
            .trim_end_matches(".json")
            .replace('\\', "/");
        if path.is_empty() || path.contains('/') {
            // Nested item model paths are rare; still accept as id with /
            // Minecraft item ids use _ not nested folders usually — keep nested as _
            // Actually nested like `foo/bar` → `ns:foo/bar` is valid for some mods.
        }
        let id = format!("{ns}:{path}");
        let name_disp = lookup_lang_name(lang, ns, &path).unwrap_or_else(|| prettify_item_id(&id));
        by_id.entry(id.clone()).or_insert(CatalogItemEntry {
            id,
            name: name_disp,
            mod_ns: ns.to_string(),
        });
    }
}

fn lookup_lang_name(lang: &HashMap<String, String>, ns: &str, path: &str) -> Option<String> {
    let dotted = path.replace('/', ".");
    let keys = [
        format!("item.{ns}.{dotted}"),
        format!("block.{ns}.{dotted}"),
        format!("item.{ns}.{path}"),
        format!("block.{ns}.{path}"),
    ];
    for k in keys {
        if let Some(v) = lang.get(&k) {
            if !v.is_empty() {
                return Some(v.clone());
            }
        }
    }
    None
}

fn namespace_of(id: &str) -> String {
    id.split_once(':')
        .map(|(ns, _)| ns.to_string())
        .unwrap_or_else(|| "minecraft".into())
}

fn prettify_item_id(id: &str) -> String {
    let path = id.split_once(':').map(|(_, p)| p).unwrap_or(id);
    path.replace('_', " ").replace('/', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prettifies_ids() {
        assert_eq!(prettify_item_id("minecraft:oak_planks"), "oak planks");
    }
}
