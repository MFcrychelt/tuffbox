//! Scan all mod JAR recipes with JEI-style layout metadata.

use crate::environment::{McVersion, ModpackEnvironment};
use crate::manifest::{LoaderKind, ProjectManifest};
use crate::recipe_layout::{collect_item_ids, layout_from_json, ScannedRecipe};
use crate::registry::AdapterRegistry;
use std::io::Read;
use std::path::Path;

const MAX_RECIPES: usize = 5000;

pub fn scan_project_recipes(manifest_path: &Path) -> Result<Vec<ScannedRecipe>, String> {
    let manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest has no parent".to_string())?;

    let mc_version = McVersion::parse(&manifest.minecraft.version).unwrap_or(McVersion::new(1, 21, 0));
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

    let mods_dir = project_dir.join("mods");
    if !mods_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut recipes = Vec::new();
    for entry in std::fs::read_dir(&mods_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().map_or(true, |e| e != "jar") {
            continue;
        }
        let mod_source = entry
            .file_name()
            .to_string_lossy()
            .trim_end_matches(".jar")
            .to_string();

        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        let recipe_paths = adapter.recipe_paths(&archive);

        for rpath in recipe_paths {
            if recipes.len() >= MAX_RECIPES {
                return Ok(recipes);
            }
            let Ok(mut zip_entry) = archive.by_name(&rpath) else {
                continue;
            };
            let mut content = String::new();
            if zip_entry.read_to_string(&mut content).is_err() || content.len() > 128 * 1024 {
                continue;
            }
            let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
                continue;
            };
            let Ok(unified) = adapter.parse_recipe(&json, &rpath, &env.mc_version) else {
                continue;
            };
            let layout = layout_from_json(&json, &unified.recipe_type);
            let (input_ids, output_id) = collect_item_ids(&layout);
            recipes.push(ScannedRecipe {
                id: unified.id,
                recipe_type: unified.recipe_type.clone(),
                category: layout.category.clone(),
                mod_source: mod_source.clone(),
                source_file: rpath,
                layout,
                input_ids,
                output_id,
                is_conditional: unified.is_conditional,
            });
        }
    }

    recipes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(recipes)
}

pub fn loader_kind_from_manifest(manifest_path: &Path) -> Result<LoaderKind, String> {
    let manifest = ProjectManifest::load_from_path(manifest_path).map_err(|e| e.to_string())?;
    Ok(manifest.loader.kind)
}
