//! Pack optimization helpers: curated Fabric maps, ModernFix safety, config templates.
//!
//! Network resolve (Modrinth / CurseForge) lives in the desktop crate; this module
//! stays deterministic and AI-free.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::action_plan::{ActionPlan, LauncherAction, ACTION_PLAN_SCHEMA_VERSION};
use crate::manifest::ProjectManifest;

/// Bundled map: loader → MC version → Modrinth project.
const CURATED_PACKS_JSON: &str = include_str!("../data/optimize-packs.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CuratedPackRef {
    pub project_id: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CuratedPacksFile {
    #[serde(default)]
    pub fabric: HashMap<String, CuratedPackRef>,
    #[serde(default)]
    pub quilt: HashMap<String, CuratedPackRef>,
}

pub fn load_curated_packs() -> CuratedPacksFile {
    serde_json::from_str(CURATED_PACKS_JSON).unwrap_or(CuratedPacksFile {
        fabric: HashMap::new(),
        quilt: HashMap::new(),
    })
}

/// Optional env override: path to JSON with the same shape as `optimize-packs.json`.
pub fn load_curated_packs_with_override() -> CuratedPacksFile {
    if let Ok(path) = std::env::var("TUFFBOX_OPT_PACKS") {
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str(&raw) {
                return parsed;
            }
        }
    }
    load_curated_packs()
}

pub fn curated_pack_for(loader: &str, mc_version: &str) -> Option<CuratedPackRef> {
    let file = load_curated_packs_with_override();
    let map = match loader {
        "fabric" => &file.fabric,
        "quilt" => {
            if let Some(p) = file.quilt.get(mc_version) {
                return Some(p.clone());
            }
            &file.fabric
        }
        _ => return None,
    };
    map.get(mc_version).cloned()
}

pub fn list_curated_pack_entries(loader: &str) -> Vec<(String, CuratedPackRef)> {
    let file = load_curated_packs_with_override();
    let map = match loader {
        "fabric" => file.fabric,
        "quilt" => {
            let mut m = file.fabric;
            for (k, v) in file.quilt {
                m.insert(k, v);
            }
            m
        }
        _ => HashMap::new(),
    };
    let mut out: Vec<_> = map.into_iter().collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Mods that make aggressive ModernFix options unsafe.
pub fn modernfix_denylist_hit(mod_ids_and_names: &[String]) -> Vec<String> {
    const DENY: &[&str] = &[
        "optifine",
        "optifabric",
        "connector",
        "sinytra-connector",
        "forgified-fabric-api",
        "rubidium", // prefer embeddium path; mixed sodium forks + modernfix can be fragile
    ];
    let mut hits = Vec::new();
    for raw in mod_ids_and_names {
        let c: String = raw
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .flat_map(|ch| ch.to_lowercase())
            .collect();
        for d in DENY {
            let dc: String = d
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .flat_map(|ch| ch.to_lowercase())
                .collect();
            if c == dc || c.contains(&dc) || dc.contains(&c) && c.len() >= 6 {
                hits.push(raw.clone());
                break;
            }
        }
    }
    hits.sort();
    hits.dedup();
    hits
}

pub fn inventory_tokens(manifest: &ProjectManifest) -> Vec<String> {
    let mut out = Vec::new();
    for m in &manifest.mods {
        out.push(m.id.clone());
        out.push(m.name.clone());
        if let Some(pid) = &m.source.project_id {
            out.push(pid.clone());
        }
        if let Some(f) = &m.file_name {
            out.push(f.clone());
        }
    }
    out
}

fn edit_action(path: &str, patch_type: &str, patch: Value, reason: &str, risk: &str) -> LauncherAction {
    LauncherAction {
        op: "edit_config".into(),
        mod_id: None,
        provider: None,
        project_id: None,
        version: None,
        path: Some(path.into()),
        patch_type: Some(patch_type.into()),
        patch: Some(patch),
        reason: Some(reason.into()),
        risk: risk.into(),
    }
}

/// Deterministic safe config patches. Prefer patching existing files.
pub fn build_optimize_config_actions(
    project_dir: &Path,
    manifest: &ProjectManifest,
    allow_modernfix_aggressive: bool,
) -> (Vec<LauncherAction>, Vec<String>) {
    let mut actions = Vec::new();
    let mut warnings = Vec::new();
    let tokens = inventory_tokens(manifest);
    let deny_hits = modernfix_denylist_hit(&tokens);
    if !deny_hits.is_empty() {
        warnings.push(format!(
            "ModernFix aggressive options skipped — possible conflicts: {}",
            deny_hits.join(", ")
        ));
    }
    let modernfix_safe_only = !allow_modernfix_aggressive || !deny_hits.is_empty();

    // options.txt
    let options_path = project_dir.join("options.txt");
    if options_path.is_file() {
        if let Ok(raw) = std::fs::read_to_string(&options_path) {
            let mut patch = serde_json::Map::new();
            if let Some(rd) = prop_get(&raw, "renderDistance") {
                if let Ok(n) = rd.parse::<i32>() {
                    if n > 16 {
                        patch.insert("renderDistance".into(), json!("12"));
                    }
                }
            }
            if let Some(sd) = prop_get(&raw, "simulationDistance") {
                if let Ok(n) = sd.parse::<i32>() {
                    if n > 12 {
                        patch.insert("simulationDistance".into(), json!("8"));
                    }
                }
            }
            if prop_get(&raw, "maxFps").is_none() {
                patch.insert("maxFps".into(), json!("120"));
            }
            if !patch.is_empty() {
                actions.push(edit_action(
                    "options.txt",
                    "properties_set",
                    Value::Object(patch),
                    "Cap render/simulation distance and set a sane maxFps for modded clients",
                    "low",
                ));
            }
        }
    }

    // Sodium
    for name in ["sodium-options.json", "sodium-extra-options.json"] {
        let rel = format!("config/{name}");
        let fp = project_dir.join("config").join(name);
        if !fp.is_file() {
            continue;
        }
        if name.starts_with("sodium-options") {
            actions.push(edit_action(
                &rel,
                "json_merge",
                json!({
                    "performance": {
                        "enable_memory_tracing": false,
                        "always_defer_chunk_updates_v0_shouldbedeleted": false
                    }
                }),
                "Disable Sodium memory tracing / keep chunk updates efficient",
                "low",
            ));
        } else {
            actions.push(edit_action(
                &rel,
                "json_merge",
                json!({
                    "extra_settings": {
                        "reduce_fps_when_afk": true
                    }
                }),
                "Enable mild Sodium Extra AFK FPS reduction when available",
                "low",
            ));
        }
    }

    // C2ME — only touch if config exists
    for name in ["c2me.toml", "c2me.json", "c2me-opts.toml"] {
        let fp = project_dir.join("config").join(name);
        if !fp.is_file() {
            continue;
        }
        let rel = format!("config/{name}");
        if name.ends_with(".toml") {
            actions.push(edit_action(
                &rel,
                "toml_set",
                json!({
                    "threadedWorldGen.enabled": true
                }),
                "Enable moderate C2ME threaded worldgen when present",
                "medium",
            ));
        } else {
            actions.push(edit_action(
                &rel,
                "json_merge",
                json!({ "threadedWorldGen": { "enabled": true } }),
                "Enable moderate C2ME threaded worldgen when present",
                "medium",
            ));
        }
        break;
    }

    // Bobby
    for name in ["bobby.toml", "bobby.json"] {
        let fp = project_dir.join("config").join(name);
        if !fp.is_file() {
            continue;
        }
        let rel = format!("config/{name}");
        if name.ends_with(".toml") {
            actions.push(edit_action(
                &rel,
                "toml_set",
                json!({
                    "maxRenderDistance": 12
                }),
                "Keep Bobby cache render distance moderate",
                "low",
            ));
        } else {
            actions.push(edit_action(
                &rel,
                "json_merge",
                json!({ "maxRenderDistance": 12 }),
                "Keep Bobby cache render distance moderate",
                "low",
            ));
        }
        break;
    }

    // ModernFix — safe subset only when denylist hits
    let mf_names = [
        "modernfix-common.toml",
        "modernfix.toml",
        "modernfix-fabric.toml",
        "modernfix-neoforge.toml",
    ];
    for name in mf_names {
        let fp = project_dir.join("config").join(name);
        if !fp.is_file() {
            continue;
        }
        let rel = format!("config/{name}");
        if modernfix_safe_only {
            actions.push(edit_action(
                &rel,
                "toml_set",
                json!({
                    "mixin.perf.dynamic_resources": false,
                    "mixin.perf.faster_item_rendering": true
                }),
                "ModernFix safe subset only (aggressive options disabled due to pack conflicts)",
                "low",
            ));
        } else {
            actions.push(edit_action(
                &rel,
                "toml_set",
                json!({
                    "mixin.perf.dynamic_resources": true,
                    "mixin.perf.faster_item_rendering": true,
                    "mixin.perf.cache_strongholds": true
                }),
                "Enable common ModernFix performance mixins",
                "medium",
            ));
        }
        break;
    }

    (actions, warnings)
}

fn prop_get(raw: &str, key: &str) -> Option<String> {
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.trim() == key {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

pub fn config_actions_to_plan(actions: Vec<LauncherAction>, explanation: &str) -> ActionPlan {
    ActionPlan {
        schema_version: ACTION_PLAN_SCHEMA_VERSION,
        human_explanation: explanation.into(),
        confidence: 0.85,
        suspected_mods: Vec::new(),
        needs_user_review: true,
        source: Some("optimize_pack".into()),
        matched_case_ids: Vec::new(),
        actions,
        additional_context: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denylist_detects_optifine() {
        let hits = modernfix_denylist_hit(&["OptiFine 1.20.1".into()]);
        assert!(!hits.is_empty());
    }

    #[test]
    fn curated_json_parses() {
        let _ = load_curated_packs();
    }
}
