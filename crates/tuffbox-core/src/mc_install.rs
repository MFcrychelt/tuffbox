use rayon::prelude::*;
use serde::Deserialize;
use sha1::Digest;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};
use thiserror::Error;

const MOJANG_VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("missing download: {0}")]
    MissingDownload(String),
    #[error("unsupported loader: {0}")]
    UnsupportedLoader(String),
}

#[derive(Debug, Clone)]
pub struct InstalledVersion {
    pub id: String,
    pub main_class: String,
    pub client_jar: PathBuf,
    pub libraries: Vec<PathBuf>,
    pub natives_dir: PathBuf,
    pub asset_index_id: String,
    pub asset_dir: PathBuf,
    pub log_config: Option<PathBuf>,
    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,
}

pub struct InstallProgress {
    pub log_path: PathBuf,
}

impl InstallProgress {
    pub fn log(&self, msg: &str) {
        if let Ok(mut f) = fs::OpenOptions::new().append(true).create(true).open(&self.log_path) {
            let _ = writeln!(f, "{msg}");
        }
    }
}

pub fn install_game(
    mc_version: &str,
    loader_kind: &str,
    loader_version: &str,
    launcher_dir: &Path,
    java_path: &Path,
    progress: &InstallProgress,
) -> Result<InstalledVersion, InstallError> {
    let versions_dir = launcher_dir.join("versions");
    let libraries_dir = launcher_dir.join("libraries");
    let assets_dir = launcher_dir.join("assets");
    let natives_base = launcher_dir.join("natives");

    fs::create_dir_all(&versions_dir)?;
    fs::create_dir_all(&libraries_dir)?;
    fs::create_dir_all(&assets_dir)?;

    progress.log(&format!("# Checking Minecraft {mc_version}..."));
    let mut vanilla = install_vanilla(mc_version, &versions_dir, &libraries_dir, &assets_dir, &natives_base, progress)?;

    if loader_kind == "fabric" || loader_kind == "quilt" {
        progress.log(&format!("# Fetching {loader_kind} loader {loader_version}..."));
        let fabric_profile = fetch_fabric_profile(mc_version, loader_version, loader_kind, launcher_dir, progress)?;
        merge_fabric_profile(&mut vanilla, fabric_profile, &libraries_dir, mc_version, progress)?;
    } else if loader_kind == "forge" {
        progress.log(&format!("# Fetching Forge {loader_version}..."));
        let forge_profile = crate::forge::fetch_forge_profile(mc_version, loader_version)?;
        crate::forge::download_forge_libraries(&forge_profile, &libraries_dir, progress)?;
        crate::forge::run_forge_processors(
            &forge_profile,
            &libraries_dir,
            launcher_dir,
            &vanilla.client_jar,
            mc_version,
            java_path,
            progress,
            None,
        )?;
        merge_forge_profile(&mut vanilla, forge_profile, &libraries_dir)?;
    } else if loader_kind == "neoforge" {
        progress.log(&format!("# Fetching NeoForge {loader_version}..."));
        let (neoforge_profile, installer_path) = crate::forge::fetch_neoforge_profile(loader_version, progress)?;
        crate::forge::download_forge_libraries(&neoforge_profile, &libraries_dir, progress)?;
        crate::forge::run_forge_processors(
            &neoforge_profile,
            &libraries_dir,
            launcher_dir,
            &vanilla.client_jar,
            mc_version,
            java_path,
            progress,
            Some(&installer_path),
        )?;
        merge_forge_profile(&mut vanilla, neoforge_profile, &libraries_dir)?;
    } else if loader_kind != "vanilla" {
        return Err(InstallError::UnsupportedLoader(loader_kind.to_string()));
    }

    progress.log("# Installation complete.");
    Ok(vanilla)
}

fn install_vanilla(
    mc_version: &str,
    versions_dir: &Path,
    libraries_dir: &Path,
    assets_dir: &Path,
    natives_base: &Path,
    progress: &InstallProgress,
) -> Result<InstalledVersion, InstallError> {
    let version_dir = versions_dir.join(mc_version);
    let version_json_path = version_dir.join(format!("{mc_version}.json"));

    let version_json: VersionJson = if version_json_path.exists() {
        progress.log("# Loading cached version JSON...");
        serde_json::from_str(&fs::read_to_string(&version_json_path)?)?
    } else {
        progress.log("# Fetching version manifest...");
        let manifest: VersionManifest = crate::http::get_json(MOJANG_VERSION_MANIFEST)?;
        let version_url = manifest
            .versions
            .into_iter()
            .find(|v| v.id == mc_version)
            .map(|v| v.url)
            .ok_or_else(|| InstallError::MissingDownload(format!("Minecraft {mc_version} not found in manifest")))?;
        progress.log("# Fetching version JSON...");
        let raw = crate::http::get_text(&version_url)?;
        fs::create_dir_all(&version_dir)?;
        fs::write(&version_json_path, &raw)?;
        serde_json::from_str(&raw)?
    };

    let client_jar = version_dir.join(format!("{}.jar", version_json.id));
    if !client_jar.exists() {
        progress.log("# Downloading client jar...");
        download_with_sha1(&version_json.downloads.client.url, &client_jar, Some(&version_json.downloads.client.sha1))?;
    }

    let natives_dir = natives_base.join(&version_json.id);
    fs::create_dir_all(&natives_dir)?;

    let mut libraries: Vec<PathBuf> = Vec::new();
    let mut lib_tasks: Vec<(String, PathBuf, Option<String>)> = Vec::new();
    let mut native_tasks: Vec<(String, PathBuf, Option<String>)> = Vec::new();
    for lib in &version_json.libraries {
        if !library_allowed(lib) {
            continue;
        }
        let artifact = &lib.downloads.artifact;
        let path = maven_path(libraries_dir, &artifact.path);
        if !path.exists() {
            lib_tasks.push((artifact.url.clone(), path.clone(), artifact.sha1.clone()));
        }
        libraries.push(path);

        if let Some(classifiers) = lib.downloads.classifiers.as_ref() {
            if let Some(native) = classifiers.get(native_classifier()) {
                let native_path = maven_path(libraries_dir, &native.path);
                if !native_path.exists() {
                    native_tasks.push((native.url.clone(), native_path.clone(), native.sha1.clone()));
                }
            }
        }
    }

    if lib_tasks.is_empty() && native_tasks.is_empty() {
        progress.log("# All libraries already present.");
    } else {
        let total = lib_tasks.len() + native_tasks.len();
        progress.log(&format!("# Downloading {total} libraries in parallel..."));
        let counter = AtomicUsize::new(0);
        let errs: Vec<String> = lib_tasks
            .into_par_iter()
            .chain(native_tasks.into_par_iter())
            .filter_map(|(url, path, sha1)| {
                if let Err(e) = fs::create_dir_all(path.parent().unwrap()) {
                    return Some(format!("{url}: {e}"));
                }
                if let Err(e) = download_with_sha1(&url, &path, sha1.as_deref()) {
                    return Some(format!("{url}: {e}"));
                }
                let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
                progress.log(&format!("# Downloaded library {n}/{total}."));
                None
            })
            .collect();
        if !errs.is_empty() {
            return Err(InstallError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                errs.join("\n"),
            )));
        }
        progress.log("# Library download complete.");
    }

    for lib in &version_json.libraries {
        if !library_allowed(lib) {
            continue;
        }
        if let Some(classifiers) = lib.downloads.classifiers.as_ref() {
            if let Some(native) = classifiers.get(native_classifier()) {
                let native_path = maven_path(libraries_dir, &native.path);
                extract_natives(&native_path, &natives_dir)?;
            }
        }
    }

    libraries.push(client_jar.clone());

    let asset_index_path = assets_dir
        .join("indexes")
        .join(format!("{}.json", version_json.asset_index.id));
    if !asset_index_path.exists() {
        progress.log("# Downloading asset index...");
        fs::create_dir_all(asset_index_path.parent().unwrap())?;
        download_with_sha1(&version_json.asset_index.url, &asset_index_path, Some(&version_json.asset_index.sha1))?;
    }
    let asset_index: AssetIndex = serde_json::from_str(&fs::read_to_string(&asset_index_path)?)?;
    install_assets_index(&asset_index, assets_dir, progress)?;

    let log_config = version_json
        .logging
        .as_ref()
        .and_then(|l| l.client.as_ref())
        .map(|c| {
            let path = version_dir.join(&c.file.id);
            if !path.exists() {
                let _ = download_with_sha1(&c.file.url, &path, Some(&c.file.sha1));
            }
            path
        });

    let (jvm_args, game_args) = parse_arguments(&version_json.arguments, &version_json.legacy_arguments);

    Ok(InstalledVersion {
        id: version_json.id.clone(),
        main_class: version_json.main_class.clone(),
        client_jar,
        libraries,
        natives_dir,
        asset_index_id: version_json.asset_index.id.clone(),
        asset_dir: assets_dir.to_path_buf(),
        log_config,
        jvm_args,
        game_args,
    })
}

fn fetch_fabric_profile(
    mc_version: &str,
    loader_version: &str,
    loader_kind: &str,
    launcher_dir: &Path,
    progress: &InstallProgress,
) -> Result<FabricProfileJson, InstallError> {
    let base = if loader_kind == "quilt" {
        "https://meta.quiltmc.org/v3/versions/loader"
    } else {
        "https://meta.fabricmc.net/v2/versions/loader"
    };
    let url = format!("{base}/{mc_version}/{loader_version}/profile/json");

    let cache_dir = launcher_dir.join("fabric-cache");
    let cache_path = cache_dir.join(format!("{loader_kind}-{mc_version}-{loader_version}.json"));

    progress.log(&format!("# {loader_kind} profile URL: {url}"));

    match crate::http::get_text(&url) {
        Ok(text) => {
            let _ = fs::create_dir_all(&cache_dir);
            let _ = fs::write(&cache_path, &text);
            progress.log("# Profile downloaded, parsing...");
            Ok(serde_json::from_str(&text)?)
        }
        Err(e) => {
            if cache_path.exists() {
                progress.log("# Profile fetch failed, using cached version...");
                let text = fs::read_to_string(&cache_path)?;
                Ok(serde_json::from_str(&text)?)
            } else {
                Err(InstallError::Network(e))
            }
        }
    }
}

fn merge_forge_profile(
    vanilla: &mut InstalledVersion,
    forge: crate::forge::ForgeProfile,
    libraries_dir: &Path,
) -> Result<(), InstallError> {
    if !forge.main_class.is_empty() {
        vanilla.main_class = forge.main_class;
    }
    for lib in &forge.libraries {
        if !lib.include_in_classpath {
            continue;
        }
        if let Some(artifact) = crate::forge::resolve_library(lib) {
            vanilla.libraries.push(libraries_dir.join(&artifact.path));
        }
    }
    if let Some(args) = forge.arguments {
        let (jvm, game) = crate::forge::flatten_arguments(&args);
        if !jvm.is_empty() {
            vanilla.jvm_args = jvm;
        }
        if !game.is_empty() {
            vanilla.game_args = game;
        }
    }
    Ok(())
}

fn merge_fabric_profile(
    vanilla: &mut InstalledVersion,
    fabric: FabricProfileJson,
    libraries_dir: &Path,
    mc_version: &str,
    progress: &InstallProgress,
) -> Result<(), InstallError> {
    progress.log(&format!("# Fabric main class: {}", fabric.main_class));
    if !fabric.main_class.is_empty() {
        vanilla.main_class = fabric.main_class;
    }

    let mut lib_tasks: Vec<DownloadTask> = Vec::new();
    for lib in &fabric.libraries {
        // Replace ${modrinth.gameVersion} placeholder with actual MC version.
        let name = lib.name.replace("${modrinth.gameVersion}", mc_version);
        let Some((group_path, artifact, version, classifier)) = parse_maven_name(&name) else {
            progress.log(&format!("# Skipping unsupported loader library coordinate: {name}"));
            continue;
        };
        let jar = match classifier {
            Some(classifier) => format!("{artifact}-{version}-{classifier}.jar"),
            None => format!("{artifact}-{version}.jar"),
        };
        let path = libraries_dir
            .join(&group_path)
            .join(&artifact)
            .join(&version)
            .join(&jar);
        if !path.exists() || lib.sha1.is_some() {
            let urls = fabric_library_urls(&lib.url, &group_path, &artifact, &version, &jar);
            lib_tasks.push(DownloadTask {
                urls,
                path: path.clone(),
                sha1: lib.sha1.clone(),
            });
        }
        vanilla.libraries.push(path);
    }

    if lib_tasks.is_empty() {
        progress.log("# All loader libraries already present.");
    } else {
        download_tasks_with_sequential_retry("loader library", lib_tasks, progress, 5)?;
        progress.log("# Loader library download complete.");
    }

    let (jvm_args, game_args) = parse_arguments(&fabric.arguments, &None);
    if !jvm_args.is_empty() {
        vanilla.jvm_args = jvm_args;
    }
    if !game_args.is_empty() {
        vanilla.game_args = game_args;
    }

    Ok(())
}

fn install_assets_index(index: &AssetIndex, assets_dir: &Path, progress: &InstallProgress) -> Result<(), InstallError> {
    let objects_dir = assets_dir.join("objects");
    let mut to_download: Vec<(String, String)> = Vec::new();
    for (_name, obj) in &index.objects {
        let prefix = &obj.hash[..2];
        let object_path = objects_dir.join(prefix).join(&obj.hash);
        if !object_path.exists() {
            to_download.push((prefix.to_string(), obj.hash.clone()));
        }
    }
    if to_download.is_empty() {
        progress.log("# All assets already present.");
        return Ok(());
    }
    let total = to_download.len();
    progress.log(&format!("# Downloading {total} assets in parallel..."));
    let counter = AtomicUsize::new(0);
    let errs: Vec<String> = to_download
        .into_par_iter()
            .filter_map(|(prefix, hash)| {
            let object_dir = objects_dir.join(&prefix);
            let object_path = object_dir.join(&hash);
            let url = format!("{}/{}/{}", RESOURCES_URL, prefix, hash);
            if let Err(e) = fs::create_dir_all(&object_dir) {
                return Some(format!("{url}: {e}"));
            }
            if let Err(e) = download_with_sha1(&url, &object_path, Some(&hash)) {
                return Some(format!("{url}: {e}"));
            }
            let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 50 == 0 || n == total {
                progress.log(&format!("# Assets: {n}/{total}..."));
            }
            None
        })
        .collect();
    if !errs.is_empty() {
        return Err(InstallError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            errs.join("\n"),
        )));
    }
    progress.log("# Asset download complete.");
    Ok(())
}

#[derive(Debug, Clone)]
struct DownloadTask {
    urls: Vec<String>,
    path: PathBuf,
    sha1: Option<String>,
}

fn download_tasks_with_sequential_retry(
    label: &str,
    tasks: Vec<DownloadTask>,
    progress: &InstallProgress,
    log_every: usize,
) -> Result<(), InstallError> {
    let total = tasks.len();
    progress.log(&format!("# Downloading {total} {label}s in parallel..."));
    let counter = AtomicUsize::new(0);

    let failed: Vec<(DownloadTask, String)> = tasks
        .into_par_iter()
        .filter_map(|task| {
            if let Some(parent) = task.path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return Some((task, e.to_string()));
                }
            }
            match download_task(&task) {
                Ok(()) => {
                    let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % log_every == 0 || n == total {
                        progress.log(&format!("# {label}s: {n}/{total}..."));
                    }
                    None
                }
                Err(e) => Some((task, e.to_string())),
            }
        })
        .collect();

    if failed.is_empty() {
        return Ok(());
    }

    progress.log(&format!(
        "# {} {label}(s) failed in parallel; retrying sequentially with fallback URLs...",
        failed.len()
    ));

    let mut errors = Vec::new();
    for (task, first_error) in failed {
        match download_task(&task) {
            Ok(()) => {
                let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
                progress.log(&format!("# {label}s: {n}/{total}..."));
            }
            Err(e) => {
                let urls = task.urls.join(", ");
                errors.push(format!("{urls}: first error: {first_error}; retry error: {e}"));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(InstallError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            errors.join("\n"),
        )))
    }
}

fn download_task(task: &DownloadTask) -> Result<(), InstallError> {
    let mut last_error = None;
    for url in &task.urls {
        match download_with_sha1(url, &task.path, task.sha1.as_deref()) {
            Ok(()) => return Ok(()),
            Err(e) => last_error = Some(e),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        InstallError::MissingDownload(format!("no URL for {}", task.path.display()))
    }))
}

fn fabric_library_urls(base_url: &str, group_path: &str, artifact: &str, version: &str, jar: &str) -> Vec<String> {
    let base = base_url.trim_end_matches('/');
    let primary = format!("{base}/{group_path}/{artifact}/{version}/{jar}");
    let mut urls = vec![primary.clone()];

    // Fabric's canonical host is maven.fabricmc.net, but some networks/proxies
    // intermittently fail parallel TLS connections to it. maven2.fabricmc.net is
    // an official alias/CDN endpoint for the same artifacts, so keep it as a
    // deterministic fallback before surfacing the error to the UI log.
    if base == "https://maven.fabricmc.net" || base == "http://maven.fabricmc.net" {
        urls.push(format!(
            "https://maven2.fabricmc.net/{group_path}/{artifact}/{version}/{jar}"
        ));
    }
    urls
}

fn parse_maven_name(name: &str) -> Option<(String, String, String, Option<String>)> {
    let parts: Vec<&str> = name.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    Some((
        parts[0].replace('.', "/"),
        parts[1].to_string(),
        parts[2].to_string(),
        parts.get(3).map(|s| s.to_string()),
    ))
}

pub fn download_with_sha1(url: &str, path: &Path, expected_sha1: Option<&str>) -> Result<(), InstallError> {
    if path.exists() {
        if let Some(expected) = expected_sha1 {
            let existing = sha1_file(path)?;
            if existing.eq_ignore_ascii_case(expected) {
                return Ok(());
            }
        } else {
            return Ok(());
        }
    }

    let bytes = crate::http::get_bytes(url)?;

    if let Some(expected) = expected_sha1 {
        let actual = format!("{:x}", sha1::Sha1::digest(&bytes));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(InstallError::MissingDownload(format!(
                "sha1 mismatch for {}",
                path.display()
            )));
        }
    }

    fs::create_dir_all(path.parent().unwrap())?;
    let mut file = fs::File::create(path)?;
    file.write_all(&bytes)?;
    Ok(())
}

pub fn sha1_file(path: &Path) -> Result<String, InstallError> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", sha1::Sha1::digest(&bytes)))
}

fn maven_path(libraries_dir: &Path, maven_path: &str) -> PathBuf {
    libraries_dir.join(maven_path.replace('/', std::path::MAIN_SEPARATOR_STR))
}

fn library_allowed(lib: &Library) -> bool {
    if lib.rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in &lib.rules {
        let applies = rule_applies(rule);
        if applies {
            allowed = rule.action == "allow";
        }
    }
    allowed
}

fn rule_applies(rule: &Rule) -> bool {
    match (&rule.os, &rule.features) {
        (Some(_), Some(_)) => false,
        (Some(os), None) => os.name.as_deref() == Some(current_os()),
        (None, Some(_)) => false,
        (None, None) => true,
    }
}

fn current_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

fn native_classifier() -> &'static str {
    if cfg!(target_os = "windows") {
        "natives-windows"
    } else if cfg!(target_os = "macos") {
        "natives-osx"
    } else {
        "natives-linux"
    }
}

fn extract_natives(archive_path: &Path, natives_dir: &Path) -> Result<(), InstallError> {
    fs::create_dir_all(natives_dir)?;
    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.is_file() {
            let name = entry.name();
            let out_path = natives_dir.join(name);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = fs::File::create(out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}

fn parse_arguments(args: &Option<Arguments>, legacy: &Option<String>) -> (Vec<String>, Vec<String>) {
    if let Some(legacy) = legacy {
        let game_args = legacy.split_whitespace().map(String::from).collect();
        return (Vec::new(), game_args);
    }

    let empty = Arguments::default();
    let args = args.as_ref().unwrap_or(&empty);
    let jvm = args
        .jvm
        .iter()
        .flat_map(|a| match a {
            Argument::String(s) => vec![s.clone()],
            Argument::Object { rules, value } => {
                if rule_matches(rules) {
                    match value {
                        ArgValue::Single(s) => vec![s.clone()],
                        ArgValue::Many(v) => v.clone(),
                    }
                } else {
                    Vec::new()
                }
            }
        })
        .collect();
    let game = args
        .game
        .iter()
        .flat_map(|a| match a {
            Argument::String(s) => vec![s.clone()],
            Argument::Object { rules, value } => {
                if rule_matches(rules) {
                    match value {
                        ArgValue::Single(s) => vec![s.clone()],
                        ArgValue::Many(v) => v.clone(),
                    }
                } else {
                    Vec::new()
                }
            }
        })
        .collect();
    (jvm, game)
}

fn rule_matches(rules: &[Rule]) -> bool {
    rules.iter().all(|r| rule_applies(r))
}

#[derive(Debug, Deserialize)]
struct VersionManifest {
    versions: Vec<ManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    id: String,
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionJson {
    id: String,
    main_class: String,
    downloads: Downloads,
    libraries: Vec<Library>,
    asset_index: AssetIndexEntry,
    arguments: Option<Arguments>,
    #[serde(rename = "minecraftArguments")]
    legacy_arguments: Option<String>,
    logging: Option<LoggingConfig>,
}

#[derive(Debug, Deserialize)]
struct Downloads {
    client: Download,
}

#[derive(Debug, Deserialize)]
struct Download {
    url: String,
    sha1: String,
}

#[derive(Debug, Deserialize)]
struct Library {
    #[serde(default)]
    #[allow(dead_code)]
    name: String,
    downloads: LibraryDownloads,
    #[serde(default)]
    rules: Vec<Rule>,
}

#[derive(Debug, Deserialize)]
struct LibraryDownloads {
    artifact: Artifact,
    #[serde(default)]
    classifiers: Option<std::collections::HashMap<String, Artifact>>,
}

#[derive(Debug, Deserialize)]
struct Artifact {
    path: String,
    url: String,
    sha1: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssetIndexEntry {
    id: String,
    url: String,
    sha1: String,
}

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: std::collections::HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    #[serde(default)]
    #[allow(dead_code)]
    size: u64,
}

#[derive(Debug, Deserialize, Default)]
struct Arguments {
    #[serde(default)]
    jvm: Vec<Argument>,
    #[serde(default)]
    game: Vec<Argument>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Argument {
    String(String),
    Object { rules: Vec<Rule>, value: ArgValue },
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ArgValue {
    Single(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
struct Rule {
    action: String,
    os: Option<RuleOs>,
    features: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RuleOs {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    client: Option<LoggingEntry>,
}

#[derive(Debug, Deserialize)]
struct LoggingEntry {
    #[serde(default)]
    #[allow(dead_code)]
    argument: String,
    file: LoggingFile,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct LoggingFile {
    id: String,
    url: String,
    sha1: String,
    #[serde(default)]
    #[allow(dead_code)]
    size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FabricProfileJson {
    #[serde(default)]
    libraries: Vec<FabricLibrary>,
    #[serde(default)]
    main_class: String,
    arguments: Option<Arguments>,
}

#[derive(Debug, Deserialize)]
struct FabricLibrary {
    name: String,
    #[serde(default = "default_fabric_maven_url")]
    url: String,
    #[serde(default)]
    sha1: Option<String>,
}

fn default_fabric_maven_url() -> String {
    "https://maven.fabricmc.net/".to_string()
}
