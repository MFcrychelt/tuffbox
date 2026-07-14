//! CurseForge ("Flame") API client — mirrors PrismLauncher's `FlameAPI`.
//!
//! Auth: `x-api-key` on `api.curseforge.com` and CDN hosts (`edge.forgecdn.net`).
//! Default key matches PrismLauncher's public build-time key; override with
//! `TUFFBOX_CURSEFORGE_API_KEY`.

use crate::http;
use crate::provider::{ProviderError, ProviderFileHashes};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const BASE_URL: &str = "https://api.curseforge.com/v1";
const MINECRAFT_GAME_ID: u32 = 432;
const CLASS_MODPACK: u32 = 4471;
const CLASS_MOD: u32 = 6;
const CLASS_RESOURCEPACK: u32 = 12;
const CLASS_SHADER: u32 = 6552;
const CLASS_DATAPACK: u32 = 6945;

/// PrismLauncher's default CurseForge API key (Overwolf / public launcher builds).
const DEFAULT_API_KEY: &str =
    "$2a$10$wuAJuNZuted3NORVmpgUC.m8sI.pv1tOPKZyBgLFGjxFp/br0lZCC";

#[derive(Debug, Clone)]
pub struct CurseForgeProvider {
    api_key: String,
}

impl Default for CurseForgeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CurseForgeProvider {
    pub fn new() -> Self {
        let api_key = std::env::var("TUFFBOX_CURSEFORGE_API_KEY")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_API_KEY.to_string());
        Self { api_key }
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    pub fn is_configured(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ProviderError> {
        let url = format!("{BASE_URL}{path}");
        cf_get_json(&url, &self.api_key)
    }

    fn post_json<B: Serialize, T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ProviderError> {
        let url = format!("{BASE_URL}{path}");
        cf_post_json(&url, &self.api_key, body)
    }

    /// Search CurseForge modpacks (`classId=4471`).
    pub fn search_modpacks(
        &self,
        query: &str,
        game_version: Option<&str>,
        offset: u32,
        page_size: u32,
    ) -> Result<Vec<CurseForgeSearchHit>, ProviderError> {
        let mut path = format!(
            "/mods/search?gameId={MINECRAFT_GAME_ID}&classId={CLASS_MODPACK}&index={offset}&pageSize={}&sortField=2&sortOrder=desc",
            page_size.clamp(1, 50)
        );
        if !query.trim().is_empty() {
            path.push_str(&format!("&searchFilter={}", urlencoding_minimal(query.trim())));
        }
        if let Some(gv) = game_version.filter(|s| !s.is_empty()) {
            path.push_str(&format!("&gameVersion={}", urlencoding_minimal(gv)));
        }
        let resp: CfData<Vec<CfMod>> = self.get_json(&path)?;
        Ok(resp.data.into_iter().map(Into::into).collect())
    }

    pub fn get_mod(&self, mod_id: u64) -> Result<CurseForgeSearchHit, ProviderError> {
        let resp: CfData<CfMod> = self.get_json(&format!("/mods/{mod_id}"))?;
        Ok(resp.data.into())
    }

    /// List files for a project (modpack or mod).
    pub fn get_mod_files(
        &self,
        mod_id: u64,
        game_version: Option<&str>,
    ) -> Result<Vec<CurseForgeFileInfo>, ProviderError> {
        let mut path = format!("/mods/{mod_id}/files?pageSize=50");
        if let Some(gv) = game_version.filter(|s| !s.is_empty()) {
            path.push_str(&format!("&gameVersion={}", urlencoding_minimal(gv)));
        }
        let resp: CfData<Vec<CfFile>> = self.get_json(&path)?;
        Ok(resp.data.into_iter().map(Into::into).collect())
    }

    pub fn get_file(&self, mod_id: u64, file_id: u64) -> Result<CurseForgeFileInfo, ProviderError> {
        let resp: CfData<CfFile> = self.get_json(&format!("/mods/{mod_id}/files/{file_id}"))?;
        Ok(resp.data.into())
    }

    /// Batch-resolve files by id — Prism's `POST /mods/files`.
    pub fn get_files(
        &self,
        file_ids: &[u64],
    ) -> Result<HashMap<u64, CurseForgeFileInfo>, ProviderError> {
        if file_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut out = HashMap::new();
        for chunk in file_ids.chunks(100) {
            let body = serde_json::json!({ "fileIds": chunk });
            let resp: CfData<Vec<CfFile>> = self.post_json("/mods/files", &body)?;
            for file in resp.data {
                let info: CurseForgeFileInfo = file.into();
                out.insert(info.id, info);
            }
        }
        Ok(out)
    }

    pub fn get_mods(&self, mod_ids: &[u64]) -> Result<HashMap<u64, CurseForgeSearchHit>, ProviderError> {
        if mod_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut out = HashMap::new();
        for chunk in mod_ids.chunks(100) {
            let body = serde_json::json!({ "modIds": chunk });
            let resp: CfData<Vec<CfMod>> = self.post_json("/mods", &body)?;
            for m in resp.data {
                let hit: CurseForgeSearchHit = m.into();
                out.insert(hit.id, hit);
            }
        }
        Ok(out)
    }

    /// Fill empty `download_url` entries via Modrinth SHA1 (Prism `FallbackMRBlockedMods`).
    pub fn apply_modrinth_fallback(
        &self,
        files: &mut HashMap<u64, CurseForgeFileInfo>,
    ) -> Result<(), ProviderError> {
        let hashes: Vec<String> = files
            .values()
            .filter(|f| f.download_url.as_ref().map(|u| u.is_empty()).unwrap_or(true))
            .filter_map(|f| f.hashes.sha1.clone())
            .collect();
        if hashes.is_empty() {
            return Ok(());
        }
        let url = "https://api.modrinth.com/v2/version_files";
        let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });
        let map: HashMap<String, serde_json::Value> = http::post_json(url, &body)
            .map_err(|e| ProviderError::NetworkContext(e.to_string()))?;
        for file in files.values_mut() {
            let Some(sha1) = file.hashes.sha1.as_ref() else {
                continue;
            };
            if file.download_url.as_ref().map(|u| !u.is_empty()).unwrap_or(false) {
                continue;
            }
            let Some(version) = map.get(sha1) else {
                continue;
            };
            if let Some(files_arr) = version.get("files").and_then(|v| v.as_array()) {
                let primary = files_arr
                    .iter()
                    .find(|f| f.get("primary").and_then(|p| p.as_bool()).unwrap_or(false))
                    .or_else(|| files_arr.first());
                if let Some(url) = primary
                    .and_then(|f| f.get("url"))
                    .and_then(|u| u.as_str())
                    .map(|s| s.to_string())
                {
                    file.download_url = Some(url);
                    file.blocked = false;
                }
            }
        }
        Ok(())
    }
}

fn urlencoding_minimal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn cf_get_json<T: serde::de::DeserializeOwned>(
    url: &str,
    api_key: &str,
) -> Result<T, ProviderError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("TuffBox-IDE/0.1.0")
        .build()
        .map_err(|e| ProviderError::NetworkContext(e.to_string()))?;
    let resp = client
        .get(url)
        .header("x-api-key", api_key)
        .header("Accept", "application/json")
        .send()
        .map_err(ProviderError::Network)?;
    let status = resp.status();
    let text = resp.text().map_err(ProviderError::Network)?;
    if !status.is_success() {
        return Err(ProviderError::Api {
            status: status.as_u16(),
            message: text.chars().take(300).collect(),
        });
    }
    serde_json::from_str(&text).map_err(ProviderError::Parse)
}

fn cf_post_json<B: Serialize, T: serde::de::DeserializeOwned>(
    url: &str,
    api_key: &str,
    body: &B,
) -> Result<T, ProviderError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("TuffBox-IDE/0.1.0")
        .build()
        .map_err(|e| ProviderError::NetworkContext(e.to_string()))?;
    let resp = client
        .post(url)
        .header("x-api-key", api_key)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .map_err(ProviderError::Network)?;
    let status = resp.status();
    let text = resp.text().map_err(ProviderError::Network)?;
    if !status.is_success() {
        return Err(ProviderError::Api {
            status: status.as_u16(),
            message: text.chars().take(300).collect(),
        });
    }
    serde_json::from_str(&text).map_err(ProviderError::Parse)
}

/// Returns true when a download URL should carry the CurseForge API key
/// (API host or forge CDN), matching Prism's `ApiHeaderProxy`.
pub fn url_needs_curseforge_key(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("://api.curseforge.com")
        || lower.contains("forgecdn.net")
        || lower.contains("://media.forgecdn.net")
        || lower.contains("://edge.forgecdn.net")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeSearchHit {
    pub id: u64,
    pub slug: String,
    pub name: String,
    pub summary: String,
    pub download_count: u64,
    pub icon_url: Option<String>,
    pub authors: Vec<String>,
    pub categories: Vec<String>,
    pub latest_files: Vec<CurseForgeFileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeFileInfo {
    pub id: u64,
    pub mod_id: u64,
    pub display_name: String,
    pub file_name: String,
    pub download_url: Option<String>,
    pub release_type: u32,
    pub game_versions: Vec<String>,
    pub hashes: ProviderFileHashes,
    pub file_date: Option<String>,
    /// True when CurseForge withheld the CDN URL (author distribution restrictions).
    pub blocked: bool,
    pub class_id: Option<u32>,
}

impl CurseForgeFileInfo {
    pub fn content_folder(&self) -> &'static str {
        match self.class_id.unwrap_or(CLASS_MOD) {
            CLASS_RESOURCEPACK => "resourcepacks",
            CLASS_SHADER => "shaderpacks",
            CLASS_DATAPACK => "datapacks",
            _ => "mods",
        }
    }
}

#[derive(Debug, Deserialize)]
struct CfData<T> {
    data: T,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfMod {
    id: u64,
    #[serde(default)]
    slug: String,
    name: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    logo: Option<CfLogo>,
    #[serde(default)]
    authors: Vec<CfAuthor>,
    #[serde(default)]
    categories: Vec<CfCategory>,
    #[serde(default)]
    latest_files: Vec<CfFile>,
    #[serde(default)]
    #[allow(dead_code)]
    class_id: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CfLogo {
    #[serde(default, rename = "thumbnailUrl")]
    thumbnail_url: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CfAuthor {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CfCategory {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfFile {
    id: u64,
    #[serde(default)]
    mod_id: u64,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    file_name: String,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    release_type: u32,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    hashes: Vec<CfHash>,
    #[serde(default)]
    file_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CfHash {
    value: String,
    /// 1 = sha1, 2 = md5 on CurseForge.
    algo: u32,
}

impl From<CfMod> for CurseForgeSearchHit {
    fn from(m: CfMod) -> Self {
        Self {
            id: m.id,
            slug: m.slug,
            name: m.name,
            summary: m.summary,
            download_count: m.download_count,
            icon_url: m
                .logo
                .as_ref()
                .and_then(|l| l.thumbnail_url.clone().or_else(|| l.url.clone())),
            authors: m.authors.into_iter().map(|a| a.name).collect(),
            categories: m.categories.into_iter().map(|c| c.name).collect(),
            latest_files: m.latest_files.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<CfFile> for CurseForgeFileInfo {
    fn from(f: CfFile) -> Self {
        let mut sha1 = None;
        let sha512 = None;
        for h in f.hashes {
            match h.algo {
                1 => sha1 = Some(h.value),
                _ => {}
            }
            // CurseForge doesn't ship sha512 commonly; keep field for ProviderFileHashes.
            let _ = &sha512;
        }
        let url = f
            .download_url
            .filter(|u| !u.trim().is_empty());
        let blocked = url.is_none();
        Self {
            id: f.id,
            mod_id: f.mod_id,
            display_name: if f.display_name.is_empty() {
                f.file_name.clone()
            } else {
                f.display_name
            },
            file_name: f.file_name,
            download_url: url,
            release_type: f.release_type,
            game_versions: f.game_versions,
            hashes: ProviderFileHashes { sha1, sha512 },
            file_date: f.file_date,
            blocked,
            class_id: None,
        }
    }
}

/// Download a URL that may require the CurseForge `x-api-key` header.
pub fn download_curseforge_url(
    url: &str,
    dest: &std::path::Path,
    expected_sha1: Option<&str>,
) -> Result<(), ProviderError> {
    use sha1::{Digest, Sha1};
    use std::io::{Read, Write};

    let provider = CurseForgeProvider::new();
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .user_agent("TuffBox-IDE/0.1.0")
        .redirect(reqwest::redirect::Policy::limited(8))
        .build()
        .map_err(|e| ProviderError::NetworkContext(e.to_string()))?;

    let mut request = client.get(url);
    if url_needs_curseforge_key(url) {
        request = request.header("x-api-key", provider.api_key());
    }
    let response = request.send().map_err(ProviderError::Network)?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProviderError::Api {
            status: status.as_u16(),
            message: format!("download failed for {url}"),
        });
    }

    let parent = dest.parent().unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::Builder::new()
        .prefix(".tuffbox-download-")
        .suffix(".part")
        .tempfile_in(parent)?;
    let mut hasher = Sha1::new();
    let mut stream = response;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let n = stream.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        file.write_all(&buffer[..n])?;
    }
    file.flush()?;
    let actual = format!("{:x}", hasher.finalize());
    if let Some(expected) = expected_sha1 {
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(ProviderError::NetworkContext(format!(
                "sha1 mismatch: expected {expected}, got {actual}"
            )));
        }
    }
    file.persist(dest)
        .map_err(|e| ProviderError::Io(e.error))?;
    Ok(())
}
