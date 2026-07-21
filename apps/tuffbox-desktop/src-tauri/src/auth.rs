use keyring::Entry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const KEYRING_SERVICE: &str = "dev.tuffbox.ide";
const MICROSOFT_CLIENT_ID: &str = "89484d4e-6ac2-4643-a786-21386f3269c5";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

// Mutex protecting concurrent reads/writes to auth.json and mc_accounts.json.
// Lock is held only during file I/O (brief), so a std::sync::Mutex is fine
// even in async context.
static AUTH_FILE_MUTEX: Mutex<()> = Mutex::new(());

// Cache for mc_get_auth_status: prevents network refresh on every call.
// The frontend polls this on every focus/navigation, so we skip the refresh
// if less than 30 seconds have elapsed since the last successful one.
static LAST_AUTH_REFRESH: Mutex<Option<Instant>> = Mutex::new(None);
const AUTH_REFRESH_TTL: Duration = Duration::from_secs(30);

// ─── Skin source ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkinSource {
    Mojang,
    Elyby,
    TLauncher,
    Offline,
}

impl Default for SkinSource {
    fn default() -> Self {
        Self::Mojang
    }
}

impl std::fmt::Display for SkinSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkinSource::Mojang => write!(f, "Mojang"),
            SkinSource::Elyby => write!(f, "Ely.by"),
            SkinSource::TLauncher => write!(f, "TLauncher"),
            SkinSource::Offline => write!(f, "Offline"),
        }
    }
}

// ─── Login type ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LoginType {
    Microsoft,
    Offline,
}

impl Default for LoginType {
    fn default() -> Self {
        Self::Offline
    }
}

// ─── Token storage ────────────────────────────────────────────────

fn keyring_entry(name: &str) -> Result<Entry, String> {
    Entry::new(KEYRING_SERVICE, name).map_err(|e| e.to_string())
}

fn save_token(name: &str, value: &str) -> Result<(), String> {
    keyring_entry(name)?
        .set_password(value)
        .map_err(|e| e.to_string())
}

fn load_token(name: &str) -> Result<String, String> {
    keyring_entry(name)?
        .get_password()
        .map_err(|_| "not logged in".to_string())
}

fn clear_token(name: &str) -> Result<(), String> {
    let entry = keyring_entry(name)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ─── Per-account token helpers ────────────────────────────────────

fn account_refresh_key(uuid: &str) -> String {
    format!("mc-refresh-{uuid}")
}

fn account_access_key(uuid: &str) -> String {
    format!("mc-access-{uuid}")
}

// ─── Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McProfile {
    pub uuid: String,
    pub name: String,
    pub skin_url: Option<String>,
    pub cape_url: Option<String>,
    #[serde(default)]
    pub capes: Vec<McCapeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McCapeEntry {
    pub id: String,
    pub alias: Option<String>,
    pub url: String,
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    #[allow(dead_code)]
    expires_in: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct McProfileResponse {
    id: String,
    name: String,
    skins: Option<Vec<McSkinEntry>>,
    capes: Option<Vec<McCapeRawEntry>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct McSkinEntry {
    id: String,
    state: String,
    url: String,
    variant: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct McCapeRawEntry {
    id: String,
    alias: Option<String>,
    url: String,
    state: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TextureResponse {
    textures: TexturesData,
}

#[derive(Debug, Serialize, Deserialize)]
struct TexturesData {
    #[serde(alias = "SKIN")]
    skin: Option<TextureInfo>,
    #[serde(alias = "CAPE")]
    cape: Option<TextureInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TextureInfo {
    url: String,
}

// ─── Multi-account types ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountEntry {
    pub uuid: String,
    pub name: String,
    pub login_type: LoginType,
    pub skin_source: SkinSource,
    pub added_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsFile {
    pub active_account_uuid: Option<String>,
    pub accounts: Vec<AccountEntry>,
}

impl Default for AccountsFile {
    fn default() -> Self {
        Self {
            active_account_uuid: None,
            accounts: Vec::new(),
        }
    }
}

// ─── Auth state persisted to disk ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthState {
    pub logged_in: bool,
    pub profile: Option<McProfile>,
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub login_type: LoginType,
    #[serde(default)]
    pub skin_source: SkinSource,
    #[serde(default)]
    pub accounts: Vec<AccountEntry>,
    pub active_account_uuid: Option<String>,
}

fn accounts_path() -> PathBuf {
    dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("mc_accounts.json")
}

fn auth_state_path() -> PathBuf {
    dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("mc_auth.json")
}

fn load_accounts_file() -> AccountsFile {
    let _guard = AUTH_FILE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    fs::read_to_string(accounts_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_accounts_file(data: &AccountsFile) -> Result<(), String> {
    let _guard = AUTH_FILE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let path = accounts_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_auth_state() -> AuthState {
    let _guard = AUTH_FILE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    fs::read_to_string(auth_state_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_auth_state(state: &AuthState) -> Result<(), String> {
    let _guard = AUTH_FILE_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let path = auth_state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(state).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ─── HTTP helpers ────────────────────────────────────────────────

fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())
}

// ─── Offline UUID ────────────────────────────────────────────────

fn offline_uuid(name: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(b"OfflinePlayer:");
    hasher.update(name.as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}

// ─── Skin fetching from multiple sources ─────────────────────────

async fn fetch_skin_elyby(username: &str) -> Option<String> {
    let c = client().ok()?;
    let url = format!("http://skinsystem.ely.by/skins/{username}.png");
    let resp = c.get(&url).send().await.ok()?;
    if resp.status().is_success() {
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if content_type.contains("image") {
            return Some(url);
        }
    }
    let url = format!("http://skinsystem.ely.by/textures/{username}");
    let resp = c.get(&url).send().await.ok()?;
    if resp.status().is_success() {
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if ct.contains("image") {
            return Some(url);
        }
    }
    None
}

async fn fetch_skin_tlauncher(username: &str) -> Option<String> {
    let c = client().ok()?;
    let url = format!("https://www.tlauncher.org/skins/{username}.png");
    let resp = c.get(&url).send().await.ok()?;
    if resp.status().is_success() {
        let ct = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if ct.contains("image") {
            return Some(url);
        }
    }
    None
}

async fn fetch_skin_mojang(uuid: &str) -> Option<String> {
    let c = client().ok()?;
    let resp = c
        .get(format!(
            "https://sessionserver.mojang.com/session/minecraft/profile/{uuid}"
        ))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body: Value = resp.json().await.ok()?;
    let texture_b64 = body
        .get("properties")
        .and_then(|p| p.as_array())
        .and_then(|props| {
            props
                .iter()
                .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("textures"))
        })
        .and_then(|p| p.get("value"))
        .and_then(|v| v.as_str())?;

    let decoded = base64_decode(texture_b64)?;
    let tex: TextureResponse = serde_json::from_str(&decoded).ok()?;
    tex.textures.skin.map(|s| s.url)
}

async fn fetch_skin_for_username(username: &str, source: &SkinSource) -> Option<String> {
    match source {
        SkinSource::Elyby => fetch_skin_elyby(username).await,
        SkinSource::TLauncher => fetch_skin_tlauncher(username).await,
        SkinSource::Mojang => {
            let c = client().ok()?;
            let resp = c
                .get(format!(
                    "https://api.mojang.com/users/profiles/minecraft/{username}"
                ))
                .send()
                .await
                .ok()?;
            if resp.status().is_success() {
                let body: Value = resp.json().await.ok()?;
                if let Some(uuid) = body.get("id").and_then(|v| v.as_str()) {
                    return fetch_skin_mojang(uuid).await;
                }
            }
            None
        }
        SkinSource::Offline => None,
    }
}

fn base64_decode(input: &str) -> Option<String> {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    let decoded = engine.decode(input).ok()?;
    String::from_utf8(decoded).ok()
}

// ─── Device code flow ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeInfo {
    pub user_code: String,
    pub verification_uri: String,
    pub message: String,
    pub expires_in: u64,
}

pub async fn start_device_code_flow() -> Result<(DeviceCodeInfo, String, u64), String> {
    let c = client()?;
    let resp = c
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&[
            ("client_id", MICROSOFT_CLIENT_ID),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await
        .map_err(|e| format!("device code request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("device code failed ({status}): {body}"));
    }

    let data: DeviceCodeResponse = resp.json().await.map_err(|e| e.to_string())?;
    let info = DeviceCodeInfo {
        user_code: data.user_code.clone(),
        verification_uri: data.verification_uri.clone(),
        message: format!(
            "Go to {} and enter code: {}",
            data.verification_uri, data.user_code
        ),
        expires_in: data.expires_in,
    };
    Ok((info, data.device_code, data.interval))
}

pub async fn poll_device_code_token(
    device_code: &str,
    interval: u64,
) -> Result<TokenResponse, String> {
    let c = client()?;
    let start = Instant::now();
    let max_wait = Duration::from_secs(900);

    loop {
        if start.elapsed() > max_wait {
            return Err("Login timed out".to_string());
        }

        tokio::time::sleep(Duration::from_secs(interval)).await;

        let resp = c
            .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
            .form(&[
                ("client_id", MICROSOFT_CLIENT_ID),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", device_code),
            ])
            .send()
            .await
            .map_err(|e| format!("token poll failed: {e}"))?;

        let status = resp.status();
        let body: Value = resp.json().await.map_err(|e| e.to_string())?;

        if status.is_success() {
            let token_response: TokenResponse =
                serde_json::from_value(body).map_err(|e| e.to_string())?;
            return Ok(token_response);
        }

        let error = body
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        match error {
            "authorization_pending" => continue,
            "slow_down" => {
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            "authorization_declined" => return Err("Login was declined".to_string()),
            "expired_token" => return Err("Device code expired".to_string()),
            "bad_verification_code" => return Err("Invalid device code".to_string()),
            _ => {
                let desc = body
                    .get("error_description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                return Err(format!("Login error: {error}: {desc}"));
            }
        }
    }
}

// ─── Full auth chain: MS → XBL → XSTS → MC ──────────────────────

async fn authenticate_with_xbl(ms_token: &str) -> Result<(String, String), String> {
    let c = client()?;
    let resp = c
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|e| format!("XBL auth failed: {e}"))?;

    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    let token = body
        .get("Token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "XBL response missing token".to_string())?
        .to_string();
    let userhash = body
        .get("DisplayClaims")
        .and_then(|dc| dc.get("xui"))
        .and_then(|xui| xui.as_array())
        .and_then(|arr| arr.first())
        .and_then(|claim| claim.get("uhs"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "XBL response missing userhash".to_string())?
        .to_string();
    Ok((token.to_string(), userhash))
}

async fn authenticate_with_xsts(xbl_token: &str) -> Result<String, String> {
    let c = client()?;
    let resp = c
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbl_token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }))
        .send()
        .await
        .map_err(|e| format!("XSTS auth failed: {e}"))?;

    let body: Value = resp.json().await.map_err(|e| e.to_string())?;

    if let Some(err) = body.get("XErr") {
        let err_code = err.as_u64().unwrap_or(0);
        let message = body
            .get("Message")
            .and_then(|v| v.as_str())
            .unwrap_or("XSTS authorization failed");
        return Err(format!(
            "XSTS error {err_code}: {message}. Ensure your Microsoft account has Xbox Live access."
        ));
    }

    body.get("Token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "XSTS response missing token".to_string())
}

async fn authenticate_with_minecraft(userhash: &str, xsts_token: &str) -> Result<String, String> {
    let c = client()?;

    // Try launcher/login first (more reliable), fallback to login_with_xbox
    let identity_token = format!("XBL3.0 x={userhash};{xsts_token}");

    let resp = c
        .post("https://api.minecraftservices.com/launcher/login")
        .json(&serde_json::json!({
            "identityToken": identity_token
        }))
        .send()
        .await;

    match resp {
        Ok(r) if r.status().is_success() => {
            let body: Value = r.json().await.map_err(|e| e.to_string())?;
            if let Some(token) = body.get("access_token").and_then(|v| v.as_str()) {
                return Ok(token.to_string());
            }
        }
        _ => {}
    }

    // Fallback: login_with_xbox
    let resp = c
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&serde_json::json!({
            "identityToken": format!("XBL3.0 x={userhash};{xsts_token}")
        }))
        .send()
        .await
        .map_err(|e| format!("MC auth failed: {e}"))?;

    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    body.get("access_token")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "MC auth response missing access_token".to_string())
}

async fn fetch_mc_profile(mc_token: &str) -> Result<McProfile, String> {
    let c = client()?;
    let resp = c
        .get(MC_PROFILE_URL)
        .bearer_auth(mc_token)
        .send()
        .await
        .map_err(|e| format!("MC profile fetch failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("MC profile error ({status}): {body}"));
    }

    let data: McProfileResponse = resp.json().await.map_err(|e| e.to_string())?;

    let skin_url = data
        .skins
        .as_ref()
        .and_then(|skins| skins.iter().find(|s| s.url.contains("texture")))
        .map(|s| s.url.clone());

    let skin_url = match skin_url {
        Some(url) => Some(url),
        None => fetch_skin_mojang(&data.id).await,
    };

    let capes: Vec<McCapeEntry> = data
        .capes
        .as_ref()
        .map(|capes| {
            capes
                .iter()
                .map(|c| McCapeEntry {
                    id: c.id.clone(),
                    alias: c.alias.clone(),
                    url: c.url.clone(),
                    state: c.state.clone(),
                })
                .collect()
        })
        .unwrap_or_default();

    let cape_url = capes.iter().find(|c| c.state == "ACTIVE").map(|c| c.url.clone());

    Ok(McProfile {
        uuid: data.id,
        name: data.name,
        skin_url,
        cape_url,
        capes,
    })
}

// ─── Entitlement check ───────────────────────────────────────────

pub async fn check_minecraft_entitlement(mc_token: &str) -> Result<bool, String> {
    let c = client()?;
    let resp = c
        .get("https://api.minecraftservices.com/entitlements/mcstore")
        .bearer_auth(mc_token)
        .send()
        .await
        .map_err(|e| format!("entitlement check failed: {e}"))?;

    if !resp.status().is_success() {
        return Ok(false);
    }

    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    let has_game = body
        .get("items")
        .and_then(|items| items.as_array())
        .map(|items| {
            items
                .iter()
                .any(|item| item.get("name").and_then(|n| n.as_str()) == Some("game_minecraft"))
        })
        .unwrap_or(false);
    Ok(has_game)
}

// ─── Skin upload ─────────────────────────────────────────────────

pub async fn apply_minecraft_skin(
    mc_token: &str,
    skin_url: &str,
    variant: &str,
) -> Result<(), String> {
    let c = client()?;
    let resp = c
        .post(format!("{MC_PROFILE_URL}/skins"))
        .bearer_auth(mc_token)
        .json(&serde_json::json!({
            "variant": variant,
            "url": skin_url
        }))
        .send()
        .await
        .map_err(|e| format!("skin upload failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("skin upload error ({status}): {body}"));
    }
    Ok(())
}

pub async fn apply_minecraft_cape(mc_token: &str, cape_id: &str) -> Result<(), String> {
    let c = client()?;
    let resp = c
        .put(format!("{MC_PROFILE_URL}/capes/{cape_id}/activate"))
        .bearer_auth(mc_token)
        .send()
        .await
        .map_err(|e| format!("cape activate failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("cape activate error ({status}): {body}"));
    }
    Ok(())
}

// ─── Refresh token ───────────────────────────────────────────────

pub async fn refresh_minecraft_token(refresh_token: &str) -> Result<TokenResponse, String> {
    let c = client()?;
    let resp = c
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&[
            ("client_id", MICROSOFT_CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", "XboxLive.signin offline_access"),
        ])
        .send()
        .await
        .map_err(|e| format!("token refresh failed: {e}"))?;

    let status = resp.status();
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        let error = body
            .get("error_description")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(format!("Token refresh failed: {error}"));
    }

    serde_json::from_value(body).map_err(|e| e.to_string())
}

// ─── Full login flow ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResult {
    pub profile: McProfile,
    pub mc_access_token: String,
}

pub async fn complete_microsoft_login(ms_token: &str) -> Result<LoginResult, String> {
    let (xbl_token, userhash) = authenticate_with_xbl(ms_token).await?;
    let xsts_token = authenticate_with_xsts(&xbl_token).await?;
    let mc_token = authenticate_with_minecraft(&userhash, &xsts_token).await?;
    let profile = fetch_mc_profile(&mc_token).await?;
    Ok(LoginResult {
        profile,
        mc_access_token: mc_token,
    })
}

pub async fn login_with_refresh_token(refresh_token: &str) -> Result<LoginResult, String> {
    let token_resp = refresh_minecraft_token(refresh_token).await?;
    complete_microsoft_login(&token_resp.access_token).await
}

// ─── Skin caching ────────────────────────────────────────────────

fn skin_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("skins")
}

pub fn cached_skin_path(uuid: &str) -> PathBuf {
    skin_cache_dir().join(format!("{uuid}.png"))
}

pub async fn download_and_cache_skin(skin_url: &str, uuid: &str) -> Result<PathBuf, String> {
    let path = cached_skin_path(uuid);
    if path.exists() {
        if let Ok(meta) = fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    if elapsed < Duration::from_secs(86400) {
                        return Ok(path);
                    }
                }
            }
        }
    }

    let c = client()?;
    let bytes = c
        .get(skin_url)
        .send()
        .await
        .map_err(|e| format!("skin download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("skin download body failed: {e}"))?;

    let dir = skin_cache_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn load_mc_access_token() -> Result<String, String> {
    // Try per-account key first, then legacy
    if let Ok(accounts) = fs::read_to_string(accounts_path()) {
        if let Ok(data) = serde_json::from_str::<AccountsFile>(&accounts) {
            if let Some(ref uuid) = data.active_account_uuid {
                if let Ok(token) = load_token(&account_access_key(uuid)) {
                    return Ok(token);
                }
            }
        }
    }
    load_token("mc-access-token")
}

// ─── Skin as base64 for 3D viewer ───────────────────────────────

pub async fn fetch_skin_as_base64(url: &str) -> Result<String, String> {
    let c = client()?;
    let bytes = c
        .get(url)
        .send()
        .await
        .map_err(|e| format!("skin fetch failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("skin fetch body failed: {e}"))?;

    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;
    Ok(format!("data:image/png;base64,{}", engine.encode(&bytes)))
}

// ─── Multi-account helpers ───────────────────────────────────────

fn add_account_to_list(entry: &AccountEntry) -> Result<(), String> {
    let mut data = load_accounts_file();
    data.accounts.retain(|a| a.uuid != entry.uuid);
    data.accounts.push(entry.clone());
    if data.active_account_uuid.is_none() {
        data.active_account_uuid = Some(entry.uuid.clone());
    }
    save_accounts_file(&data)
}

fn remove_account_from_list(uuid: &str) -> Result<(), String> {
    let mut data = load_accounts_file();
    data.accounts.retain(|a| a.uuid != uuid);
    if data.active_account_uuid.as_deref() == Some(uuid) {
        data.active_account_uuid = data.accounts.first().map(|a| a.uuid.clone());
    }
    save_accounts_file(&data)
}

fn set_active_account(uuid: &str) -> Result<(), String> {
    let mut data = load_accounts_file();
    if data.accounts.iter().any(|a| a.uuid == uuid) {
        data.active_account_uuid = Some(uuid.to_string());
        save_accounts_file(&data)?;
    }
    Ok(())
}

fn sync_auth_state_from_accounts() -> Result<(), String> {
    let accounts = load_accounts_file();
    let mut state = load_auth_state();
    state.accounts = accounts.accounts.clone();
    state.active_account_uuid = accounts.active_account_uuid.clone();

    // Sync active account profile
    if let Some(ref uuid) = accounts.active_account_uuid {
        if let Some(entry) = accounts.accounts.iter().find(|a| &a.uuid == uuid) {
            state.login_type = entry.login_type.clone();
            state.skin_source = entry.skin_source.clone();
        }
    }
    save_auth_state(&state)
}

// ─── Tauri commands ──────────────────────────────────────────────

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_start_device_code() -> Result<DeviceCodeInfo, String> {
    let (info, device_code, interval) = start_device_code_flow().await?;
    save_token("mc-device-code", &device_code)?;
    save_token("mc-device-interval", &interval.to_string())?;
    Ok(info)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_poll_device_code() -> Result<LoginResult, String> {
    let device_code = load_token("mc-device-code")?;
    let interval: u64 = load_token("mc-device-interval")?.parse().unwrap_or(5);

    let token_resp = poll_device_code_token(&device_code, interval).await?;

    let _ = clear_token("mc-device-code");
    let _ = clear_token("mc-device-interval");
    let login = complete_microsoft_login(&token_resp.access_token).await?;

    // Save per-account tokens
    if let Some(ref rt) = token_resp.refresh_token {
        save_token(&account_refresh_key(&login.profile.uuid), rt)?;
    }
    save_token(&account_access_key(&login.profile.uuid), &login.mc_access_token)?;

    // Also save legacy keys for load_mc_access_token
    save_token("mc-access-token", &login.mc_access_token)?;

    // Add to accounts list
    let entry = AccountEntry {
        uuid: login.profile.uuid.clone(),
        name: login.profile.name.clone(),
        login_type: LoginType::Microsoft,
        skin_source: SkinSource::Mojang,
        added_at: now_secs(),
    };
    add_account_to_list(&entry)?;

    // Update auth state
    let accounts = load_accounts_file();
    let state = AuthState {
        logged_in: true,
        profile: Some(login.profile.clone()),
        expires_at: Some(now_secs() + 86400),
        login_type: LoginType::Microsoft,
        skin_source: SkinSource::Mojang,
        accounts: accounts.accounts,
        active_account_uuid: accounts.active_account_uuid,
    };
    save_auth_state(&state)?;

    if let Some(ref skin_url) = login.profile.skin_url {
        let _ = download_and_cache_skin(skin_url, &login.profile.uuid).await;
    }

    Ok(login)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_offline_login(
    username: String,
    skin_source: SkinSource,
) -> Result<LoginResult, String> {
    let trimmed = username.trim().to_string();
    if trimmed.is_empty() {
        return Err("Username cannot be empty".to_string());
    }
    if trimmed.len() > 16 {
        return Err("Username must be 16 characters or less".to_string());
    }

    let uuid = offline_uuid(&trimmed);

    let skin_url = fetch_skin_for_username(&trimmed, &skin_source).await;

    let profile = McProfile {
        uuid: uuid.clone(),
        name: trimmed,
        skin_url: skin_url.clone(),
        cape_url: None,
        capes: vec![],
    };

    if let Some(ref url) = skin_url {
        let _ = download_and_cache_skin(url, &uuid).await;
    }

    let entry = AccountEntry {
        uuid: uuid.clone(),
        name: profile.name.clone(),
        login_type: LoginType::Offline,
        skin_source: skin_source.clone(),
        added_at: now_secs(),
    };
    add_account_to_list(&entry)?;

    let accounts = load_accounts_file();
    let state = AuthState {
        logged_in: true,
        profile: Some(profile.clone()),
        expires_at: None,
        login_type: LoginType::Offline,
        skin_source: skin_source.clone(),
        accounts: accounts.accounts,
        active_account_uuid: accounts.active_account_uuid,
    };
    save_auth_state(&state)?;

    Ok(LoginResult {
        profile,
        mc_access_token: "0".to_string(),
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_get_auth_status() -> Result<AuthState, String> {
    let mut state = load_auth_state();
    let accounts = load_accounts_file();
    state.accounts = accounts.accounts;
    state.active_account_uuid = accounts.active_account_uuid;

    // Only refresh Microsoft tokens; offline login persists until explicit logout.
    // Skip the network call if we refreshed recently (frontend polls on every focus).
    let should_refresh = state.logged_in
        && state.login_type == LoginType::Microsoft
        && LAST_AUTH_REFRESH
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map(|t| t.elapsed() >= AUTH_REFRESH_TTL)
            .unwrap_or(true);

    if should_refresh {
        if let Some(ref uuid) = state.active_account_uuid {
            if let Ok(refresh_token) = load_token(&account_refresh_key(uuid)) {
                match login_with_refresh_token(&refresh_token).await {
                    Ok(login) => {
                        state.profile = Some(login.profile.clone());
                        save_token(&account_access_key(uuid), &login.mc_access_token)?;
                        save_token("mc-access-token", &login.mc_access_token)?;
                        if let Some(ref skin_url) = login.profile.skin_url {
                            let _ =
                                download_and_cache_skin(skin_url, &login.profile.uuid).await;
                        }
                        save_auth_state(&state)?;
                        if let Ok(mut last) = LAST_AUTH_REFRESH.lock() {
                            *last = Some(Instant::now());
                        }
                    }
                    Err(_) => {
                        state.logged_in = false;
                        state.profile = None;
                        save_auth_state(&state)?;
                        let _ = clear_token(&account_refresh_key(uuid));
                        let _ = clear_token(&account_access_key(uuid));
                    }
                }
            }
        }
    }

    // For offline login, refresh skin from selected source
    if state.logged_in && state.login_type == LoginType::Offline {
        if let Some(ref profile) = state.profile {
            let skin_url = fetch_skin_for_username(&profile.name, &state.skin_source).await;
            if let Some(ref url) = skin_url {
                let _ = download_and_cache_skin(url, &profile.uuid).await;
            }
            let updated_profile = McProfile {
                skin_url: skin_url.or_else(|| profile.skin_url.clone()),
                ..profile.clone()
            };
            state.profile = Some(updated_profile);
            save_auth_state(&state)?;
        }
    }

    Ok(state)
}

#[tauri::command(rename_all = "camelCase")]
pub fn mc_logout() -> Result<(), String> {
    // Clear current account tokens
    let state = load_auth_state();
    if let Some(ref uuid) = state.active_account_uuid {
        let _ = clear_token(&account_refresh_key(uuid));
        let _ = clear_token(&account_access_key(uuid));
    }
    let _ = clear_token("mc-refresh-token");
    let _ = clear_token("mc-access-token");
    let _ = clear_token("mc-device-code");
    let _ = clear_token("mc-device-interval");

    let accounts = load_accounts_file();
    let new_state = AuthState {
        logged_in: false,
        profile: None,
        expires_at: None,
        login_type: LoginType::default(),
        skin_source: SkinSource::default(),
        accounts: accounts.accounts,
        active_account_uuid: accounts.active_account_uuid,
    };
    save_auth_state(&new_state)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_refresh_profile() -> Result<McProfile, String> {
    let state = load_auth_state();

    if state.login_type == LoginType::Microsoft {
        if let Some(ref uuid) = state.active_account_uuid {
            let refresh_token = load_token(&account_refresh_key(uuid))?;
            let login = login_with_refresh_token(&refresh_token).await?;
            save_token(&account_access_key(uuid), &login.mc_access_token)?;
            save_token("mc-access-token", &login.mc_access_token)?;

            let accounts = load_accounts_file();
            let new_state = AuthState {
                logged_in: true,
                profile: Some(login.profile.clone()),
                expires_at: Some(now_secs() + 86400),
                login_type: LoginType::Microsoft,
                skin_source: SkinSource::Mojang,
                accounts: accounts.accounts,
                active_account_uuid: accounts.active_account_uuid,
            };
            save_auth_state(&new_state)?;

            if let Some(ref skin_url) = login.profile.skin_url {
                let _ = download_and_cache_skin(skin_url, &login.profile.uuid).await;
            }

            return Ok(login.profile);
        }
    }

    // Offline: refresh skin from source
    let profile = state.profile.ok_or("Not logged in")?;
    let skin_url = fetch_skin_for_username(&profile.name, &state.skin_source).await;
    if let Some(ref url) = skin_url {
        let _ = download_and_cache_skin(url, &profile.uuid).await;
    }
    let updated = McProfile {
        skin_url: skin_url.or_else(|| profile.skin_url.clone()),
        ..profile
    };
    let accounts = load_accounts_file();
    let new_state = AuthState {
        profile: Some(updated.clone()),
        accounts: accounts.accounts,
        active_account_uuid: accounts.active_account_uuid,
        ..state
    };
    save_auth_state(&new_state)?;
    Ok(updated)
}

#[tauri::command(rename_all = "camelCase")]
pub fn mc_get_skin_path(uuid: String) -> Result<String, String> {
    let path = cached_skin_path(&uuid);
    if path.exists() {
        Ok(path.to_string_lossy().to_string())
    } else {
        Err("skin not cached".to_string())
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_fetch_skin_url(uuid: String) -> Result<Option<String>, String> {
    if let Some(url) = fetch_skin_mojang(&uuid).await {
        let _ = download_and_cache_skin(&url, &uuid).await;
        return Ok(Some(url));
    }
    Ok(None)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_fetch_skin_for_username(
    username: String,
    source: SkinSource,
) -> Result<Option<String>, String> {
    Ok(fetch_skin_for_username(&username, &source).await)
}

#[tauri::command(rename_all = "camelCase")]
pub fn mc_set_skin_source(source: SkinSource) -> Result<(), String> {
    let mut state = load_auth_state();
    state.skin_source = source;
    save_auth_state(&state)
}

// ─── Multi-account commands ──────────────────────────────────────

#[tauri::command(rename_all = "camelCase")]
pub fn mc_list_accounts() -> Result<Vec<AccountEntry>, String> {
    let data = load_accounts_file();
    Ok(data.accounts)
}

#[tauri::command(rename_all = "camelCase")]
pub fn mc_switch_account(uuid: String) -> Result<AuthState, String> {
    set_active_account(&uuid)?;
    sync_auth_state_from_accounts()?;
    Ok(load_auth_state())
}

#[tauri::command(rename_all = "camelCase")]
pub fn mc_remove_account(uuid: String) -> Result<(), String> {
    let _ = clear_token(&account_refresh_key(&uuid));
    let _ = clear_token(&account_access_key(&uuid));
    remove_account_from_list(&uuid)?;
    sync_auth_state_from_accounts()
}

// ─── Skin upload commands ────────────────────────────────────────

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_apply_skin(skin_url: String, variant: String) -> Result<(), String> {
    let access_token = load_mc_access_token()?;
    apply_minecraft_skin(&access_token, &skin_url, &variant).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_apply_cape(cape_id: String) -> Result<(), String> {
    let access_token = load_mc_access_token()?;
    apply_minecraft_cape(&access_token, &cape_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_check_entitlement() -> Result<bool, String> {
    let access_token = load_mc_access_token()?;
    check_minecraft_entitlement(&access_token).await
}

// ─── Skin base64 for 3D viewer ──────────────────────────────────

#[tauri::command(rename_all = "camelCase")]
pub async fn mc_get_skin_base64(url: String) -> Result<String, String> {
    fetch_skin_as_base64(&url).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_state_serializes() {
        let state = AuthState::default();
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("logged_in"));
    }

    #[test]
    fn skin_cache_path_is_deterministic() {
        let a = cached_skin_path("abc123");
        let b = cached_skin_path("abc123");
        assert_eq!(a, b);
    }

    #[test]
    fn offline_uuid_is_deterministic() {
        assert_eq!(offline_uuid("Steve"), offline_uuid("Steve"));
        assert_ne!(offline_uuid("Steve"), offline_uuid("Alex"));
    }

    #[test]
    fn skin_source_serializes() {
        let src = SkinSource::Elyby;
        let json = serde_json::to_string(&src).unwrap();
        assert_eq!(json, "\"elyby\"");
    }

    #[test]
    fn account_entry_serializes() {
        let entry = AccountEntry {
            uuid: "abc123".to_string(),
            name: "Test".to_string(),
            login_type: LoginType::Microsoft,
            skin_source: SkinSource::Mojang,
            added_at: 12345,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("uuid"));
        assert!(json.contains("loginType"));
    }
}
