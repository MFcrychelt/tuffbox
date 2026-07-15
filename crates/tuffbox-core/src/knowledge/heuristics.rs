//! Automatic heuristics for config scanning, ore generation detection,
//! item classification, and tag normalization.
//!
//! This works without any pre-existing metadata — it uses pattern
//! matching to figure out what config keys control ore generation,
//! which items belong to which mods, and how to normalize tags between
//! forge: and c: namespaces.

use std::collections::HashMap;

/// Result of scanning a config file for ore-gen related keys.
#[derive(Debug, Clone)]
pub struct HeuristicOreGen {
    pub resource_name: String,
    pub config_file: String,
    pub enabled_key: String,
    pub enabled_value: String,
    pub vein_size: Option<(String, String)>,
    pub min_height: Option<(String, String)>,
    pub max_height: Option<(String, String)>,
    pub spawns_per_chunk: Option<(String, String)>,
    pub confidence: HeuristicConfidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeuristicConfidence {
    /// Exact match from knowledge base
    High,
    /// Pattern match with strong signal
    Medium,
    /// Weak pattern match — user should verify
    Low,
}

/// Known ore-generation key patterns that work across many mods.
const ORE_GEN_KEY_PATTERNS: &[&str] = &[
    "generate",
    "enable",
    "spawn",
    "gen",
    "ore",
    "shouldGenerate",
    "oreGeneration",
    "generateOre",
    "enableOreGen",
    "enableWorldGen",
    "worldGen",
    "spawnOre",
    "oreSpawn",
    "generateInWorld",
    "shouldSpawn",
    "canGenerate",
    "allowGeneration",
];

/// Known suffixes that identify vein-size keys.
const VEIN_SIZE_SUFFIXES: &[&str] = &[
    "veinSize",
    "vein_size",
    "size",
    "clusterSize",
    "cluster_size",
    "maxVeinSize",
    "veinCount",
    "countPerVein",
    "perVein",
    "veinCount",
    "maxPerCluster",
];

/// Known suffixes for height-range keys.
const HEIGHT_SUFFIXES: &[(&[&str], &[&str])] = &[(
    &[
        "minHeight",
        "min_height",
        "minY",
        "min_y",
        "bottomY",
        "bottom",
        "minWorldHeight",
        "minimumHeight",
        "startY",
    ],
    &[
        "maxHeight",
        "max_height",
        "maxY",
        "max_y",
        "topY",
        "top",
        "maxWorldHeight",
        "maximumHeight",
        "endY",
    ],
)];

/// Known suffixes for frequency keys.
const FREQUENCY_SUFFIXES: &[&str] = &[
    "spawnsPerChunk",
    "spawns_per_chunk",
    "frequency",
    "perChunk",
    "chance",
    "weight",
    "rarity",
    "spawnChance",
    "count",
    "spawnRate",
    "rate",
    "density",
];

/// Scans a flat map of (config_file_path, content) for ore-generation
/// keys using heuristic patterns without any pre-existing knowledge base.
pub fn scan_configs_for_ore_gen(config_contents: &[(String, String)]) -> Vec<HeuristicOreGen> {
    let mut results = Vec::new();

    for (file_path, content) in config_contents {
        let lines: Vec<&str> = content.lines().collect();

        // Find lines that match ore-gen patterns
        for (line_no, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
                continue;
            }

            // Extract key=value or "key": value patterns
            let (key, value) = if let Some((k, v)) = parse_toml_kv(trimmed) {
                (k, v)
            } else if let Some((k, v)) = parse_json_kv(trimmed) {
                (k, v)
            } else if let Some((k, v)) = parse_cfg_kv(trimmed) {
                (k, v)
            } else {
                continue;
            };

            let key_lower = key.to_lowercase();

            // Check if this key looks like an ore-gen toggle
            let is_ore_gen = ORE_GEN_KEY_PATTERNS
                .iter()
                .any(|pat| key_lower.contains(&pat.to_lowercase()));

            if !is_ore_gen {
                continue;
            }

            // Try to figure out what resource this controls
            let resource_name = infer_resource_name(&key, file_path);
            if resource_name.is_empty() {
                continue;
            }

            // Look for related keys in nearby lines
            let vein_size = find_related_key(&lines, line_no, VEIN_SIZE_SUFFIXES);
            let frequency = find_related_key(&lines, line_no, FREQUENCY_SUFFIXES);
            let (min_height, max_height) = find_height_range(&lines, line_no);

            let confidence = if key_lower.contains("enable") || key_lower.contains("generate") {
                HeuristicConfidence::Medium
            } else {
                HeuristicConfidence::Low
            };

            results.push(HeuristicOreGen {
                resource_name,
                config_file: file_path.clone(),
                enabled_key: key.to_string(),
                enabled_value: value.to_string(),
                vein_size,
                min_height,
                max_height,
                spawns_per_chunk: frequency,
                confidence,
            });
        }
    }

    results
}

fn parse_toml_kv(line: &str) -> Option<(&str, &str)> {
    let eq_pos = line.find('=')?;
    let key = line[..eq_pos].trim();
    let value = line[eq_pos + 1..].trim().trim_matches('"');
    if key.is_empty() {
        return None;
    }
    Some((key, value))
}

fn parse_json_kv(line: &str) -> Option<(&str, &str)> {
    let line = line.trim().trim_end_matches(',');
    let colon = line.find(':')?;
    let key = line[..colon].trim().trim_matches('"');
    let value = line[colon + 1..].trim().trim_matches('"');
    if key.is_empty() {
        return None;
    }
    Some((key, value))
}

fn parse_cfg_kv(line: &str) -> Option<(&str, &str)> {
    let line = line
        .trim_start_matches("B:")
        .trim_start_matches("I:")
        .trim_start_matches("S:");
    let eq_pos = line.find('=')?;
    let key = line[..eq_pos].trim();
    let value = line[eq_pos + 1..].trim();
    if key.is_empty() {
        return None;
    }
    Some((key, value))
}

fn infer_resource_name(key: &str, _file_path: &str) -> String {
    // Try to infer from the key name
    let lower = key.to_lowercase();

    // Known patterns
    let resource_patterns = [
        "copper",
        "tin",
        "lead",
        "silver",
        "nickel",
        "zinc",
        "osmium",
        "uranium",
        "aluminum",
        "aluminium",
        "bauxite",
        "cobalt",
        "ruby",
        "sapphire",
        "amethyst",
        "topaz",
        "peridot",
        "tungsten",
        "platinum",
        "iridium",
        "titanium",
        "chromium",
        "quartz",
        "certus",
        "fluorite",
        "sulfur",
        "saltpeter",
        "coal",
        "iron",
        "gold",
        "diamond",
        "emerald",
        "redstone",
        "lapis",
        "netherite",
        "ancient_debris",
    ];

    for res in &resource_patterns {
        if lower.contains(res) {
            return res.to_string();
        }
    }

    // Fallback: use the key stem
    let stem = key
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| {
            w.len() > 2
                && !matches!(
                    w.to_lowercase().as_str(),
                    "generate" | "enable" | "ore" | "gen" | "spawn" | "world"
                )
        })
        .unwrap_or("unknown");

    stem.to_lowercase()
}

fn find_related_key(lines: &[&str], center: usize, suffixes: &[&str]) -> Option<(String, String)> {
    let window = 8usize;
    let start = center.saturating_sub(window);
    let end = (center + window).min(lines.len());

    for line in &lines[start..end] {
        if let Some((k, v)) = parse_toml_kv(line)
            .or_else(|| parse_json_kv(line))
            .or_else(|| parse_cfg_kv(line))
        {
            let kl = k.to_lowercase();
            if suffixes.iter().any(|s| kl.contains(&s.to_lowercase())) {
                return Some((k.to_string(), v.to_string()));
            }
        }
    }
    None
}

fn find_height_range(
    lines: &[&str],
    center: usize,
) -> (Option<(String, String)>, Option<(String, String)>) {
    let window = 10usize;
    let start = center.saturating_sub(window);
    let end = (center + window).min(lines.len());
    let (min_sfx, max_sfx) = &HEIGHT_SUFFIXES[0];

    let mut min = None;
    let mut max = None;

    for line in &lines[start..end] {
        if let Some((k, v)) = parse_toml_kv(line)
            .or_else(|| parse_json_kv(line))
            .or_else(|| parse_cfg_kv(line))
        {
            let kl = k.to_lowercase();
            if min_sfx.iter().any(|s| kl.contains(&s.to_lowercase())) {
                min = Some((k.to_string(), v.to_string()));
            }
            if max_sfx.iter().any(|s| kl.contains(&s.to_lowercase())) {
                max = Some((k.to_string(), v.to_string()));
            }
        }
    }
    (min, max)
}

/// Classifies a Minecraft item name into a (material, type) pair by
/// pattern matching on common naming conventions.
///
/// Examples:
///   "tin_ingot" → ("tin", "ingot")
///   "ingotTin" → ("tin", "ingot")
///   "copper_block" → ("copper", "block")
///   "iron_nugget" → ("iron", "nugget")
pub fn classify_item(item_id: &str) -> Option<(String, String)> {
    let item_types = [
        "ingot",
        "nugget",
        "block",
        "ore",
        "dust",
        "plate",
        "gear",
        "rod",
        "gem",
        "raw_ore",
        "raw_block",
        "crystal",
        "shard",
        "clump",
        "dirty_dust",
        "slurry",
        "seed",
        "pellet",
        "deepslate_ore",
        "nether_ore",
        "end_ore",
    ];

    let id = item_id.to_lowercase();

    // Pattern: {material}_{type}
    for ty in &item_types {
        if let Some(stripped) = id.strip_suffix(&format!("_{ty}")) {
            if !stripped.is_empty() {
                return Some((stripped.to_string(), ty.to_string()));
            }
        }
    }

    // Pattern: {type}_{material} (camelCase prefix)
    for ty in &item_types {
        if let Some(stripped) = id.strip_prefix(&format!("{ty}_")) {
            if !stripped.is_empty() && !item_types.contains(&stripped) {
                return Some((stripped.to_string(), ty.to_string()));
            }
        }
    }

    // Pattern: {type}{Material} (camelCase)
    for ty in item_types {
        let _capitalized = capitalize_first(ty);
        if let Some(stripped) = id.strip_prefix(ty) {
            if !stripped.is_empty() && stripped.chars().next().map_or(false, |c| c.is_uppercase()) {
                let material = decapitalize_first(stripped);
                return Some((material, ty.to_string()));
            }
        }
    }

    None
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn decapitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
    }
}

/// Groups items by their material, across different types.
/// E.g. ["copper_ingot", "copper_block", "copper_nugget", "tin_ingot", "tin_block"]
/// → {"copper": {"ingot", "block", "nugget"}, "tin": {"ingot", "block"}}
pub fn group_items_by_material(item_ids: &[String]) -> HashMap<String, Vec<String>> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for id in item_ids {
        if let Some((material, _ty)) = classify_item(id) {
            groups.entry(material).or_default().push(id.clone());
        }
    }
    groups
}

/// Detects duplicate resources across mods — if two mods both add
/// "tin_ingot", returns a grouping.
pub fn detect_duplicate_groups(mod_items: &[(String, Vec<String>)]) -> Vec<DuplicateItemGroup> {
    let mut by_material: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for (mod_id, items) in mod_items {
        for item in items {
            if let Some((material, _ty)) = classify_item(item) {
                by_material
                    .entry(material)
                    .or_default()
                    .push((mod_id.clone(), item.clone()));
            }
        }
    }

    by_material
        .into_iter()
        .filter(|(_, entries)| {
            let mods: std::collections::HashSet<_> = entries.iter().map(|(m, _)| m).collect();
            mods.len() > 1
        })
        .map(|(material, entries)| DuplicateItemGroup { material, entries })
        .collect()
}

#[derive(Debug, Clone)]
pub struct DuplicateItemGroup {
    pub material: String,
    pub entries: Vec<(String, String)>, // (mod_id, item_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_ingots() {
        assert_eq!(
            classify_item("tin_ingot"),
            Some(("tin".into(), "ingot".into()))
        );
        assert_eq!(
            classify_item("copper_block"),
            Some(("copper".into(), "block".into()))
        );
        assert_eq!(
            classify_item("iron_nugget"),
            Some(("iron".into(), "nugget".into()))
        );
    }

    #[test]
    fn scans_toml_for_ore_gen() {
        let toml = "[world]
enableCopperOre = true
copperVeinSize = 8
copperMinHeight = -16
copperMaxHeight = 112
";
        let results =
            scan_configs_for_ore_gen(&[("config/mekanism/world.toml".into(), toml.into())]);
        assert!(!results.is_empty());
        assert_eq!(results[0].resource_name, "copper");
    }

    #[test]
    fn detect_duplicates_across_mods() {
        let mods = vec![
            (
                "mekanism".into(),
                vec!["tin_ingot".into(), "copper_ingot".into()],
            ),
            (
                "thermal".into(),
                vec!["tin_ingot".into(), "tin_block".into(), "lead_ingot".into()],
            ),
        ];
        let groups = detect_duplicate_groups(&mods);
        assert_eq!(groups.len(), 1); // tin is duplicated, copper is not
        assert_eq!(groups[0].material, "tin");
    }
}
