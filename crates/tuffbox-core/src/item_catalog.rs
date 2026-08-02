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

/// Build a catalog of item ids from jar assets (models, 1.21.4+ item defs, lang) plus extras.
/// Merges any extra ids (e.g. from recipes) that do not appear in jars.
pub fn build_item_catalog(
    project_dir: &Path,
    mc_version: &str,
    extra_ids: impl IntoIterator<Item = String>,
    extra_vanilla_roots: &[PathBuf],
) -> Result<Vec<CatalogItemEntry>, String> {
    let jars = catalog_jar_sources(project_dir, mc_version, extra_vanilla_roots);
    let fingerprint = catalog_fingerprint(mc_version, &jars);
    let cache_path = project_dir
        .join(".tuffbox")
        .join("cache")
        .join("item-catalog-v2.json");

    let mut by_id: BTreeMap<String, CatalogItemEntry> = BTreeMap::new();

    if let Ok(raw) = std::fs::read_to_string(&cache_path) {
        if let Ok(cached) = serde_json::from_str::<CatalogCache>(&raw) {
            if cached.fingerprint == fingerprint && !cached.items.is_empty() {
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
            collect_item_defs_from_archive(&mut archive, &mut by_id, &lang);
        }
        collect_ids_from_lang(&lang, &mut by_id);

        if !by_id.is_empty() {
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
    extra_vanilla_roots: &[PathBuf],
) -> Result<Vec<CatalogItemEntry>, String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest path has no parent".to_string())?;
    let manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    build_item_catalog(
        project_dir,
        &manifest.minecraft.version,
        extra_ids,
        extra_vanilla_roots,
    )
}

fn catalog_jar_sources(
    project_dir: &Path,
    mc_version: &str,
    extra_vanilla_roots: &[PathBuf],
) -> Vec<PathBuf> {
    let mut jars = vanilla_client_jars(mc_version, extra_vanilla_roots);
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

fn default_vanilla_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(&appdata).join("TuffBox"));
        roots.push(PathBuf::from(appdata).join(".minecraft"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".local/share/TuffBox"));
        roots.push(PathBuf::from(home).join(".minecraft"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("TuffBox"));
    }
    if let Some(data) = dirs::data_dir() {
        roots.push(data.join("TuffBox"));
    }
    roots
}

fn vanilla_client_jars(mc_version: &str, extra_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut jars = Vec::new();
    let mut roots = default_vanilla_roots();
    for root in extra_roots {
        if !roots.iter().any(|existing| existing == root) {
            roots.push(root.clone());
        }
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
    hasher.update(b"item-catalog-v2");
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

fn normalize_zip_path(name: &str) -> String {
    name.replace('\\', "/")
}

fn collect_lang_from_archive(archive: &mut ZipArchive<std::fs::File>, lang: &mut HashMap<String, String>) {
    let names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
    for name in names {
        let normalized = normalize_zip_path(&name);
        let lower = normalized.to_ascii_lowercase();
        if !lower.ends_with("/lang/en_us.json") {
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

/// `item.minecraft.diamond` / `block.minecraft.stone` → `minecraft:diamond` / `minecraft:stone`.
fn lang_key_to_item_id(key: &str) -> Option<String> {
    let rest = key.strip_prefix("item.").or_else(|| key.strip_prefix("block."))?;
    let mut parts = rest.split('.');
    let ns = parts.next()?;
    if ns.is_empty() {
        return None;
    }
    let path = parts.collect::<Vec<_>>().join("/");
    if path.is_empty() {
        return None;
    }
    Some(format!("{ns}:{path}"))
}

fn collect_ids_from_lang(lang: &HashMap<String, String>, by_id: &mut BTreeMap<String, CatalogItemEntry>) {
    for (key, display) in lang {
        let Some(id) = lang_key_to_item_id(key) else {
            continue;
        };
        let ns = namespace_of(&id);
        let name = if display.is_empty() {
            prettify_item_id(&id)
        } else {
            display.clone()
        };
        by_id.entry(id.clone()).or_insert(CatalogItemEntry {
            id,
            name,
            mod_ns: ns,
        });
    }
}

fn insert_catalog_entry(
    by_id: &mut BTreeMap<String, CatalogItemEntry>,
    lang: &HashMap<String, String>,
    ns: &str,
    path: &str,
) {
    if ns.is_empty() || ns == "_generated" || path.is_empty() {
        return;
    }
    let id = format!("{ns}:{path}");
    let name_disp = lookup_lang_name(lang, ns, path).unwrap_or_else(|| prettify_item_id(&id));
    by_id.entry(id.clone()).or_insert(CatalogItemEntry {
        id,
        name: name_disp,
        mod_ns: ns.to_string(),
    });
}

fn collect_models_from_archive(
    archive: &mut ZipArchive<std::fs::File>,
    by_id: &mut BTreeMap<String, CatalogItemEntry>,
    lang: &HashMap<String, String>,
) {
    let names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
    for name in names {
        let normalized = normalize_zip_path(&name);
        // assets/<ns>/models/item/<path>.json
        let parts: Vec<&str> = normalized.split('/').collect();
        if parts.len() < 5 || parts[0] != "assets" || parts[2] != "models" || parts[3] != "item" {
            continue;
        }
        if !normalized.ends_with(".json") {
            continue;
        }
        let ns = parts[1];
        let path = parts[4..]
            .join("/")
            .trim_end_matches(".json")
            .replace('\\', "/");
        insert_catalog_entry(by_id, lang, ns, &path);
    }
}

/// 1.21.4+ item definitions live under assets/<ns>/items/<path>.json.
fn collect_item_defs_from_archive(
    archive: &mut ZipArchive<std::fs::File>,
    by_id: &mut BTreeMap<String, CatalogItemEntry>,
    lang: &HashMap<String, String>,
) {
    let names: Vec<String> = archive.file_names().map(|s| s.to_string()).collect();
    for name in names {
        let normalized = normalize_zip_path(&name);
        let parts: Vec<&str> = normalized.split('/').collect();
        if parts.len() < 4 || parts[0] != "assets" || parts[2] != "items" {
            continue;
        }
        if !normalized.ends_with(".json") {
            continue;
        }
        let ns = parts[1];
        let path = parts[3..]
            .join("/")
            .trim_end_matches(".json")
            .replace('\\', "/");
        insert_catalog_entry(by_id, lang, ns, &path);
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

    #[test]
    fn lang_keys_map_to_item_ids() {
        assert_eq!(
            lang_key_to_item_id("item.minecraft.diamond"),
            Some("minecraft:diamond".into())
        );
        assert_eq!(
            lang_key_to_item_id("block.minecraft.stone"),
            Some("minecraft:stone".into())
        );
        assert_eq!(
            lang_key_to_item_id("item.create.brass_ingot"),
            Some("create:brass_ingot".into())
        );
        assert_eq!(
            lang_key_to_item_id("item.mod.sub.item"),
            Some("mod:sub/item".into())
        );
    }
}
