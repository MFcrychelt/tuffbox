//! Launch-time in-game overlay stack: tuffbox-overlay jar + session JSON.
//!
//! Mirrors the cosmetics bridge: copy the nearest anchor jar into mods/ as a
//! runtime artifact, write `.tuffbox/overlay-session.json` (identity +
//! Supabase credentials), clean everything up when the game exits.

use crate::{LoaderKind, McVersion, ProjectManifest};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const BUILTIN_OVERLAY_API_BASE: &str = crate::swarm::BUILTIN_SUPABASE_URL;
pub const BUILTIN_OVERLAY_ANON_KEY: &str = crate::swarm::BUILTIN_SUPABASE_ANON_KEY;

/// Override via env if the built-in anon key is wrong for the linked project.
fn overlay_anon_key() -> String {
    std::env::var("TUFFBOX_OVERLAY_ANON_KEY")
        .or_else(|_| std::env::var("TUFFBOX_COSMETICS_ANON_KEY"))
        .or_else(|_| std::env::var("TUFFSWARM_SUPABASE_ANON_KEY"))
        .unwrap_or_else(|_| BUILTIN_OVERLAY_ANON_KEY.to_string())
}

fn overlay_api_base() -> String {
    std::env::var("TUFFBOX_OVERLAY_API_BASE")
        .or_else(|_| std::env::var("TUFFBOX_COSMETICS_API_BASE"))
        .or_else(|_| std::env::var("TUFFSWARM_SUPABASE_URL"))
        .unwrap_or_else(|_| BUILTIN_OVERLAY_API_BASE.to_string())
}

#[derive(Debug, Clone)]
pub struct OverlayBridgeLaunch {
    pub cleanup_paths: Vec<PathBuf>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OverlaySessionFile {
    username: String,
    uuid: String,
    api_base: String,
    anon_key: String,
    #[serde(default)]
    write_secret: String,
    #[serde(default)]
    pack_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayAnchor {
    pub version: &'static str,
    pub loaders: &'static [&'static str],
}

/// Overlay anchors. v1 ships 1.21.1 only; older/newer packs silently skip.
pub const OVERLAY_ANCHORS: &[OverlayAnchor] = &[OverlayAnchor {
    version: "1.21.1",
    loaders: &["fabric", "neoforge"],
}];

fn overlay_loader_tag(loader: &LoaderKind) -> Option<&'static str> {
    match loader {
        LoaderKind::Fabric | LoaderKind::Quilt => Some("fabric"),
        LoaderKind::Neoforge => Some("neoforge"),
        _ => None,
    }
}

/// Resolve overlay artifact: highest anchor ≤ `mc` that supports `loader`.
pub fn resolve_overlay_artifact(
    mc: &str,
    loader: &LoaderKind,
) -> Option<(&'static str, &'static str)> {
    let want = McVersion::parse(mc)?;
    let tag = overlay_loader_tag(loader)?;
    let mut best: Option<(&'static str, McVersion)> = None;
    for a in OVERLAY_ANCHORS {
        if !a.loaders.contains(&tag) {
            continue;
        }
        let av = McVersion::parse(a.version)?;
        if av > want {
            continue;
        }
        match best {
            None => best = Some((a.version, av)),
            Some((_, bv)) if av > bv => best = Some((a.version, av)),
            _ => {}
        }
    }
    best.map(|(v, _)| (v, tag))
}

fn find_overlay_jar(mc: &str, loader: &LoaderKind) -> Option<(PathBuf, &'static str)> {
    let (anchor, loader_name) = resolve_overlay_artifact(mc, loader)?;
    let needle = format!("tuffbox-overlay-{anchor}-{loader_name}");
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = std::env::var("TUFFBOX_OVERLAY_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../bridges/overlay");
    candidates.push(root.join("prebuilt"));
    for mod_name in ["fabric", "neoforge"] {
        candidates.push(root.join(mod_name).join("build").join("libs"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("resources").join("overlay"));
            candidates.push(parent.join("overlay"));
        }
    }
    for dir in candidates {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(rd) = fs::read_dir(&dir) {
            for ent in rd.flatten() {
                let name = ent.file_name().to_string_lossy().to_string();
                if name.ends_with(".jar")
                    && name.contains(&needle)
                    && !name.contains("-sources")
                    && !name.contains("-dev")
                    && !name.contains("-javadoc")
                {
                    return Some((ent.path(), anchor));
                }
            }
        }
    }
    None
}

/// Pinned WATERMeDIA runtime (universal all-loader jar, MC 1.21.1 line).
/// Update the pair together; env overrides exist for emergency repins.
const WATERMEDIA_URL: &str =
    "https://cdn.modrinth.com/data/G922NeHS/versions/xp27BzFX/watermedia-2.1.1.jar";
const WATERMEDIA_SHA256: &str =
    "23c7e9ae03f28b234cfdb4b8a642a67f7dd1a7c336d87ced19e617dce9f8b58a";
const WATERMEDIA_DEST: &str = "watermedia-2.1.1.jar";

fn watermedia_url() -> String {
    std::env::var("TUFFBOX_OVERLAY_WATERMEDIA_URL").unwrap_or_else(|_| WATERMEDIA_URL.to_string())
}

fn watermedia_sha256() -> String {
    std::env::var("TUFFBOX_OVERLAY_WATERMEDIA_SHA256")
        .unwrap_or_else(|_| WATERMEDIA_SHA256.to_string())
}

fn overlay_cache_root() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("overlay")
}

/// True when the instance already ships any watermedia jar (pack-provided).
fn watermedia_present(mods_dir: &Path) -> bool {
    if let Ok(rd) = fs::read_dir(mods_dir) {
        for ent in rd.flatten() {
            let name = ent.file_name().to_string_lossy().to_lowercase();
            if name.starts_with("watermedia") && name.ends_with(".jar") {
                return true;
            }
        }
    }
    false
}

/// Download (cached, hash-pinned) and copy WATERMeDIA into mods/.
/// Errors degrade to a note — the overlay itself still works without video.
fn ensure_watermedia(
    mods_dir: &Path,
    cleanup: &mut Vec<PathBuf>,
    notes: &mut Vec<String>,
) {
    if watermedia_present(mods_dir) {
        notes.push("WATERMeDIA already present in mods".to_string());
        return;
    }
    let cache_root = overlay_cache_root();
    match crate::download_cache::get_or_download(
        &cache_root,
        &watermedia_url(),
        Some(&watermedia_sha256()),
    ) {
        Ok(src) => {
            let dest = mods_dir.join(WATERMEDIA_DEST);
            match fs::copy(&src, &dest) {
                Ok(_) => {
                    cleanup.push(dest);
                    notes.push("WATERMeDIA 2.1.1 injected".to_string());
                }
                Err(e) => notes.push(format!("WATERMeDIA copy failed: {e}")),
            }
        }
        Err(e) => notes.push(format!("WATERMeDIA download failed: {e}")),
    }
}

/// Prepare the overlay inject for a launch. Returns None when the pack's
/// MC/loader has no overlay anchor — caller should still launch the game.
pub fn prepare_overlay_bridge(
    manifest: &ProjectManifest,
    game_dir: &Path,
    username: &str,
    uuid: &str,
    write_secret: &str,
) -> Result<Option<OverlayBridgeLaunch>, String> {
    let mc = manifest.minecraft.version.as_str();
    let loader = &manifest.loader.kind;
    if resolve_overlay_artifact(mc, loader).is_none() {
        return Ok(None);
    }

    let mods_dir = game_dir.join("mods");
    fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
    let runtime_dir = game_dir.join(".tuffbox");
    fs::create_dir_all(&runtime_dir).map_err(|e| e.to_string())?;

    let mut cleanup = Vec::new();
    let mut notes = Vec::new();

    let mut overlay_injected = false;
    match find_overlay_jar(mc, loader) {
        Some((src, anchor)) => {
            let dest = mods_dir.join("tuffbox-overlay.runtime.jar");
            fs::copy(&src, &dest).map_err(|e| format!("copy overlay jar: {e}"))?;
            cleanup.push(dest);
            overlay_injected = true;
            if anchor == mc {
                notes.push(format!("tuffbox-overlay injected ({anchor})"));
            } else {
                notes.push(format!("tuffbox-overlay injected (anchor {anchor} for {mc})"));
            }
        }
        None => {
            if let Some((anchor, tag)) = resolve_overlay_artifact(mc, loader) {
                notes.push(format!(
                    "tuffbox-overlay jar missing for anchor {anchor}/{tag} — build bridges/overlay"
                ));
            }
        }
    }

    // Video engine for the YouTube app (best-effort; overlay works without it).
    if overlay_injected {
        ensure_watermedia(&mods_dir, &mut cleanup, &mut notes);
    }

    let session_path = runtime_dir.join("overlay-session.json");
    let session = OverlaySessionFile {
        username: username.to_string(),
        uuid: uuid.to_string(),
        api_base: overlay_api_base(),
        anon_key: overlay_anon_key(),
        write_secret: write_secret.to_string(),
        pack_name: manifest.project.name.clone(),
    };
    let body = serde_json::to_vec_pretty(&session).map_err(|e| e.to_string())?;
    fs::write(&session_path, body).map_err(|e| e.to_string())?;
    cleanup.push(session_path);

    Ok(Some(OverlayBridgeLaunch {
        cleanup_paths: cleanup,
        message: notes.join("; "),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_anchor_fallbacks() {
        assert_eq!(
            resolve_overlay_artifact("1.21.1", &LoaderKind::Fabric),
            Some(("1.21.1", "fabric"))
        );
        assert_eq!(
            resolve_overlay_artifact("1.21.4", &LoaderKind::Neoforge),
            Some(("1.21.1", "neoforge"))
        );
        // Quilt maps to the fabric jar
        assert_eq!(
            resolve_overlay_artifact("1.21.4", &LoaderKind::Quilt),
            Some(("1.21.1", "fabric"))
        );
        // Below the only anchor → skip
        assert_eq!(resolve_overlay_artifact("1.21", &LoaderKind::Fabric), None);
        // No forge anchor in v1
        assert_eq!(resolve_overlay_artifact("1.21.1", &LoaderKind::Forge), None);
        // Below the only anchor → skip
        assert_eq!(resolve_overlay_artifact("1.20.1", &LoaderKind::Fabric), None);
    }
}
