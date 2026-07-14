//! Resolves Minecraft item icons from mod jars and the installed client jar.

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub fn item_icon_cache_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".tuffbox").join("cache").join("item-icons")
}

fn cache_file_for(project_dir: &Path, item_id: &str) -> PathBuf {
    let safe = item_id
        .replace(':', "__")
        .replace('#', "_tag_")
        .replace('/', "_");
    item_icon_cache_dir(project_dir).join(format!("{safe}.png"))
}

fn parse_item_id(item_id: &str) -> Option<(String, String)> {
    let trimmed = item_id.trim();
    let id = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if id.is_empty() || id.contains(' ') {
        return None;
    }
    let (namespace, path) = id.split_once(':')?;
    if namespace.is_empty() || path.is_empty() {
        return None;
    }
    Some((namespace.to_string(), path.to_string()))
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10])
}

fn read_zip_entry(archive: &mut ZipArchive<File>, path: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(path).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    if is_png(&buf) {
        Some(buf)
    } else {
        None
    }
}

fn texture_zip_path(namespace: &str, texture_ref: &str) -> String {
    let (ns, path) = if let Some((a, b)) = texture_ref.split_once(':') {
        (a, b)
    } else {
        (namespace, texture_ref)
    };
    let full = if path.starts_with("item/") || path.starts_with("block/") {
        path.to_string()
    } else {
        format!("item/{path}")
    };
    format!("assets/{ns}/textures/{full}.png")
}

fn resolve_from_model(
    archive: &mut ZipArchive<File>,
    namespace: &str,
    item_path: &str,
    depth: u8,
) -> Option<Vec<u8>> {
    if depth > 5 {
        return None;
    }
    let model_path = format!("assets/{namespace}/models/item/{item_path}.json");
    let content = {
        let mut file = archive.by_name(&model_path).ok()?;
        let mut content = String::new();
        file.read_to_string(&mut content).ok()?;
        content
    };
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    if let Some(textures) = json.get("textures").and_then(|value| value.as_object()) {
        for key in ["layer0", "layer1", "particle", "texture"] {
            let Some(texture_ref) = textures.get(key).and_then(|value| value.as_str()) else {
                continue;
            };
            let zip_path = texture_zip_path(namespace, texture_ref);
            if let Some(png) = read_zip_entry(archive, &zip_path) {
                return Some(png);
            }
        }
    }

    let Some(parent) = json.get("parent").and_then(|value| value.as_str()) else {
        return None;
    };

    if let Some((parent_ns, parent_path)) = parse_item_id(parent) {
        return resolve_from_model(archive, &parent_ns, &parent_path, depth + 1);
    }

    if let Some(rest) = parent.strip_prefix("minecraft:block/") {
        return read_zip_entry(
            archive,
            &format!("assets/minecraft/textures/block/{rest}.png"),
        );
    }
    if let Some(rest) = parent.strip_prefix("block/") {
        return read_zip_entry(
            archive,
            &format!("assets/{namespace}/textures/block/{rest}.png"),
        );
    }
    if parent.contains('/') {
        let zip_path = texture_zip_path(namespace, parent);
        if let Some(png) = read_zip_entry(archive, &zip_path) {
            return Some(png);
        }
    }

    None
}

fn find_in_jar(archive: &mut ZipArchive<File>, namespace: &str, item_path: &str) -> Option<Vec<u8>> {
    for candidate in [
        format!("assets/{namespace}/textures/item/{item_path}.png"),
        format!("assets/{namespace}/textures/block/{item_path}.png"),
    ] {
        if let Some(png) = read_zip_entry(archive, &candidate) {
            return Some(png);
        }
    }
    resolve_from_model(archive, namespace, item_path, 0)
}

fn jar_sources(project_dir: &Path, extra_jars: &[PathBuf]) -> Vec<PathBuf> {
    let mut sources = extra_jars.to_vec();

    let mods_dir = project_dir.join("mods");
    if mods_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "jar") {
                    sources.push(path);
                }
            }
        }
    }
    sources
}

/// Locates a PNG texture for `item_id`, caches it under `.tuffbox/cache/item-icons/`,
/// and returns the cached file path when found.
pub fn resolve_item_icon_path(
    project_dir: &Path,
    item_id: &str,
    extra_jars: &[PathBuf],
) -> Result<Option<PathBuf>, String> {
    let Some((namespace, item_path)) = parse_item_id(item_id) else {
        return Ok(None);
    };

    let cache_dir = item_icon_cache_dir(project_dir);
    std::fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;
    let cache_file = cache_file_for(project_dir, item_id);
    if cache_file.is_file() {
        return Ok(Some(cache_file));
    }

    for jar_path in jar_sources(project_dir, extra_jars) {
        let file = match File::open(&jar_path) {
            Ok(file) => file,
            Err(_) => continue,
        };
        let mut archive = match ZipArchive::new(file) {
            Ok(archive) => archive,
            Err(_) => continue,
        };
        if let Some(png) = find_in_jar(&mut archive, &namespace, &item_path) {
            std::fs::write(&cache_file, png).map_err(|error| error.to_string())?;
            return Ok(Some(cache_file));
        }
    }

    Ok(None)
}

fn open_jar_archives(project_dir: &Path, extra_jars: &[PathBuf]) -> Vec<ZipArchive<File>> {
    jar_sources(project_dir, extra_jars)
        .into_iter()
        .filter_map(|jar_path| {
            let file = File::open(&jar_path).ok()?;
            ZipArchive::new(file).ok()
        })
        .collect()
}

/// Resolves many item icons in one pass, opening each mod jar only once.
pub fn resolve_item_icons_batch(
    project_dir: &Path,
    item_ids: &[String],
    extra_jars: &[PathBuf],
) -> Result<HashMap<String, Option<PathBuf>>, String> {
    let cache_dir = item_icon_cache_dir(project_dir);
    std::fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;

    let mut out = HashMap::with_capacity(item_ids.len());
    let mut pending: Vec<(String, String, String)> = Vec::new();

    for item_id in item_ids {
        let Some((namespace, item_path)) = parse_item_id(item_id) else {
            out.insert(item_id.clone(), None);
            continue;
        };
        let cache_file = cache_file_for(project_dir, item_id);
        if cache_file.is_file() {
            out.insert(item_id.clone(), Some(cache_file));
            continue;
        }
        pending.push((item_id.clone(), namespace, item_path));
    }

    if pending.is_empty() {
        return Ok(out);
    }

    let mut archives = open_jar_archives(project_dir, extra_jars);
    for (item_id, namespace, item_path) in pending {
        let mut found = None;
        for archive in &mut archives {
            if let Some(png) = find_in_jar(archive, &namespace, &item_path) {
                let cache_file = cache_file_for(project_dir, &item_id);
                if std::fs::write(&cache_file, png).is_ok() {
                    found = Some(cache_file);
                }
                break;
            }
        }
        out.insert(item_id, found);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_item_ids() {
        assert_eq!(
            parse_item_id("minecraft:diamond"),
            Some(("minecraft".into(), "diamond".into()))
        );
        assert_eq!(parse_item_id("#forge:ingots/copper"), Some(("forge".into(), "ingots/copper".into())));
        assert_eq!(parse_item_id("bad"), None);
    }

    #[test]
    fn maps_texture_refs_to_zip_paths() {
        assert_eq!(
            texture_zip_path("thermal", "item/signalum_gear"),
            "assets/thermal/textures/item/signalum_gear.png"
        );
        assert_eq!(
            texture_zip_path("minecraft", "minecraft:item/diamond"),
            "assets/minecraft/textures/item/diamond.png"
        );
    }
}
