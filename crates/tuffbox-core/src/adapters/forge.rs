use super::*;
use crate::environment::LoaderKind;
use crate::unified::tag::tag_id_from_path;
use crate::unified::recipe::*;

pub struct ForgeAdapter;

impl LoaderAdapter for ForgeAdapter {
    fn supported_loaders(&self) -> &[LoaderKind] {
        &[LoaderKind::Forge]
    }

    fn extract_metadata(
        &self,
        archive: &mut zip::ZipArchive<std::fs::File>,
    ) -> Result<ModMetadata, AdapterError> {
        if let Ok(mut entry) = archive.by_name("META-INF/mods.toml") {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return Self::parse_mods_toml(&content);
        }

        if let Ok(mut entry) = archive.by_name("mcmod.info") {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            return Self::parse_mcmod_info(&content);
        }

        Err(AdapterError::NoMetadata)
    }

    fn recipe_paths(&self, archive: &zip::ZipArchive<std::fs::File>) -> Vec<String> {
        archive
            .file_names()
            .filter(|name| {
                let parts: Vec<&str> = name.split('/').collect();
                parts.len() >= 4
                    && parts[0] == "data"
                    && parts[2] == "recipes"
                    && name.ends_with(".json")
            })
            .map(|s| s.to_string())
            .collect()
    }

    fn item_tag_paths(&self, archive: &zip::ZipArchive<std::fs::File>) -> Vec<String> {
        archive
            .file_names()
            .filter(|name| {
                let parts: Vec<&str> = name.split('/').collect();
                parts.len() >= 5
                    && parts[0] == "data"
                    && parts[2] == "tags"
                    && (parts[3] == "items" || parts[3] == "blocks")
                    && name.ends_with(".json")
            })
            .map(|s| s.to_string())
            .collect()
    }

    fn config_file_patterns(&self) -> Vec<ConfigPattern> {
        vec![
            ConfigPattern { path_pattern: "config/*.toml".to_string(), format: ConfigFileFormat::Toml },
            ConfigPattern { path_pattern: "config/*/*.toml".to_string(), format: ConfigFileFormat::Toml },
            ConfigPattern { path_pattern: "config/*.cfg".to_string(), format: ConfigFileFormat::Cfg },
        ]
    }

    fn parse_recipe(
        &self,
        json: &serde_json::Value,
        file_path: &str,
        mc_version: &McVersion,
    ) -> Result<UnifiedRecipe, AdapterError> {
        let recipe_type = json
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        parser_for_type(recipe_type, mc_version)
            .parse(json, file_path, mc_version)
            .map_err(|e| AdapterError::Parse(e.to_string()))
    }

    fn parse_tag(
        &self,
        json: &serde_json::Value,
        file_path: &str,
    ) -> Result<UnifiedTag, AdapterError> {
        let replace = json.get("replace").and_then(|v| v.as_bool()).unwrap_or(false);

        let values: Vec<TagEntry> = json
            .get("values")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        if let Some(obj) = v.as_object() {
                            let id = obj.get("id")?.as_str()?.to_string();
                            let required = obj.get("required").and_then(|r| r.as_bool()).unwrap_or(true);
                            Some(TagEntry { id, required })
                        } else if let Some(s) = v.as_str() {
                            Some(TagEntry { id: s.to_string(), required: true })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let tag_id = tag_id_from_path(file_path)
            .ok_or_else(|| AdapterError::InvalidPath(file_path.to_string()))?;

        Ok(UnifiedTag { id: tag_id, entries: values, replace })
    }
}

impl ForgeAdapter {
    pub fn parse_mods_toml(content: &str) -> Result<ModMetadata, AdapterError> {
        let table: toml::Table = content.parse().map_err(|e: toml::de::Error| AdapterError::Parse(e.to_string()))?;

        let mods = table
            .get("mods")
            .and_then(|v| v.as_array())
            .ok_or(AdapterError::NoMetadata)?;

        let first_mod = mods.first().ok_or(AdapterError::NoMetadata)?;

        let mod_id = first_mod.get("modId").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let display_name = first_mod.get("displayName").and_then(|v| v.as_str()).unwrap_or(&mod_id).to_string();
        let version = first_mod.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0").to_string();

        Ok(ModMetadata {
            namespace: mod_id.clone(),
            mod_id,
            display_name,
            version,
            dependencies: Vec::new(),
        })
    }

    pub fn parse_mcmod_info(content: &str) -> Result<ModMetadata, AdapterError> {
        let clean = content.trim().trim_start_matches('\u{feff}');
        let json: serde_json::Value = serde_json::from_str(clean)
            .map_err(|e| AdapterError::Parse(e.to_string()))?;

        let mod_info = if let Some(arr) = json.as_array() {
            arr.first().cloned()
        } else if let Some(list) = json.get("modList").and_then(|v| v.as_array()) {
            list.first().cloned()
        } else {
            Some(json.clone())
        };

        let mod_info = mod_info.ok_or(AdapterError::NoMetadata)?;

        Ok(ModMetadata {
            mod_id: mod_info.get("modid").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            display_name: mod_info.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown").to_string(),
            version: mod_info.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0").to_string(),
            namespace: mod_info.get("modid").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            dependencies: Vec::new(),
        })
    }
}
