//! Scan all mod JAR / datapack / KubeJS recipes with JEI-style layout metadata.

use crate::environment::{McVersion, ModpackEnvironment};
use crate::manifest::{LoaderKind, ProjectManifest};
use crate::recipe_layout::{collect_item_ids, layout_from_json, ScannedRecipe};
use crate::registry::AdapterRegistry;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_RECIPES: usize = 8000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeScanResult {
    pub recipes: Vec<ScannedRecipe>,
    pub jar_count: u32,
    pub datapack_files: u32,
    pub truncated: bool,
    pub total_scanned: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KubeJsScript {
    pub kind: String,
    pub filename: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecipeScanCacheFile {
    fingerprint: String,
    #[serde(flatten)]
    result: RecipeScanResult,
}

fn recipe_scan_cache_path(project_dir: &Path) -> PathBuf {
    project_dir
        .join(".tuffbox")
        .join("cache")
        .join("recipe-scan-v1.json")
}

fn hash_path_entry(hasher: &mut DefaultHasher, path: &Path) {
    path.file_name().hash(hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.modified().ok().hash(hasher);
        meta.len().hash(hasher);
    }
}

fn hash_json_tree(hasher: &mut DefaultHasher, root: &Path) {
    if !root.is_dir() {
        return;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                hash_path_entry(hasher, &path);
            }
        }
    }
}

fn recipe_scan_fingerprint(manifest_path: &Path, project_dir: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    hash_path_entry(&mut hasher, manifest_path);

    let mods_dir = project_dir.join("mods");
    if mods_dir.is_dir() {
        let mut jars: Vec<PathBuf> = std::fs::read_dir(&mods_dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("jar"))
            .collect();
        jars.sort();
        for jar in jars {
            hash_path_entry(&mut hasher, &jar);
        }
    }

    hash_json_tree(&mut hasher, &project_dir.join("datapacks"));
    hash_json_tree(&mut hasher, &project_dir.join("kubejs").join("data"));
    let saves = project_dir.join("saves");
    if saves.is_dir() {
        if let Ok(worlds) = std::fs::read_dir(&saves) {
            for world in worlds.flatten() {
                hash_json_tree(&mut hasher, &world.path().join("datapacks"));
            }
        }
    }

    format!("{:016x}", hasher.finish())
}

pub fn scan_project_recipes(manifest_path: &Path) -> Result<RecipeScanResult, String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest has no parent".to_string())?;
    let fingerprint = recipe_scan_fingerprint(manifest_path, project_dir);
    let cache_path = recipe_scan_cache_path(project_dir);

    if let Ok(raw) = std::fs::read_to_string(&cache_path) {
        if let Ok(cached) = serde_json::from_str::<RecipeScanCacheFile>(&raw) {
            if cached.fingerprint == fingerprint {
                return Ok(cached.result);
            }
        }
    }

    let result = scan_project_recipes_uncached(manifest_path)?;

    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
        if let Ok(raw) = serde_json::to_string(&RecipeScanCacheFile {
            fingerprint,
            result: result.clone(),
        }) {
            let _ = std::fs::write(&cache_path, raw);
        }
    }

    Ok(result)
}

fn scan_project_recipes_uncached(manifest_path: &Path) -> Result<RecipeScanResult, String> {
    let manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest has no parent".to_string())?;

    let mc_version =
        McVersion::parse(&manifest.minecraft.version).unwrap_or(McVersion::new(1, 21, 0));
    let loader = manifest.loader.kind;
    let env = ModpackEnvironment {
        mc_version: mc_version.clone(),
        loader,
        loader_version: Some(manifest.loader.version.clone()),
        root_path: project_dir.to_path_buf(),
        data_epoch: mc_version.data_epoch(),
        tag_namespace: mc_version.tag_namespace(),
    };

    let registry = AdapterRegistry::new();
    let adapter = registry
        .get_adapter(loader)
        .ok_or_else(|| format!("unsupported loader: {:?}", loader))?;

    let mut recipes = Vec::new();
    let mut jar_count = 0u32;
    let mut datapack_files = 0u32;
    let mut total_scanned = 0u32;
    let mut truncated = false;

    // 1) Mod JARs
    let mods_dir = project_dir.join("mods");
    if mods_dir.is_dir() {
        for entry in std::fs::read_dir(&mods_dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().map_or(true, |e| e != "jar") {
                continue;
            }
            jar_count += 1;
            let mod_source = entry
                .file_name()
                .to_string_lossy()
                .trim_end_matches(".jar")
                .to_string();

            let file = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let mut archive = match zip::ZipArchive::new(file) {
                Ok(a) => a,
                Err(_) => continue,
            };
            let recipe_paths = adapter.recipe_paths(&archive);

            for rpath in recipe_paths {
                total_scanned += 1;
                if recipes.len() >= MAX_RECIPES {
                    truncated = true;
                    break;
                }
                let Ok(mut zip_entry) = archive.by_name(&rpath) else {
                    continue;
                };
                let mut content = String::new();
                if zip_entry.read_to_string(&mut content).is_err() || content.len() > 128 * 1024 {
                    continue;
                }
                if let Some(scanned) =
                    try_parse_recipe(adapter, &content, &rpath, &mod_source, &env.mc_version)
                {
                    recipes.push(scanned);
                }
            }
            if truncated {
                break;
            }
        }
    }

    // 2) Project datapacks/ + world datapacks
    if !truncated {
        for root in datapack_roots(project_dir) {
            let (added, scanned, files) = scan_filesystem_recipes(
                adapter,
                &root,
                "datapack",
                &env.mc_version,
                MAX_RECIPES - recipes.len(),
            );
            datapack_files += files;
            total_scanned += scanned;
            recipes.extend(added);
            if recipes.len() >= MAX_RECIPES {
                truncated = true;
                break;
            }
        }
    }

    // 3) KubeJS generated data
    if !truncated {
        let kubejs_data = project_dir.join("kubejs").join("data");
        if kubejs_data.is_dir() {
            let (added, scanned, files) = scan_filesystem_recipes(
                adapter,
                &kubejs_data,
                "kubejs",
                &env.mc_version,
                MAX_RECIPES - recipes.len(),
            );
            datapack_files += files;
            total_scanned += scanned;
            recipes.extend(added);
            if recipes.len() >= MAX_RECIPES {
                truncated = true;
            }
        }
    }

    recipes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(RecipeScanResult {
        recipes,
        jar_count,
        datapack_files,
        truncated,
        total_scanned,
    })
}

fn datapack_roots(project_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let top = project_dir.join("datapacks");
    if top.is_dir() {
        roots.push(top);
    }
    let saves = project_dir.join("saves");
    if saves.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&saves) {
            for entry in entries.flatten() {
                let dp = entry.path().join("datapacks");
                if dp.is_dir() {
                    roots.push(dp);
                }
            }
        }
    }
    roots
}

fn scan_filesystem_recipes(
    adapter: &dyn crate::adapters::LoaderAdapter,
    root: &Path,
    mod_source: &str,
    mc_version: &McVersion,
    remaining: usize,
) -> (Vec<ScannedRecipe>, u32, u32) {
    let mut recipes = Vec::new();
    let mut scanned = 0u32;
    let mut files = 0u32;
    if remaining == 0 {
        return (recipes, scanned, files);
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if !is_recipe_rel_path(&rel_str) {
                continue;
            }
            files += 1;
            scanned += 1;
            if recipes.len() >= remaining {
                return (recipes, scanned, files);
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            if content.len() > 128 * 1024 {
                continue;
            }
            let virtual_path = if rel_str.starts_with("data/") {
                rel_str.clone()
            } else if let Some(idx) = rel_str.find("/data/") {
                rel_str[idx + 1..].to_string()
            } else {
                format!(
                    "data/datapack/recipes/{}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            };
            if let Some(scanned_recipe) =
                try_parse_recipe(adapter, &content, &virtual_path, mod_source, mc_version)
            {
                recipes.push(scanned_recipe);
            }
        }
    }
    (recipes, scanned, files)
}

fn is_recipe_rel_path(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    // data/<ns>/recipes/*.json or data/<ns>/recipe/*.json
    // or <pack>/data/<ns>/recipes/*.json
    parts
        .windows(3)
        .any(|w| w[0] == "data" && (w[2] == "recipes" || w[2] == "recipe"))
        || parts.windows(2).any(|w| {
            (w[0] == "recipes" || w[0] == "recipe")
                && parts.last().map(|p| p.ends_with(".json")).unwrap_or(false)
        })
}

fn try_parse_recipe(
    adapter: &dyn crate::adapters::LoaderAdapter,
    content: &str,
    rpath: &str,
    mod_source: &str,
    mc_version: &McVersion,
) -> Option<ScannedRecipe> {
    let json: serde_json::Value = serde_json::from_str(content).ok()?;
    let unified = adapter.parse_recipe(&json, rpath, mc_version).ok()?;
    let layout = layout_from_json(&json, &unified.recipe_type);
    let (input_ids, output_id) = collect_item_ids(&layout);
    Some(ScannedRecipe {
        id: unified.id,
        recipe_type: unified.recipe_type.clone(),
        category: layout.category.clone(),
        mod_source: mod_source.to_string(),
        source_file: rpath.to_string(),
        layout,
        input_ids,
        output_id,
        is_conditional: unified.is_conditional,
    })
}

/// Generate a KubeJS 6 server script that removes the given recipe ids.
pub fn kubejs_remove_script(recipe_ids: &[String]) -> KubeJsScript {
    let mut body = String::from("ServerEvents.recipes(event => {\n");
    for id in recipe_ids {
        body.push_str(&format!("  event.remove({{ id: '{}' }})\n", id));
    }
    body.push_str("})\n");
    KubeJsScript {
        kind: "remove".into(),
        filename: "tuffbox_recipe_removes.js".into(),
        content: body,
    }
}

/// Generate a KubeJS replace-output snippet for one recipe.
pub fn kubejs_replace_output(recipe_id: &str, new_item: &str, count: u32) -> KubeJsScript {
    let content = format!(
        "ServerEvents.recipes(event => {{\n  event.replaceOutput({{ id: '{recipe_id}' }}, '{new_item}', Item.of('{new_item}', {count}))\n}})\n"
    );
    KubeJsScript {
        kind: "replace_output".into(),
        filename: "tuffbox_recipe_replace.js".into(),
        content,
    }
}

/// Write (append or create) a KubeJS remove script under kubejs/server_scripts/.
pub fn write_kubejs_remove(project_dir: &Path, recipe_ids: &[String]) -> Result<String, String> {
    if recipe_ids.is_empty() {
        return Err("no recipe ids".into());
    }
    let dir = project_dir.join("kubejs").join("server_scripts");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("tuffbox_recipe_removes.js");

    let mut existing = if path.exists() {
        std::fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };

    if existing.is_empty() {
        existing = String::from(
            "// Generated by TuffBox IDE — JEI recipe browser\nServerEvents.recipes(event => {\n",
        );
    } else if !existing.contains("ServerEvents.recipes") {
        existing.push_str("\nServerEvents.recipes(event => {\n");
    } else {
        // Insert before closing `})`
        if let Some(idx) = existing.rfind("})") {
            existing = existing[..idx].to_string();
        }
    }

    for id in recipe_ids {
        let line = format!("  event.remove({{ id: '{}' }})\n", id);
        if !existing.contains(&format!("id: '{}'", id)) {
            existing.push_str(&line);
        }
    }
    if !existing.trim_end().ends_with("})") {
        existing.push_str("})\n");
    }

    std::fs::write(&path, existing).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

pub fn loader_kind_from_manifest(manifest_path: &Path) -> Result<LoaderKind, String> {
    let manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    Ok(manifest.loader.kind)
}
