use crate::environment::McVersion;

#[derive(Debug, Clone)]
pub struct UnifiedRecipe {
    pub id: String,
    pub recipe_type: String,
    pub inputs: Vec<UnifiedIngredient>,
    pub output: UnifiedOutput,
    pub source_file: String,
    pub is_conditional: bool,
}

#[derive(Debug, Clone)]
pub enum UnifiedIngredient {
    Item(String),
    Tag(String),
    OneOf(Vec<UnifiedIngredient>),
}

#[derive(Debug, Clone)]
pub struct UnifiedOutput {
    pub item: String,
    pub count: u32,
}

pub trait RecipeParser: Send + Sync {
    fn parse(
        &self,
        json: &serde_json::Value,
        file_path: &str,
        mc_version: &McVersion,
    ) -> Result<UnifiedRecipe, RecipeError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RecipeError {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Unsupported recipe type: {0}")]
    UnsupportedType(String),
}

pub struct ShapedRecipeParser;
pub struct ShapedRecipeParser121;
pub struct ShapelessRecipeParser;
pub struct ShapelessRecipeParser121;
pub struct CookingRecipeParser;
pub struct LegacySmithingParser;
pub struct SmithingTransformParser;
pub struct ForgeConditionalRecipeParser;
pub struct NeoForgeConditionalParser;
pub struct GenericRecipeParser;

impl RecipeParser for ShapedRecipeParser {
    fn parse(
        &self,
        json: &serde_json::Value,
        file_path: &str,
        _mc_version: &McVersion,
    ) -> Result<UnifiedRecipe, RecipeError> {
        let key = json.get("key").and_then(|v| v.as_object());
        let mut inputs = Vec::new();

        if let Some(key_map) = key {
            for (_symbol, ingredient) in key_map {
                if let Some(ing) = parse_ingredient_value(ingredient) {
                    inputs.push(ing);
                }
            }
        }

        let output = parse_result_value(json)?;

        Ok(UnifiedRecipe {
            id: recipe_id_from_path(file_path),
            recipe_type: "minecraft:crafting_shaped".to_string(),
            inputs,
            output,
            source_file: file_path.to_string(),
            is_conditional: json.get("conditions").is_some(),
        })
    }
}

impl RecipeParser for ShapedRecipeParser121 {
    fn parse(
        &self,
        json: &serde_json::Value,
        file_path: &str,
        _mc_version: &McVersion,
    ) -> Result<UnifiedRecipe, RecipeError> {
        let key = json.get("key").and_then(|v| v.as_object());
        let mut inputs = Vec::new();

        if let Some(key_map) = key {
            for (_symbol, ingredient) in key_map {
                if let Some(s) = ingredient.as_str() {
                    if s.starts_with('#') {
                        inputs.push(UnifiedIngredient::Tag(s.to_string()));
                    } else {
                        inputs.push(UnifiedIngredient::Item(s.to_string()));
                    }
                } else if let Some(ing) = parse_ingredient_value(ingredient) {
                    inputs.push(ing);
                }
            }
        }

        let output = parse_result_121(json).or_else(|_| parse_result_value(json))?;

        Ok(UnifiedRecipe {
            id: recipe_id_from_path(file_path),
            recipe_type: "minecraft:crafting_shaped".to_string(),
            inputs,
            output,
            source_file: file_path.to_string(),
            is_conditional: json.get("neoforge:conditions").is_some(),
        })
    }
}

impl RecipeParser for GenericRecipeParser {
    fn parse(
        &self,
        json: &serde_json::Value,
        file_path: &str,
        _mc_version: &McVersion,
    ) -> Result<UnifiedRecipe, RecipeError> {
        let recipe_type = json
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let mut all_item_refs = Vec::new();
        extract_all_item_references(json, &mut all_item_refs);

        let output = parse_result_value(json)
            .or_else(|_| {
                for key in &["output", "outputs", "result", "results"] {
                    if let Some(val) = json.get(*key) {
                        if let Ok(out) = try_parse_output(val) {
                            return Ok(out);
                        }
                    }
                }
                Err(RecipeError::Parse("No output found".to_string()))
            })
            .unwrap_or(UnifiedOutput {
                item: "unknown:unknown".to_string(),
                count: 1,
            });

        let inputs = all_item_refs
            .into_iter()
            .filter(|r| *r != output.item)
            .map(|r| {
                if r.starts_with('#') {
                    UnifiedIngredient::Tag(r)
                } else {
                    UnifiedIngredient::Item(r)
                }
            })
            .collect();

        Ok(UnifiedRecipe {
            id: recipe_id_from_path(file_path),
            recipe_type,
            inputs,
            output,
            source_file: file_path.to_string(),
            is_conditional: false,
        })
    }
}

pub fn parse_ingredient_value(value: &serde_json::Value) -> Option<UnifiedIngredient> {
    if let Some(item) = value.get("item").and_then(|v| v.as_str()) {
        return Some(UnifiedIngredient::Item(item.to_string()));
    }
    if let Some(tag) = value.get("tag").and_then(|v| v.as_str()) {
        return Some(UnifiedIngredient::Tag(format!("#{}", tag)));
    }
    if let Some(arr) = value.as_array() {
        let alts: Vec<UnifiedIngredient> = arr
            .iter()
            .filter_map(parse_ingredient_value)
            .collect();
        if !alts.is_empty() {
            return Some(UnifiedIngredient::OneOf(alts));
        }
    }
    if let Some(s) = value.as_str() {
        if s.starts_with('#') {
            return Some(UnifiedIngredient::Tag(s.to_string()));
        } else {
            return Some(UnifiedIngredient::Item(s.to_string()));
        }
    }
    None
}

pub fn parse_result_value(json: &serde_json::Value) -> Result<UnifiedOutput, RecipeError> {
    let result = json
        .get("result")
        .ok_or_else(|| RecipeError::Parse("no result field".to_string()))?;

    if let Some(s) = result.as_str() {
        return Ok(UnifiedOutput { item: s.to_string(), count: 1 });
    }

    if let Some(obj) = result.as_object() {
        let item = obj
            .get("item")
            .or_else(|| obj.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| RecipeError::Parse("no item in result".to_string()))?;
        let count = obj
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
        return Ok(UnifiedOutput { item: item.to_string(), count });
    }

    Err(RecipeError::Parse("unparseable result".to_string()))
}

pub fn parse_result_121(json: &serde_json::Value) -> Result<UnifiedOutput, RecipeError> {
    let result = json
        .get("result")
        .ok_or_else(|| RecipeError::Parse("no result".to_string()))?;

    if let Some(obj) = result.as_object() {
        let item = obj
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RecipeError::Parse("no id in 1.21 result".to_string()))?;
        let count = obj
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
        return Ok(UnifiedOutput { item: item.to_string(), count });
    }

    if let Some(s) = result.as_str() {
        return Ok(UnifiedOutput { item: s.to_string(), count: 1 });
    }

    Err(RecipeError::Parse("unparseable 1.21 result".to_string()))
}

pub fn try_parse_output(value: &serde_json::Value) -> Result<UnifiedOutput, RecipeError> {
    if let Some(s) = value.as_str() {
        return Ok(UnifiedOutput { item: s.to_string(), count: 1 });
    }
    if let Some(obj) = value.as_object() {
        let item = obj
            .get("item")
            .or_else(|| obj.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| RecipeError::Parse("no item".to_string()))?;
        let count = obj
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as u32;
        return Ok(UnifiedOutput { item: item.to_string(), count });
    }
    if let Some(arr) = value.as_array() {
        if let Some(first) = arr.first() {
            return try_parse_output(first);
        }
    }
    Err(RecipeError::Parse("no output".to_string()))
}

pub fn extract_all_item_references(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) => {
            if s.contains(':') && !s.contains(' ') && !s.starts_with("minecraft:crafting") {
                out.push(s.clone());
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(item) = map.get("item").and_then(|v| v.as_str()) {
                out.push(item.to_string());
            }
            if let Some(tag) = map.get("tag").and_then(|v| v.as_str()) {
                out.push(format!("#{}", tag));
            }
            for (key, val) in map {
                if key != "type" && key != "group" && key != "category" {
                    extract_all_item_references(val, out);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for val in arr {
                extract_all_item_references(val, out);
            }
        }
        _ => {}
    }
}

pub fn recipe_id_from_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 4 && parts[0] == "data" {
        let namespace = parts[1];
        let rest = parts[3..].join("/");
        let name = rest.trim_end_matches(".json");
        format!("{}:{}", namespace, name)
    } else {
        path.to_string()
    }
}
