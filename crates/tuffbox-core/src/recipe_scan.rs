//! Scan all mod JAR / datapack / KubeJS recipes with JEI-style layout metadata.

use crate::environment::{McVersion, ModpackEnvironment};
use crate::manifest::{LoaderKind, ProjectManifest};
use crate::recipe_layout::{collect_item_ids, expand_layout_tags, layout_from_json, ScannedRecipe};
use crate::registry::AdapterRegistry;
use crate::tag_index::TagIndex;
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
        .join("recipe-scan-v2.json")
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

    // Expand #tag ingredients into concrete item alts for icon cycling.
    let extra_jars = vanilla_client_jars(&manifest.minecraft.version);
    let tags = TagIndex::build(project_dir, loader, &extra_jars);
    for recipe in &mut recipes {
        expand_layout_tags(&mut recipe.layout, &tags);
        let (inputs, output) = collect_item_ids(&recipe.layout);
        recipe.input_ids = inputs;
        recipe.output_id = output;
    }

    Ok(RecipeScanResult {
        recipes,
        jar_count,
        datapack_files,
        truncated,
        total_scanned,
    })
}

fn vanilla_client_jars(mc_version: &str) -> Vec<PathBuf> {
    let mut jars = Vec::new();
    let mut roots = Vec::new();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push(PathBuf::from(appdata).join("TuffBox"));
        roots.push(PathBuf::from(std::env::var_os("APPDATA").unwrap()).join(".minecraft"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".local/share/TuffBox"));
        roots.push(PathBuf::from(home).join(".minecraft"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("TuffBox"));
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
        let line = format!("  event.remove({{ id: '{}' }})\n", escape_js_single(id));
        if !existing.contains(&format!("id: '{}'", escape_js_single(id))) {
            existing.push_str(&line);
        }
    }
    if !existing.trim_end().ends_with("})") {
        existing.push_str("})\n");
    }

    std::fs::write(&path, existing).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Draft for a crafting / cooking / smithing / stonecutting recipe written via KubeJS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CraftDraft {
    /// `shaped` | `shapeless` | `smelting` | `blasting` | `smoking` | `campfire` | `smithing` | `stonecutting`
    /// When omitted, `shaped` bool selects shaped vs shapeless.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default = "default_true")]
    pub shaped: bool,
    /// Row-major 3×3 for crafting. Tags use `#ns:path`.
    #[serde(default)]
    pub grid: Vec<Option<String>>,
    pub output: String,
    #[serde(default = "default_one")]
    pub output_count: u32,
    #[serde(default)]
    pub replace_id: Option<String>,
    /// Single input for cooking / stonecutting (item or `#tag`).
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub xp: Option<f64>,
    #[serde(default)]
    pub cook_time: Option<u32>,
    /// Smithing template (1.20+). When set, emits 4-arg `event.smithing`.
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub addition: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_one() -> u32 {
    1
}

/// Draft for item-tag edits via `ServerEvents.tags('item', ...)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagDraft {
    /// Tag id with or without leading `#` (e.g. `c:apples` or `#c:apples`).
    pub tag_id: String,
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
    /// If true, emit `event.removeAll(tag)` before adds.
    #[serde(default)]
    pub remove_all: bool,
}

fn escape_js_single(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn ingredient_js(id: &str) -> String {
    format!("'{}'", escape_js_single(id))
}

fn slot_at(grid: &[Option<String>], i: usize) -> Option<&str> {
    grid.get(i).and_then(|s| s.as_deref()).filter(|s| !s.is_empty())
}

fn resolve_craft_kind(draft: &CraftDraft) -> String {
    if let Some(ref k) = draft.kind {
        let t = k.trim().to_ascii_lowercase();
        if !t.is_empty() {
            return t;
        }
    }
    if draft.shaped {
        "shaped".into()
    } else {
        "shapeless".into()
    }
}

fn normalize_tag_id(tag_id: &str) -> String {
    tag_id.trim().trim_start_matches('#').to_string()
}

/// Emit `event.shaped(...)` for a 3×3 grid (compact bounding box).
pub fn kubejs_shaped_line(grid: &[Option<String>], output: &str, count: u32) -> Result<String, String> {
    if output.trim().is_empty() {
        return Err("output item is required".into());
    }
    let mut min_r = 3usize;
    let mut max_r = 0usize;
    let mut min_c = 3usize;
    let mut max_c = 0usize;
    let mut any = false;
    for r in 0..3 {
        for c in 0..3 {
            if slot_at(grid, r * 3 + c).is_some() {
                any = true;
                min_r = min_r.min(r);
                max_r = max_r.max(r);
                min_c = min_c.min(c);
                max_c = max_c.max(c);
            }
        }
    }
    if !any {
        return Err("at least one input ingredient is required".into());
    }

    let mut key_map: Vec<(char, String)> = Vec::new();
    let mut next = b'A';
    let mut pattern_rows = Vec::new();
    for r in min_r..=max_r {
        let mut row = String::new();
        for c in min_c..=max_c {
            match slot_at(grid, r * 3 + c) {
                None => row.push(' '),
                Some(id) => {
                    let ch = if let Some((ch, _)) = key_map.iter().find(|(_, v)| v == id) {
                        *ch
                    } else {
                        if next > b'Z' {
                            return Err("too many distinct ingredients".into());
                        }
                        let ch = next as char;
                        next += 1;
                        key_map.push((ch, id.to_string()));
                        ch
                    };
                    row.push(ch);
                }
            }
        }
        pattern_rows.push(row);
    }

    let pattern = pattern_rows
        .iter()
        .map(|r| format!("'{}'", escape_js_single(r)))
        .collect::<Vec<_>>()
        .join(",");
    let keys = key_map
        .iter()
        .map(|(ch, id)| format!(" {}: {}", ch, ingredient_js(id)))
        .collect::<Vec<_>>()
        .join(",");
    let count = count.max(1);
    Ok(format!(
        "  event.shaped(Item.of('{}', {}), [{}], {{{} }})",
        escape_js_single(output),
        count,
        pattern,
        keys
    ))
}

/// Emit `event.shapeless(...)` from non-empty grid slots (max 9).
pub fn kubejs_shapeless_line(grid: &[Option<String>], output: &str, count: u32) -> Result<String, String> {
    if output.trim().is_empty() {
        return Err("output item is required".into());
    }
    let inputs: Vec<&str> = (0..9).filter_map(|i| slot_at(grid, i)).collect();
    if inputs.is_empty() {
        return Err("at least one input ingredient is required".into());
    }
    if inputs.len() > 9 {
        return Err("shapeless recipes support at most 9 ingredients".into());
    }
    let list = inputs
        .iter()
        .map(|id| ingredient_js(id))
        .collect::<Vec<_>>()
        .join(", ");
    let count = count.max(1);
    Ok(format!(
        "  event.shapeless(Item.of('{}', {}), [{}])",
        escape_js_single(output),
        count,
        list
    ))
}

fn cooking_method(kind: &str) -> Option<&'static str> {
    match kind {
        "smelting" => Some("smelting"),
        "blasting" => Some("blasting"),
        "smoking" => Some("smoking"),
        "campfire" | "campfire_cooking" | "campfirecooking" => Some("campfireCooking"),
        _ => None,
    }
}

/// Emit cooking recipe: smelting / blasting / smoking / campfireCooking.
pub fn kubejs_cooking_line(
    method: &str,
    output: &str,
    count: u32,
    input: &str,
    xp: Option<f64>,
    cook_time: Option<u32>,
) -> Result<String, String> {
    if output.trim().is_empty() {
        return Err("output item is required".into());
    }
    if input.trim().is_empty() {
        return Err("input ingredient is required".into());
    }
    let mut line = format!(
        "  event.{}(Item.of('{}', {}), {})",
        method,
        escape_js_single(output),
        count.max(1),
        ingredient_js(input.trim())
    );
    if let Some(xp) = xp {
        if xp > 0.0 {
            line.push_str(&format!(".xp({xp})"));
        }
    }
    if let Some(ticks) = cook_time {
        if ticks > 0 {
            line.push_str(&format!(".cookingTime({ticks})"));
        }
    }
    Ok(line)
}

/// Emit `event.stonecutting(...)`.
pub fn kubejs_stonecutting_line(output: &str, count: u32, input: &str) -> Result<String, String> {
    if output.trim().is_empty() {
        return Err("output item is required".into());
    }
    if input.trim().is_empty() {
        return Err("input ingredient is required".into());
    }
    Ok(format!(
        "  event.stonecutting(Item.of('{}', {}), {})",
        escape_js_single(output),
        count.max(1),
        ingredient_js(input.trim())
    ))
}

/// Emit `event.smithing(...)` (3-arg legacy or 4-arg with template).
pub fn kubejs_smithing_line(
    output: &str,
    count: u32,
    template: Option<&str>,
    base: &str,
    addition: &str,
) -> Result<String, String> {
    if output.trim().is_empty() {
        return Err("output item is required".into());
    }
    if base.trim().is_empty() || addition.trim().is_empty() {
        return Err("smithing requires base and addition".into());
    }
    let out = format!(
        "Item.of('{}', {})",
        escape_js_single(output),
        count.max(1)
    );
    if let Some(t) = template.map(str::trim).filter(|s| !s.is_empty()) {
        Ok(format!(
            "  event.smithing({}, {}, {}, {})",
            out,
            ingredient_js(t),
            ingredient_js(base.trim()),
            ingredient_js(addition.trim())
        ))
    } else {
        Ok(format!(
            "  event.smithing({}, {}, {})",
            out,
            ingredient_js(base.trim()),
            ingredient_js(addition.trim())
        ))
    }
}

fn craft_line_from_draft(draft: &CraftDraft) -> Result<String, String> {
    let kind = resolve_craft_kind(draft);
    match kind.as_str() {
        "shaped" => {
            let mut grid = draft.grid.clone();
            grid.resize(9, None);
            kubejs_shaped_line(&grid, &draft.output, draft.output_count)
        }
        "shapeless" => {
            let mut grid = draft.grid.clone();
            grid.resize(9, None);
            kubejs_shapeless_line(&grid, &draft.output, draft.output_count)
        }
        cooking if cooking_method(cooking).is_some() => {
            let method = cooking_method(cooking).unwrap();
            let input = draft
                .input
                .as_deref()
                .or_else(|| slot_at(&draft.grid, 4))
                .or_else(|| slot_at(&draft.grid, 0))
                .ok_or_else(|| "input ingredient is required".to_string())?;
            kubejs_cooking_line(
                method,
                &draft.output,
                draft.output_count,
                input,
                draft.xp,
                draft.cook_time,
            )
        }
        "stonecutting" | "stonecutter" => {
            let input = draft
                .input
                .as_deref()
                .or_else(|| slot_at(&draft.grid, 0))
                .or_else(|| slot_at(&draft.grid, 4))
                .ok_or_else(|| "input ingredient is required".to_string())?;
            kubejs_stonecutting_line(&draft.output, draft.output_count, input)
        }
        "smithing" => {
            let base = draft
                .base
                .as_deref()
                .or_else(|| slot_at(&draft.grid, 4))
                .ok_or_else(|| "smithing base is required".to_string())?;
            let addition = draft
                .addition
                .as_deref()
                .or_else(|| slot_at(&draft.grid, 5))
                .ok_or_else(|| "smithing addition is required".to_string())?;
            let template = draft
                .template
                .as_deref()
                .or_else(|| slot_at(&draft.grid, 3))
                .filter(|s| !s.is_empty());
            kubejs_smithing_line(
                &draft.output,
                draft.output_count,
                template,
                base,
                addition,
            )
        }
        other => Err(format!("unsupported recipe kind: {other}")),
    }
}

fn append_kubejs_recipe_lines(path: &Path, header: &str, lines: &[String]) -> Result<(), String> {
    let mut existing = if path.exists() {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };

    if existing.is_empty() {
        existing = String::from(header);
    } else if !existing.contains("ServerEvents.recipes") {
        existing.push_str("\nServerEvents.recipes(event => {\n");
    } else if let Some(idx) = existing.rfind("})") {
        existing = existing[..idx].to_string();
    }

    for line in lines {
        let trimmed = line.trim_end();
        if !existing.contains(trimmed) {
            existing.push_str(trimmed);
            existing.push('\n');
        }
    }
    if !existing.trim_end().ends_with("})") {
        existing.push_str("})\n");
    }
    std::fs::write(path, existing).map_err(|e| e.to_string())
}

fn append_kubejs_tag_lines(path: &Path, lines: &[String]) -> Result<(), String> {
    let header =
        "// Generated by TuffBox IDE — item tag editor\nServerEvents.tags('item', event => {\n";
    let mut existing = if path.exists() {
        std::fs::read_to_string(path).unwrap_or_default()
    } else {
        String::new()
    };

    if existing.is_empty() {
        existing = String::from(header);
    } else if !existing.contains("ServerEvents.tags") {
        existing.push_str("\nServerEvents.tags('item', event => {\n");
    } else if let Some(idx) = existing.rfind("})") {
        existing = existing[..idx].to_string();
    }

    for line in lines {
        let trimmed = line.trim_end();
        if !existing.contains(trimmed) {
            existing.push_str(trimmed);
            existing.push('\n');
        }
    }
    if !existing.trim_end().ends_with("})") {
        existing.push_str("})\n");
    }
    std::fs::write(path, existing).map_err(|e| e.to_string())
}

/// Append recipe lines into kubejs/server_scripts/tuffbox_recipe_adds.js.
pub fn write_kubejs_craft(project_dir: &Path, draft: &CraftDraft) -> Result<String, String> {
    if draft.grid.len() > 9 {
        return Err("grid must have at most 9 slots".into());
    }
    let craft_line = craft_line_from_draft(draft)?;

    let dir = project_dir.join("kubejs").join("server_scripts");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("tuffbox_recipe_adds.js");

    let mut lines = Vec::new();
    if let Some(ref replace_id) = draft.replace_id {
        let id = replace_id.trim();
        if !id.is_empty() {
            lines.push(format!("  event.remove({{ id: '{}' }})", escape_js_single(id)));
        }
    }
    lines.push(craft_line);

    append_kubejs_recipe_lines(
        &path,
        "// Generated by TuffBox IDE — recipe craft editor\nServerEvents.recipes(event => {\n",
        &lines,
    )?;
    Ok(path.to_string_lossy().to_string())
}

/// Append tag add/remove lines into kubejs/server_scripts/tuffbox_tag_edits.js.
pub fn write_kubejs_tags(project_dir: &Path, draft: &TagDraft) -> Result<String, String> {
    let tag = normalize_tag_id(&draft.tag_id);
    if tag.is_empty() || !tag.contains(':') {
        return Err("tag id must look like namespace:path".into());
    }
    if !draft.remove_all && draft.add.is_empty() && draft.remove.is_empty() {
        return Err("nothing to write — add or remove at least one entry".into());
    }

    let dir = project_dir.join("kubejs").join("server_scripts");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("tuffbox_tag_edits.js");

    let mut lines = Vec::new();
    let tag_js = ingredient_js(&tag);
    if draft.remove_all {
        lines.push(format!("  event.removeAll({tag_js})"));
    }
    for id in &draft.remove {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        lines.push(format!("  event.remove({tag_js}, {})", ingredient_js(id)));
    }
    for id in &draft.add {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        lines.push(format!("  event.add({tag_js}, {})", ingredient_js(id)));
    }
    if lines.is_empty() {
        return Err("nothing to write — add or remove at least one entry".into());
    }

    append_kubejs_tag_lines(&path, &lines)?;
    Ok(path.to_string_lossy().to_string())
}

/// List known item tags as `#namespace:path` for the recipe palette.
pub fn list_item_tags(
    project_dir: &Path,
    loader: LoaderKind,
    extra_jars: &[PathBuf],
) -> Vec<String> {
    TagIndex::build(project_dir, loader, extra_jars).list_tag_ids()
}

/// Direct members of an item tag (items and nested `#tags`), not fully expanded.
pub fn get_tag_entries(
    project_dir: &Path,
    loader: LoaderKind,
    extra_jars: &[PathBuf],
    tag_id: &str,
) -> Vec<String> {
    TagIndex::build(project_dir, loader, extra_jars).direct_entries(tag_id)
}

pub fn loader_kind_from_manifest(manifest_path: &Path) -> Result<LoaderKind, String> {
    let manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    Ok(manifest.loader.kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaped_emits_compact_pattern() {
        let mut grid = vec![None; 9];
        grid[0] = Some("minecraft:andesite".into());
        grid[2] = Some("minecraft:diorite".into());
        grid[4] = Some("#c:stones".into());
        grid[6] = Some("minecraft:diorite".into());
        grid[8] = Some("minecraft:andesite".into());
        let line = kubejs_shaped_line(&grid, "minecraft:stone", 3).unwrap();
        assert!(line.contains("event.shaped(Item.of('minecraft:stone', 3)"));
        assert!(line.contains("'A B'"));
        assert!(line.contains("' C '"));
        assert!(line.contains("'B A'"));
        assert!(line.contains("A: 'minecraft:andesite'"));
        assert!(line.contains("B: 'minecraft:diorite'"));
        assert!(line.contains("C: '#c:stones'"));
    }

    #[test]
    fn shapeless_skips_empty() {
        let mut grid = vec![None; 9];
        grid[1] = Some("minecraft:bone_meal".into());
        grid[7] = Some("minecraft:yellow_dye".into());
        let line = kubejs_shapeless_line(&grid, "minecraft:dandelion", 3).unwrap();
        assert_eq!(
            line,
            "  event.shapeless(Item.of('minecraft:dandelion', 3), ['minecraft:bone_meal', 'minecraft:yellow_dye'])"
        );
    }

    #[test]
    fn craft_requires_inputs() {
        let grid = vec![None; 9];
        assert!(kubejs_shaped_line(&grid, "minecraft:stone", 1).is_err());
        assert!(kubejs_shapeless_line(&grid, "minecraft:stone", 1).is_err());
    }

    #[test]
    fn smelting_and_stonecutting_emit() {
        let line = kubejs_cooking_line("smelting", "minecraft:glass", 1, "minecraft:sand", Some(0.1), Some(200))
            .unwrap();
        assert!(line.contains("event.smelting(Item.of('minecraft:glass', 1), 'minecraft:sand')"));
        assert!(line.contains(".xp(0.1)"));
        assert!(line.contains(".cookingTime(200)"));
        let cut = kubejs_stonecutting_line("minecraft:stick", 3, "#minecraft:planks").unwrap();
        assert_eq!(
            cut,
            "  event.stonecutting(Item.of('minecraft:stick', 3), '#minecraft:planks')"
        );
    }

    #[test]
    fn smithing_four_arg() {
        let line = kubejs_smithing_line(
            "minecraft:netherite_ingot",
            1,
            Some("minecraft:netherite_upgrade_smithing_template"),
            "minecraft:iron_ingot",
            "minecraft:netherite_ingot",
        )
        .unwrap();
        assert!(line.contains("event.smithing(Item.of('minecraft:netherite_ingot', 1)"));
        assert!(line.contains("'minecraft:netherite_upgrade_smithing_template'"));
    }

    #[test]
    fn tag_draft_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_kubejs_tags(
            dir.path(),
            &TagDraft {
                tag_id: "#c:apples".into(),
                add: vec!["minecraft:apple".into(), "#c:golden_apples".into()],
                remove: vec!["minecraft:poisonous_potato".into()],
                remove_all: false,
            },
        )
        .unwrap();
        let body = std::fs::read_to_string(path).unwrap();
        assert!(body.contains("ServerEvents.tags('item'"));
        assert!(body.contains("event.add('c:apples', 'minecraft:apple')"));
        assert!(body.contains("event.add('c:apples', '#c:golden_apples')"));
        assert!(body.contains("event.remove('c:apples', 'minecraft:poisonous_potato')"));
    }
}
