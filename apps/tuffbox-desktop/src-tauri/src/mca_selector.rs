//! Launch the original Querz MCA Selector against a project world.
//!
//! The upstream JAR treats any CLI args without `--mode` as a fatal CLI error
//! (`missing mode` → exit). GUI launch therefore uses no `--world` flag; instead
//! we seed `%LOCALAPPDATA%/mcaselector/settings.json` so the world appears under
//! File → Open Recent and Open World starts in the pack's `saves/` folder.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MCA_VERSION: &str = "2.8";
const MCA_JAR_NAME: &str = "mcaselector-2.8.jar";
const MCA_JAR_URL: &str =
    "https://github.com/Querz/mcaselector/releases/download/2.8/mcaselector-2.8.jar";
/// GitHub release asset digest for mcaselector-2.8.jar.
const MCA_JAR_SHA256: &str = "64505f39edf9c9b5d47e666981f81e3c3a889d4f122b3065af7e269f48e53423";
const MIN_JAVA_MAJOR: u32 = 21;

/// Tauri entry: ensure JAR is cached, patch MCA Selector settings, spawn the GUI.
#[tauri::command]
pub fn open_mca_selector(path: String, world_name: String) -> Result<(), String> {
    let project_dir = crate::manifest_parent(&path)?;
    let saves_dir = project_dir.join("saves");
    let world_dir = saves_dir.join(&world_name);

    if !world_dir.is_dir() {
        return Err(format!(
            "world folder not found: {}",
            world_dir.display()
        ));
    }

    let jar = ensure_mca_jar()?;
    let java = resolve_java_for_mca()?;
    ensure_javafx(&java)?;
    seed_mca_settings(&saves_dir, &world_dir)?;

    let mut cmd = Command::new(&java);
    cmd.arg("-Xmx4G")
        .arg("-jar")
        .arg(&jar)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    // Detach so closing TuffBox does not wait on the JavaFX process.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS — keep the GUI visible.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    cmd.spawn()
        .map_err(|e| format!("failed to launch MCA Selector ({java}): {e}"))?;

    Ok(())
}

fn tools_cache_dir() -> Result<PathBuf, String> {
    let root = if let Ok(local) = std::env::var("LOCALAPPDATA") {
        PathBuf::from(local).join("TuffBox").join("tools").join("mca-selector")
    } else if let Some(cache) = dirs::cache_dir() {
        cache.join("TuffBox").join("tools").join("mca-selector")
    } else {
        std::env::temp_dir()
            .join("tuffbox")
            .join("tools")
            .join("mca-selector")
    };
    fs::create_dir_all(&root).map_err(|e| format!("create MCA cache dir: {e}"))?;
    Ok(root)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let digest = Sha256::digest(&bytes);
    Ok(hex::encode(digest))
}

fn jar_ok(path: &Path) -> bool {
    path.is_file()
        && sha256_file(path)
            .map(|h| h.eq_ignore_ascii_case(MCA_JAR_SHA256))
            .unwrap_or(false)
}

fn ensure_mca_jar() -> Result<PathBuf, String> {
    let dir = tools_cache_dir()?;
    let jar = dir.join(MCA_JAR_NAME);
    if jar_ok(&jar) {
        return Ok(jar);
    }

    // Atomic-ish download into a sibling temp file, then rename.
    let partial = dir.join(format!("{MCA_JAR_NAME}.partial"));
    let _ = fs::remove_file(&partial);

    tuffbox_core::mc_install::download_with_sha1(MCA_JAR_URL, &partial, None).map_err(|e| {
        format!(
            "failed to download MCA Selector {MCA_VERSION} from GitHub: {e}"
        )
    })?;

    let got = sha256_file(&partial)?;
    if !got.eq_ignore_ascii_case(MCA_JAR_SHA256) {
        let _ = fs::remove_file(&partial);
        return Err(format!(
            "MCA Selector JAR checksum mismatch (expected {MCA_JAR_SHA256}, got {got})"
        ));
    }

    if let Err(e) = fs::rename(&partial, &jar) {
        fs::copy(&partial, &jar).map_err(|copy_err| {
            format!("install MCA Selector jar: rename {e}; copy {copy_err}")
        })?;
        let _ = fs::remove_file(&partial);
    }

    if !jar_ok(&jar) {
        return Err("MCA Selector JAR failed verification after download".into());
    }

    Ok(jar)
}

fn resolve_java_for_mca() -> Result<String, String> {
    let runtimes = tuffbox_core::jre::find_all_runtimes()
        .map_err(|e| format!("scan Java runtimes: {e}"))?;
    let candidates: Vec<_> = runtimes
        .into_iter()
        .filter(|r| r.major >= MIN_JAVA_MAJOR)
        .collect();

    // Prefer a runtime that already has JavaFX modules.
    for rt in &candidates {
        if java_has_javafx(&rt.path) {
            return Ok(rt.path.clone());
        }
    }

    if let Some(rt) = candidates.first() {
        return Ok(rt.path.clone());
    }

    Err(format!(
        "MCA Selector requires Java {MIN_JAVA_MAJOR}+ with JavaFX. \
         Install Azul Zulu JRE-FX (https://www.azul.com/downloads/?package=jdk-fx) \
         or the official MCA Selector setup \
         (https://github.com/Querz/mcaselector/releases/download/{MCA_VERSION}/mcaselector-{MCA_VERSION}-setup.exe)."
    ))
}

fn ensure_javafx(java_path: &str) -> Result<(), String> {
    if java_has_javafx(java_path) {
        return Ok(());
    }
    Err(format!(
        "Java at '{java_path}' has no JavaFX modules. MCA Selector's GUI needs JavaFX. \
         Install Azul Zulu JRE-FX (https://www.azul.com/downloads/?package=jdk-fx) \
         or the official Windows setup \
         (https://github.com/Querz/mcaselector/releases/download/{MCA_VERSION}/mcaselector-{MCA_VERSION}-setup.exe)."
    ))
}

fn java_has_javafx(java_path: &str) -> bool {
    let mut c = Command::new(java_path);
    c.arg("--list-modules")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let Ok(output) = c.output() else {
        return false;
    };
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("javafx.") || trimmed.contains("@javafx.")
    })
}

fn mca_settings_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var("LOCALAPPDATA").map_err(|_| {
            "LOCALAPPDATA is not set; cannot locate MCA Selector settings".to_string()
        })?;
        let dir = PathBuf::from(local).join("mcaselector");
        fs::create_dir_all(&dir).map_err(|e| format!("create mcaselector settings dir: {e}"))?;
        return Ok(dir.join("settings.json"));
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().ok_or_else(|| "home directory not found".to_string())?;
        let dir = home.join("Library/Application Support/mcaselector");
        fs::create_dir_all(&dir).map_err(|e| format!("create mcaselector settings dir: {e}"))?;
        return Ok(dir.join("settings.json"));
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let dir = if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            PathBuf::from(xdg).join("mcaselector")
        } else {
            let home = dirs::home_dir().ok_or_else(|| "home directory not found".to_string())?;
            home.join(".mcaselector").join("mcaselector")
        };
        fs::create_dir_all(&dir).map_err(|e| format!("create mcaselector settings dir: {e}"))?;
        Ok(dir.join("settings.json"))
    }
}

fn detect_dimension_dirs(world_dir: &Path) -> Vec<PathBuf> {
    let mut dims = Vec::new();
    // Overworld is the world root when `region/` exists (or always as primary).
    if world_dir.join("region").is_dir() || world_dir.join("level.dat").is_file() {
        dims.push(world_dir.to_path_buf());
    }
    for name in ["DIM-1", "DIM1"] {
        let p = world_dir.join(name);
        if p.is_dir() {
            dims.push(p);
        }
    }
    // Modded dimensions folder: dimensions/<ns>/<path>
    let dimensions_root = world_dir.join("dimensions");
    if dimensions_root.is_dir() {
        if let Ok(namespaces) = fs::read_dir(&dimensions_root) {
            for ns in namespaces.flatten() {
                if !ns.path().is_dir() {
                    continue;
                }
                if let Ok(children) = fs::read_dir(ns.path()) {
                    for child in children.flatten() {
                        let p = child.path();
                        if p.is_dir() && (p.join("region").is_dir() || p.join("data").is_dir()) {
                            dims.push(p);
                        }
                    }
                }
            }
        }
    }
    if dims.is_empty() {
        dims.push(world_dir.to_path_buf());
    }
    dims
}

fn seed_mca_settings(saves_dir: &Path, world_dir: &Path) -> Result<(), String> {
    let settings_path = mca_settings_path()?;
    let mut root: serde_json::Value = if settings_path.is_file() {
        let raw = fs::read_to_string(&settings_path)
            .map_err(|e| format!("read MCA settings: {e}"))?;
        serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !root.is_object() {
        root = serde_json::json!({});
    }

    let obj = root
        .as_object_mut()
        .ok_or_else(|| "MCA settings JSON root must be an object".to_string())?;

    let saves_str = saves_dir
        .canonicalize()
        .unwrap_or_else(|_| saves_dir.to_path_buf())
        .to_string_lossy()
        .to_string();
    let world_str = world_dir
        .canonicalize()
        .unwrap_or_else(|_| world_dir.to_path_buf())
        .to_string_lossy()
        .to_string();

    obj.insert("mcSavesDir".into(), serde_json::Value::String(saves_str));

    let dim_dirs: Vec<serde_json::Value> = detect_dimension_dirs(world_dir)
        .into_iter()
        .map(|p| {
            serde_json::Value::String(
                p.canonicalize()
                    .unwrap_or(p)
                    .to_string_lossy()
                    .to_string(),
            )
        })
        .collect();

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
        .to_string();

    let entry = serde_json::json!({
        "recentWorld": world_str,
        "dimensionDirectories": dim_dirs,
    });

    let recent = obj
        .entry("recentWorlds")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(map) = recent.as_object_mut() {
        // Drop any prior entry pointing at the same world (by path).
        map.retain(|_, v| {
            v.get("recentWorld")
                .and_then(|x| x.as_str())
                .map(|p| {
                    !paths_equal(Path::new(p), world_dir)
                })
                .unwrap_or(true)
        });
        // Cap at 16 like MCA Selector.
        while map.len() >= 16 {
            if let Some(oldest) = map.keys().min().cloned() {
                map.remove(&oldest);
            } else {
                break;
            }
        }
        map.insert(now_ms, entry);
    }

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create settings parent: {e}"))?;
    }
    let pretty = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("serialize MCA settings: {e}"))?;
    let tmp = settings_path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("write MCA settings tmp: {e}"))?;
        f.write_all(pretty.as_bytes())
            .map_err(|e| format!("write MCA settings: {e}"))?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, &settings_path).or_else(|_| {
        fs::copy(&tmp, &settings_path)
            .map(|_| {
                let _ = fs::remove_file(&tmp);
            })
            .map_err(|e| format!("commit MCA settings: {e}"))
    })?;

    Ok(())
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    let ca = fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let cb = fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}
