use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tuffbox_core::ProjectManifest;

const KEYRING_SERVICE: &str = "dev.tuffbox.ide";
const DEFAULT_GITHUB_REPOSITORY: &str = "MFcrychelt/tuffbox";
const APP_USER_AGENT: &str = "TuffBox-IDE/0.1";
/// Default Ollama tag for crash plans (smarter; user still must pull once).
pub const DEFAULT_OLLAMA_MODEL: &str = "qwen2.5:7b";
/// Alias kept for call sites that talk about “recommended crash model”.
pub const RECOMMENDED_OLLAMA_CRASH_MODEL: &str = DEFAULT_OLLAMA_MODEL;

/// Pick full vs compact crash Explain prompt for the configured AI settings.
pub fn crash_explain_prompt_for(
    settings: &AiSettings,
    ctx: &tuffbox_core::ai_explanation::CrashAiContext,
) -> (String, bool) {
    let compact = tuffbox_core::ai_explanation::prefers_compact_crash_prompt(
        &settings.provider,
        &settings.model,
    );
    if compact {
        (
            tuffbox_core::ai_explanation::build_compact_crash_prompt(ctx),
            true,
        )
    } else {
        (
            tuffbox_core::ai_explanation::build_crash_prompt(ctx),
            false,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    /// Crash diagnose transport: `server` (default) | `local` | `kb_only`.
    #[serde(default = "default_diagnose_mode")]
    pub diagnose_mode: String,
    /// Base URL for private crash KB API (`/v1/crash/lookup`, `/v1/crash/diagnose`).
    #[serde(default)]
    pub crash_kb_endpoint: String,
    /// Optional path to `ollama` / `ollama.exe`, or the install folder that contains it.
    /// Empty = look up `ollama` on PATH / default install locations.
    #[serde(default)]
    pub ollama_binary_path: String,
    /// Directory where Ollama stores models (`OLLAMA_MODELS`). Empty = Ollama default
    /// (`~/.ollama/models` / `%USERPROFILE%\.ollama\models`).
    #[serde(default)]
    pub ollama_models_path: String,
}

fn default_diagnose_mode() -> String {
    "server".into()
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            endpoint: "http://127.0.0.1:11434".to_string(),
            model: DEFAULT_OLLAMA_MODEL.to_string(),
            diagnose_mode: default_diagnose_mode(),
            crash_kb_endpoint: String::new(),
            ollama_binary_path: String::new(),
            ollama_models_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationSettings {
    pub github_repository: String,
    pub ai: AiSettings,
    #[serde(default)]
    pub swarm: tuffbox_core::swarm::SwarmSettings,
}

impl Default for IntegrationSettings {
    fn default() -> Self {
        Self {
            github_repository: DEFAULT_GITHUB_REPOSITORY.to_string(),
            ai: AiSettings::default(),
            swarm: tuffbox_core::swarm::SwarmSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PublishConfig {
    pub github_repository: String,
    pub modrinth_project_id: String,
    pub curseforge_project_id: String,
    pub curseforge_game_version_ids: Vec<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub settings: IntegrationSettings,
    pub github_token_set: bool,
    pub modrinth_token_set: bool,
    pub curseforge_token_set: bool,
    pub ai_api_key_set: bool,
    pub crash_kb_token_set: bool,
    pub swarm_supabase_anon_set: bool,
    /// Community Supabase URL+key are built into the app (no user setup required).
    pub swarm_supabase_using_builtin: bool,
    pub swarm_supabase_configured: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: Option<String>,
    pub checked_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishResult {
    pub target: String,
    pub id: String,
    pub url: Option<String>,
    pub uploaded_files: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactRecord {
    kind: String,
    path: String,
}

fn settings_path() -> PathBuf {
    dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("integrations.json")
}

pub(crate) fn read_settings() -> IntegrationSettings {
    fs::read_to_string(settings_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn write_settings(settings: &IntegrationSettings) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&tmp, &path)
        .or_else(|_| {
            fs::remove_file(&path).ok();
            fs::rename(&tmp, &path)
        })
        .map_err(|e| e.to_string())?;
    // So tray / future Ollama launches honor the same folder (not only our serve spawn).
    persist_ollama_models_user_env(settings.ai.ollama_models_path.trim());
    Ok(())
}

/// Persist `OLLAMA_MODELS` at the user environment level.
/// Empty path clears the override so Ollama falls back to its default.
/// On Windows prefer PowerShell `[Environment]::SetEnvironmentVariable` over `setx`
/// (Unicode paths, reliable User hive write, immediate read-back for new processes).
fn persist_ollama_models_user_env(models_path: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let ps = if models_path.is_empty() {
            "[Environment]::SetEnvironmentVariable('OLLAMA_MODELS', $null, 'User')".to_string()
        } else {
            let escaped = models_path.replace('\'', "''");
            format!(
                "[Environment]::SetEnvironmentVariable('OLLAMA_MODELS', '{escaped}', 'User')"
            )
        };
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
            .creation_flags(0x08000000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        // Also set for this process so child spawns inherit immediately.
        if models_path.is_empty() {
            std::env::remove_var("OLLAMA_MODELS");
        } else {
            std::env::set_var("OLLAMA_MODELS", models_path);
        }
    }
    #[cfg(not(windows))]
    {
        if models_path.is_empty() {
            std::env::remove_var("OLLAMA_MODELS");
        } else {
            std::env::set_var("OLLAMA_MODELS", models_path);
        }
    }
}

/// Host for a short-lived TuffBox-managed `ollama serve` used only for pulls when a
/// custom models path is set. Avoids racing the tray app on :11434 (which often keeps
/// writing under `%USERPROFILE%\.ollama\models`).
const MANAGED_OLLAMA_HOST: &str = "127.0.0.1:18434";

fn managed_ollama_root() -> String {
    format!("http://{MANAGED_OLLAMA_HOST}")
}

/// Current swarm settings from integrations.json.
pub fn swarm_settings() -> tuffbox_core::swarm::SwarmSettings {
    read_settings().swarm
}

pub fn swarm_enabled() -> bool {
    swarm_settings().enabled
}

/// Prefer TuffSwarm hub URL, else private Crash KB endpoint.
pub fn swarm_network_base() -> Option<String> {
    let s = read_settings();
    tuffbox_core::swarm::resolve_swarm_network_base(&s.swarm.hub_url, &s.ai.crash_kb_endpoint)
}

/// Supabase project URL: Settings override, else built-in community project.
pub fn swarm_supabase_url() -> Option<String> {
    swarm_settings().effective_supabase_url()
}

/// Anon/publishable key: keyring override, else built-in (public by design).
pub fn swarm_supabase_anon_key() -> Option<String> {
    if let Some(k) = secret_optional("swarm_supabase") {
        return Some(k);
    }
    let builtin = tuffbox_core::swarm::BUILTIN_SUPABASE_ANON_KEY.trim();
    if builtin.is_empty() {
        None
    } else {
        Some(builtin.to_string())
    }
}

/// True when effective URL + anon key resolve (built-in counts).
pub fn swarm_supabase_configured() -> bool {
    swarm_supabase_url().is_some() && swarm_supabase_anon_key().is_some()
}

/// Whether the client is using the shipped community Supabase defaults (no overrides).
pub fn swarm_supabase_using_builtin() -> bool {
    let s = swarm_settings();
    s.supabase_url.trim().is_empty() && secret_optional("swarm_supabase").is_none()
}

/// Machine-wide durable capsule store (shared across projects on this PC).
pub fn global_capsule_library() -> tuffbox_core::swarm::CapsuleLibrary {
    let path = dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("swarm")
        .join("capsules.jsonl");
    tuffbox_core::swarm::CapsuleLibrary::open(path)
}

pub fn require_swarm_enabled() -> Result<(), String> {
    if swarm_enabled() {
        Ok(())
    } else {
        Err(
            "TuffSwarm network is disabled. Enable it in Settings → Use TuffSwarm network."
                .into(),
        )
    }
}

/// Complete first-run onboarding and set enabled flag.
#[tauri::command(rename_all = "camelCase")]
pub fn complete_swarm_onboarding(enabled: bool) -> Result<tuffbox_core::swarm::SwarmSettings, String> {
    let mut settings = read_settings();
    settings.swarm.enabled = enabled;
    settings.swarm.onboarding_done = true;
    write_settings(&settings)?;
    Ok(settings.swarm)
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_swarm_settings() -> tuffbox_core::swarm::SwarmSettings {
    swarm_settings()
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_swarm_enabled(enabled: bool) -> Result<tuffbox_core::swarm::SwarmSettings, String> {
    let mut settings = read_settings();
    settings.swarm.enabled = enabled;
    settings.swarm.onboarding_done = true;
    write_settings(&settings)?;
    Ok(settings.swarm)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_swarm_share_prompts(enabled: bool) -> Result<tuffbox_core::swarm::SwarmSettings, String> {
    let mut settings = read_settings();
    settings.swarm.share_prompts_enabled = enabled;
    write_settings(&settings)?;
    Ok(settings.swarm)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_swarm_hub_url(hub_url: String) -> Result<tuffbox_core::swarm::SwarmSettings, String> {
    let mut settings = read_settings();
    settings.swarm.hub_url = hub_url.trim().trim_end_matches('/').to_string();
    write_settings(&settings)?;
    Ok(settings.swarm)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_swarm_supabase_url(
    supabase_url: String,
) -> Result<tuffbox_core::swarm::SwarmSettings, String> {
    let mut settings = read_settings();
    let url = supabase_url.trim().trim_end_matches('/').to_string();
    if !url.is_empty()
        && !url.starts_with("https://")
        && !url.starts_with("http://127.0.0.1")
        && !url.starts_with("http://localhost")
    {
        return Err("Supabase URL must be https://… (or localhost for local stack)".into());
    }
    settings.swarm.supabase_url = url;
    write_settings(&settings)?;
    Ok(settings.swarm)
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_swarm_p2p(
    enabled: bool,
    control_url: Option<String>,
) -> Result<tuffbox_core::swarm::SwarmSettings, String> {
    let mut settings = read_settings();
    settings.swarm.p2p_enabled = enabled;
    settings.swarm.onboarding_done = true;
    if let Some(url) = control_url {
        let url = url.trim().trim_end_matches('/').to_string();
        if !url.is_empty() {
            settings.swarm.p2p_control_url = url;
        }
    }
    if settings.swarm.p2p_control_url.trim().is_empty() {
        settings.swarm.p2p_control_url = tuffbox_core::swarm::SwarmSettings::default()
            .p2p_control_url;
    }
    write_settings(&settings)?;
    Ok(settings.swarm)
}

fn keyring_entry(kind: &str) -> Result<keyring::Entry, String> {
    let account = match kind {
        "github" => "github-token",
        "modrinth" => "modrinth-token",
        "curseforge" => "curseforge-token",
        "ai" => "ai-api-key",
        "crash_kb" => "crash-kb-token",
        "swarm_supabase" => "swarm-supabase-anon",
        _ => return Err(format!("unknown credential kind: {kind}")),
    };
    keyring::Entry::new(KEYRING_SERVICE, account).map_err(|e| e.to_string())
}

fn secret(kind: &str) -> Result<String, String> {
    keyring_entry(kind)?
        .get_password()
        .map_err(|_| format!("{kind} credential is not configured"))
}

pub fn secret_optional(kind: &str) -> Option<String> {
    secret(kind).ok().filter(|s| !s.trim().is_empty())
}

fn secret_is_set(kind: &str) -> bool {
    keyring_entry(kind)
        .and_then(|entry| entry.get_password().map_err(|e| e.to_string()))
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

#[tauri::command]
pub fn get_integration_status() -> IntegrationStatus {
    IntegrationStatus {
        settings: read_settings(),
        github_token_set: secret_is_set("github"),
        modrinth_token_set: secret_is_set("modrinth"),
        curseforge_token_set: secret_is_set("curseforge"),
        ai_api_key_set: secret_is_set("ai"),
        crash_kb_token_set: secret_is_set("crash_kb"),
        swarm_supabase_anon_set: secret_is_set("swarm_supabase"),
        swarm_supabase_using_builtin: swarm_supabase_using_builtin(),
        swarm_supabase_configured: swarm_supabase_configured(),
    }
}

#[tauri::command]
pub fn save_integration_settings(mut settings: IntegrationSettings) -> Result<(), String> {
    if settings.github_repository.split('/').count() != 2 {
        return Err("GitHub repository must use owner/repository format".to_string());
    }
    if settings.ai.endpoint.trim().is_empty() {
        return Err("AI endpoint is required".to_string());
    }
    if settings.ai.provider != "ollama" && settings.ai.model.trim().is_empty() {
        return Err("AI model is required".to_string());
    }
    if !matches!(
        settings.ai.provider.as_str(),
        "ollama" | "openai-compatible"
    ) {
        return Err("AI provider must be ollama or openai-compatible".to_string());
    }
    let mode = tuffbox_core::action_plan::DiagnoseMode::parse(&settings.ai.diagnose_mode);
    settings.ai.diagnose_mode = mode.as_str().to_string();
    // Preserve swarm when older clients omit the field (serde default).
    let existing = read_settings();
    if !settings.swarm.onboarding_done && existing.swarm.onboarding_done {
        settings.swarm = existing.swarm;
    }
    write_settings(&settings)
}

#[tauri::command]
pub fn set_integration_secret(kind: String, value: String) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("credential cannot be empty".to_string());
    }
    keyring_entry(&kind)?
        .set_password(value.trim())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_integration_secret(kind: String) -> Result<(), String> {
    let entry = keyring_entry(&kind)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn publish_config_path(manifest_path: &Path) -> Result<PathBuf, String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest path has no parent directory".to_string())?;
    Ok(project_dir.join(".tuffbox").join("publish.json"))
}

#[tauri::command]
pub fn get_publish_config(path: String) -> Result<PublishConfig, String> {
    let config_path = publish_config_path(Path::new(&path))?;
    let mut config: PublishConfig = fs::read_to_string(config_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    if config.github_repository.is_empty() {
        config.github_repository = read_settings().github_repository;
    }
    Ok(config)
}

#[tauri::command]
pub fn save_publish_config(path: String, config: PublishConfig) -> Result<(), String> {
    if !config.github_repository.is_empty() && config.github_repository.split('/').count() != 2 {
        return Err("GitHub repository must use owner/repository format".to_string());
    }
    let config_path = publish_config_path(Path::new(&path))?;
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        config_path,
        serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn github_headers(
    request: reqwest::RequestBuilder,
    token: Option<&str>,
) -> reqwest::RequestBuilder {
    let request = request
        .header(USER_AGENT, APP_USER_AGENT)
        .header(ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = token {
        request.header(AUTHORIZATION, format!("Bearer {token}"))
    } else {
        request
    }
}

fn normalized_semver(value: &str) -> Option<semver::Version> {
    let trimmed = value.trim().trim_start_matches(['v', 'V']);
    semver::Version::parse(trimmed).ok()
}

#[tauri::command]
pub async fn check_for_app_update() -> Result<UpdateCheck, String> {
    let settings = read_settings();
    let repository = if settings.github_repository.trim().is_empty() {
        DEFAULT_GITHUB_REPOSITORY
    } else {
        settings.github_repository.trim()
    };
    let url = format!("https://api.github.com/repos/{repository}/releases/latest");
    let response = github_headers(reqwest::Client::new().get(url), None)
        .send()
        .await
        .map_err(|e| format!("GitHub update check failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "GitHub update check failed with status {}",
            response.status()
        ));
    }
    let release: Value = response.json().await.map_err(|e| e.to_string())?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let latest_version = release
        .get("tag_name")
        .and_then(Value::as_str)
        .unwrap_or(&current_version)
        .to_string();
    let update_available = match (
        normalized_semver(&current_version),
        normalized_semver(&latest_version),
    ) {
        (Some(current), Some(latest)) => latest > current,
        _ => latest_version != current_version,
    };
    Ok(UpdateCheck {
        current_version,
        latest_version,
        update_available,
        release_url: release
            .get("html_url")
            .and_then(Value::as_str)
            .map(str::to_string),
        checked_at: tuffbox_core::time_util::rfc3339_now(),
    })
}

#[tauri::command]
pub async fn test_integration(provider: String) -> Result<String, String> {
    match provider.as_str() {
        "github" => {
            let token = secret("github")?;
            let response = github_headers(
                reqwest::Client::new().get("https://api.github.com/user"),
                Some(&token),
            )
            .send()
            .await
            .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("GitHub rejected the token ({})", response.status()));
            }
            let body: Value = response.json().await.map_err(|e| e.to_string())?;
            Ok(format!(
                "Connected as {}",
                body.get("login")
                    .and_then(Value::as_str)
                    .unwrap_or("GitHub user")
            ))
        }
        "modrinth" => {
            let token = secret("modrinth")?;
            let response = reqwest::Client::new()
                .get("https://api.modrinth.com/v2/user")
                .header(USER_AGENT, APP_USER_AGENT)
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!(
                    "Modrinth rejected the token ({})",
                    response.status()
                ));
            }
            let body: Value = response.json().await.map_err(|e| e.to_string())?;
            Ok(format!(
                "Connected as {}",
                body.get("username")
                    .and_then(Value::as_str)
                    .unwrap_or("Modrinth user")
            ))
        }
        "curseforge" => {
            let token = secret("curseforge")?;
            let response = reqwest::Client::new()
                .get("https://minecraft.curseforge.com/api/game/versions")
                .header(USER_AGENT, APP_USER_AGENT)
                .header("X-Api-Token", token)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!(
                    "CurseForge rejected the token ({})",
                    response.status()
                ));
            }
            Ok("CurseForge token accepted".to_string())
        }
        "ai" => {
            let settings = read_settings();
            call_ai(&settings.ai, "Respond with exactly: {\"status\":\"ok\"}").await?;
            Ok(format!(
                "{} model {} responded",
                settings.ai.provider, settings.ai.model
            ))
        }
        _ => Err(format!("unknown integration provider: {provider}")),
    }
}

fn read_artifacts(manifest_path: &Path) -> Result<Vec<ArtifactRecord>, String> {
    let project_dir = manifest_path
        .parent()
        .ok_or_else(|| "manifest path has no parent directory".to_string())?;
    let path = project_dir.join(".tuffbox").join("artifacts.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn latest_artifact<'a>(
    artifacts: &'a [ArtifactRecord],
    kind: &str,
) -> Result<&'a ArtifactRecord, String> {
    artifacts
        .iter()
        .rev()
        .find(|artifact| artifact.kind == kind && Path::new(&artifact.path).is_file())
        .ok_or_else(|| format!("export a {kind} artifact before publishing"))
}

#[tauri::command]
pub async fn publish_release(
    path: String,
    target: String,
    changelog: String,
) -> Result<PublishResult, String> {
    let manifest_path = PathBuf::from(&path);
    let manifest = ProjectManifest::load_from_path(&manifest_path).map_err(|e| e.to_string())?;
    let config = get_publish_config(path)?;
    let artifacts = read_artifacts(&manifest_path)?;
    match target.as_str() {
        "github" => publish_github(&manifest, &config, &artifacts, &changelog).await,
        "modrinth" => publish_modrinth(&manifest, &config, &artifacts, &changelog).await,
        "curseforge" => publish_curseforge(&manifest, &config, &artifacts, &changelog).await,
        _ => Err(format!("unknown publish target: {target}")),
    }
}

async fn publish_github(
    manifest: &ProjectManifest,
    config: &PublishConfig,
    artifacts: &[ArtifactRecord],
    changelog: &str,
) -> Result<PublishResult, String> {
    let token = secret("github")?;
    let repository = config.github_repository.trim();
    if repository.split('/').count() != 2 {
        return Err("configure a GitHub repository as owner/repository".to_string());
    }
    let client = reqwest::Client::new();
    let create_url = format!("https://api.github.com/repos/{repository}/releases");
    let tag = format!("v{}", manifest.project.version);
    let payload = json!({
        "tag_name": tag,
        "name": format!("{} {}", manifest.project.name, manifest.project.version),
        "body": changelog,
        "draft": true,
        "prerelease": manifest.project.version.contains("alpha") || manifest.project.version.contains("beta")
    });
    let response = github_headers(client.post(&create_url), Some(&token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(api_error("GitHub", status, &body));
    }
    let release_id = body
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| "GitHub response did not include a release id".to_string())?;
    let mut uploaded_files = Vec::new();
    for artifact in artifacts {
        let file_path = Path::new(&artifact.path);
        if !file_path.is_file() {
            continue;
        }
        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("invalid artifact filename: {}", artifact.path))?;
        let bytes = fs::read(file_path).map_err(|e| e.to_string())?;
        let upload_url =
            format!("https://uploads.github.com/repos/{repository}/releases/{release_id}/assets");
        let upload = github_headers(
            client
                .post(upload_url)
                .query(&[("name", file_name)])
                .header("Content-Type", "application/octet-stream")
                .body(bytes),
            Some(&token),
        )
        .send()
        .await
        .map_err(|e| e.to_string())?;
        if !upload.status().is_success() {
            let upload_status = upload.status();
            let upload_body: Value = upload.json().await.unwrap_or_default();
            return Err(api_error(
                "GitHub asset upload",
                upload_status,
                &upload_body,
            ));
        }
        uploaded_files.push(file_name.to_string());
    }
    Ok(PublishResult {
        target: "github".to_string(),
        id: release_id.to_string(),
        url: body
            .get("html_url")
            .and_then(Value::as_str)
            .map(str::to_string),
        uploaded_files,
    })
}

async fn publish_modrinth(
    manifest: &ProjectManifest,
    config: &PublishConfig,
    artifacts: &[ArtifactRecord],
    changelog: &str,
) -> Result<PublishResult, String> {
    let token = secret("modrinth")?;
    let project_id = config.modrinth_project_id.trim();
    if project_id.is_empty() {
        return Err("configure a Modrinth project id or slug".to_string());
    }
    let artifact = latest_artifact(artifacts, "mrpack")?;
    let file_name = Path::new(&artifact.path)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "invalid Modrinth artifact filename".to_string())?
        .to_string();
    let loader = tuffbox_core::graph::loader_kind_slug(&manifest.loader.kind);
    let metadata = json!({
        "name": format!("{} {}", manifest.project.name, manifest.project.version),
        "version_number": manifest.project.version,
        "changelog": changelog,
        "dependencies": [],
        "game_versions": [manifest.minecraft.version],
        "version_type": if manifest.project.version.contains("alpha") { "alpha" } else if manifest.project.version.contains("beta") { "beta" } else { "release" },
        "loaders": [loader],
        "featured": false,
        "project_id": project_id,
        "file_parts": ["file"],
        "primary_file": "file"
    });
    let bytes = fs::read(&artifact.path).map_err(|e| e.to_string())?;
    let form = reqwest::multipart::Form::new()
        .text("data", metadata.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(bytes).file_name(file_name.clone()),
        );
    let response = reqwest::Client::new()
        .post("https://api.modrinth.com/v2/version")
        .header(USER_AGENT, APP_USER_AGENT)
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(api_error("Modrinth", status, &body));
    }
    let id = body
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(PublishResult {
        target: "modrinth".to_string(),
        id,
        url: Some(format!(
            "https://modrinth.com/modpack/{project_id}/versions"
        )),
        uploaded_files: vec![file_name],
    })
}

async fn publish_curseforge(
    manifest: &ProjectManifest,
    config: &PublishConfig,
    artifacts: &[ArtifactRecord],
    changelog: &str,
) -> Result<PublishResult, String> {
    let token = secret("curseforge")?;
    let project_id = config.curseforge_project_id.trim();
    if project_id.is_empty() {
        return Err("configure a CurseForge project id".to_string());
    }
    if config.curseforge_game_version_ids.is_empty() {
        return Err("configure at least one CurseForge game version id".to_string());
    }
    let artifact = latest_artifact(artifacts, "curseforge")?;
    let file_name = Path::new(&artifact.path)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "invalid CurseForge artifact filename".to_string())?
        .to_string();
    let metadata = json!({
        "changelog": changelog,
        "changelogType": "markdown",
        "displayName": format!("{} {}", manifest.project.name, manifest.project.version),
        "gameVersions": config.curseforge_game_version_ids,
        "releaseType": if manifest.project.version.contains("alpha") { "alpha" } else if manifest.project.version.contains("beta") { "beta" } else { "release" }
    });
    let bytes = fs::read(&artifact.path).map_err(|e| e.to_string())?;
    let form = reqwest::multipart::Form::new()
        .text("metadata", metadata.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(bytes).file_name(file_name.clone()),
        );
    let response = reqwest::Client::new()
        .post(format!(
            "https://minecraft.curseforge.com/api/projects/{project_id}/upload-file"
        ))
        .header(USER_AGENT, APP_USER_AGENT)
        .header("X-Api-Token", token)
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or_default();
    if !status.is_success() {
        return Err(api_error("CurseForge", status, &body));
    }
    let id = body
        .get("id")
        .or_else(|| body.get("data").and_then(|data| data.get("id")))
        .and_then(Value::as_u64)
        .map(|value| value.to_string())
        .unwrap_or_default();
    Ok(PublishResult {
        target: "curseforge".to_string(),
        id,
        url: Some(format!(
            "https://www.curseforge.com/minecraft/modpacks/{project_id}/files"
        )),
        uploaded_files: vec![file_name],
    })
}

fn api_error(service: &str, status: reqwest::StatusCode, body: &Value) -> String {
    let message = body
        .get("message")
        .or_else(|| body.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("request rejected");
    format!("{service} returned {status}: {message}")
}

fn openai_chat_url(endpoint: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    if endpoint.ends_with("/chat/completions") {
        endpoint.to_string()
    } else if endpoint.ends_with("/v1") {
        format!("{endpoint}/chat/completions")
    } else {
        format!("{endpoint}/v1/chat/completions")
    }
}

fn ollama_chat_url(endpoint: &str) -> String {
    let endpoint = endpoint.trim_end_matches('/');
    if endpoint.ends_with("/api/chat") {
        endpoint.to_string()
    } else {
        format!("{endpoint}/api/chat")
    }
}

fn ollama_root(endpoint: &str) -> String {
    endpoint
        .trim()
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .trim_end_matches("/api/chat")
        .trim_end_matches("/api/tags")
        .trim_end_matches("/api/pull")
        .to_string()
}

fn model_name_matches(installed: &str, wanted: &str) -> bool {
    let a = installed.trim().to_lowercase();
    let b = wanted.trim().to_lowercase();
    if a == b {
        return true;
    }
    // `llama3.2:3b` matches `llama3.2:3b-instruct-q4_K_M` or tags without digest
    let a_base = a.split(':').next().unwrap_or(&a);
    let b_base = b.split(':').next().unwrap_or(&b);
    if a_base == b_base {
        // Same family: accept if tags equal or one is latest / missing tag
        let a_tag = a.split_once(':').map(|(_, t)| t).unwrap_or("latest");
        let b_tag = b.split_once(':').map(|(_, t)| t).unwrap_or("latest");
        return a_tag == b_tag
            || a_tag.starts_with(b_tag)
            || b_tag.starts_with(a_tag)
            || a_tag == "latest"
            || b_tag == "latest";
    }
    false
}

async fn ollama_list_models(root: &str) -> Result<Vec<String>, String> {
    let url = format!("{root}/api/tags");
    let response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama at {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Ollama /api/tags failed ({})", response.status()));
    }
    let body: Value = response.json().await.map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    if let Some(models) = body.get("models").and_then(Value::as_array) {
        for m in models {
            if let Some(name) = m.get("name").and_then(Value::as_str) {
                names.push(name.to_string());
            }
        }
    }
    Ok(names)
}

fn try_start_ollama(binary_hint: &str, models_path: &str) {
    try_start_ollama_on_host(binary_hint, models_path, None);
}

fn try_start_ollama_on_host(binary_hint: &str, models_path: &str, host: Option<&str>) {
    let exe = resolve_ollama_binary(binary_hint);
    let models = models_path.trim();
    if !models.is_empty() {
        let _ = fs::create_dir_all(models);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("serve")
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if !models.is_empty() {
            cmd.env("OLLAMA_MODELS", models);
        }
        if let Some(h) = host.filter(|s| !s.trim().is_empty()) {
            cmd.env("OLLAMA_HOST", h);
        }
        let _ = cmd.spawn();
    }
    #[cfg(not(windows))]
    {
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("serve")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if !models.is_empty() {
            cmd.env("OLLAMA_MODELS", models);
        }
        if let Some(h) = host.filter(|s| !s.trim().is_empty()) {
            cmd.env("OLLAMA_HOST", h);
        }
        let _ = cmd.spawn();
    }
}

/// Stop existing Ollama processes so a restart can honor `OLLAMA_MODELS`.
/// Without this, pulls hit an already-running daemon that still uses C:\…\.ollama.
fn stop_ollama_processes() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Broad kill: tray (`ollama app`), CLI/server (`ollama`), capitalized variants.
        for image in ["ollama.exe", "ollama app.exe", "Ollama.exe"] {
            let _ = std::process::Command::new("taskkill")
                .args(["/IM", image, "/F", "/T"])
                .creation_flags(0x08000000)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        let ps = r#"
Get-Process -ErrorAction SilentlyContinue |
  Where-Object { $_.ProcessName -match 'ollama' } |
  Stop-Process -Force -ErrorAction SilentlyContinue
Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
  Where-Object {
    $_.Name -match 'ollama' -or
    ($_.CommandLine -and $_.CommandLine -match '(?i)ollama')
  } |
  ForEach-Object {
    Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue
  }
"#;
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", ps])
            .creation_flags(0x08000000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-f", "ollama serve"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = std::process::Command::new("pkill")
            .args(["-x", "ollama"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

/// Rough inventory of Ollama storage under a models dir (blobs + manifests).
fn ollama_storage_stats(models_path: &Path) -> (u64, u64) {
    fn walk_sum(dir: &Path, files: &mut u64, bytes: &mut u64) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if meta.is_dir() {
                walk_sum(&path, files, bytes);
            } else if meta.is_file() {
                *files += 1;
                *bytes = bytes.saturating_add(meta.len());
            }
        }
    }
    let mut files: u64 = 0;
    let mut bytes: u64 = 0;
    for sub in ["blobs", "manifests"] {
        walk_sum(&models_path.join(sub), &mut files, &mut bytes);
    }
    (files, bytes)
}

fn default_ollama_home_models() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(home) = std::env::var("USERPROFILE") {
            return PathBuf::from(home).join(".ollama").join("models");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".ollama").join("models");
    }
    PathBuf::from(".ollama").join("models")
}

/// After pull: ensure weights actually landed under the configured folder
/// (daemon env is what matters — CLI `OLLAMA_MODELS` alone does not).
fn verify_pull_landed_in_path(
    models_path: &str,
    before: (u64, u64),
    model: &str,
) -> Result<(), String> {
    let dir = PathBuf::from(models_path.trim());
    let after = ollama_storage_stats(&dir);
    if after.0 > before.0 || after.1 > before.1 {
        return Ok(());
    }
    // Manifests may already exist for a re-pull; accept if model tag folder is present.
    let slug = model.trim().to_lowercase();
    let (name, tag) = match slug.split_once(':') {
        Some((n, t)) => (n.to_string(), t.to_string()),
        None => (slug.clone(), "latest".into()),
    };
    let manifest_candidates = [
        dir.join("manifests")
            .join("registry.ollama.ai")
            .join("library")
            .join(&name)
            .join(&tag),
        dir.join("manifests").join("library").join(&name).join(&tag),
    ];
    if manifest_candidates.iter().any(|p| p.is_file()) {
        return Ok(());
    }

    let default_dir = default_ollama_home_models();
    let default_stats = ollama_storage_stats(&default_dir);
    let mut hint = format!(
        "Pull reported success but nothing new appeared under '{models_path}'. \
         Ollama stores models via the *daemon* (`OLLAMA_MODELS`); the tray app may still be writing to the default folder."
    );
    if default_dir != dir && (default_stats.0 > 0 || default_stats.1 > 0) {
        hint.push_str(&format!(
            " Default location still has data: {}.",
            default_dir.display()
        ));
    }
    hint.push_str(
        " Fully Quit Ollama from the system tray (right-click → Quit), then click Install model again.",
    );
    Err(hint)
}

async fn wait_ollama_api(root: &str, attempts: u32) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 0..attempts {
        let delay_ms = 400u64 + u64::from(attempt) * 200;
        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        match ollama_list_models(root).await {
            Ok(_) => return Ok(()),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

/// Start a managed daemon on `MANAGED_OLLAMA_HOST` with `OLLAMA_MODELS` set.
/// Prefer leaving the tray on :11434 alone during a long download; if the managed
/// instance cannot bind (single-instance Ollama), fall back to a full bounce.
async fn ensure_managed_pull_daemon(settings: &AiSettings) -> Result<String, String> {
    let models = settings.ollama_models_path.trim();
    if models.is_empty() {
        return Err("models path is empty".into());
    }
    let _ = fs::create_dir_all(models);
    persist_ollama_models_user_env(models);

    let root = managed_ollama_root();
    let binary = settings.ollama_binary_path.as_str();

    if ollama_list_models(&root).await.is_ok() {
        stop_ollama_processes();
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    }
    try_start_ollama_on_host(binary, models, Some(MANAGED_OLLAMA_HOST));

    let mut last_err = String::new();
    for attempt in 0..12 {
        tokio::time::sleep(std::time::Duration::from_millis(450 + attempt * 200)).await;
        match ollama_list_models(&root).await {
            Ok(_) => return Ok(root),
            Err(e) => last_err = e,
        }
        if attempt == 2 || attempt == 6 || attempt == 9 {
            // Single-instance installs often refuse a second serve — kill tray and retry.
            stop_ollama_processes();
            tokio::time::sleep(std::time::Duration::from_millis(900)).await;
            try_start_ollama_on_host(binary, models, Some(MANAGED_OLLAMA_HOST));
        }
    }
    Err(format!(
        "Could not start a managed Ollama on {MANAGED_OLLAMA_HOST} with models path '{models}'. {last_err}. \
         Fully Quit Ollama from the tray and try Install again."
    ))
}

/// After a custom-path pull, restart the user's usual endpoint daemon with `OLLAMA_MODELS`
/// so chat/detect on :11434 see the same library.
async fn relaunch_user_endpoint_daemon(settings: &AiSettings) -> Result<String, String> {
    let root = ollama_root(&settings.endpoint);
    let models = settings.ollama_models_path.trim();
    persist_ollama_models_user_env(models);
    stop_ollama_processes();
    tokio::time::sleep(std::time::Duration::from_millis(900)).await;
    try_start_ollama(&settings.ollama_binary_path, models);
    match wait_ollama_api(&root, 12).await {
        Ok(()) => Ok(root),
        Err(e) => {
            // Soft failure: models are on disk; user can open Ollama tray (now with User env).
            Err(format!(
                "Models were saved under '{models}', but Ollama did not come back on {root}: {e}. \
                 Open the Ollama app once (it should pick up OLLAMA_MODELS), then Re-detect."
            ))
        }
    }
}

/// Ensure a daemon is running for the user's configured endpoint.
/// Custom models path: prefer starting with `OLLAMA_MODELS` if the API is down.
/// (Pulls use `ensure_managed_pull_daemon` so they do not race the tray on :11434.)
async fn ensure_ollama_daemon(settings: &AiSettings) -> Result<String, String> {
    let root = ollama_root(&settings.endpoint);
    let models = settings.ollama_models_path.trim();
    if models.is_empty() {
        if ollama_list_models(&root).await.is_err() {
            try_start_ollama(&settings.ollama_binary_path, "");
            tokio::time::sleep(std::time::Duration::from_millis(900)).await;
        }
        return Ok(root);
    }

    let _ = fs::create_dir_all(models);
    persist_ollama_models_user_env(models);
    if ollama_list_models(&root).await.is_ok() {
        return Ok(root);
    }
    try_start_ollama(&settings.ollama_binary_path, models);
    let _ = wait_ollama_api(&root, 10).await;
    Ok(root)
}

async fn ollama_pull_model_cli(
    binary_hint: &str,
    models_path: &str,
    model: &str,
    host: Option<&str>,
) -> Result<(), String> {
    let exe = resolve_ollama_binary(binary_hint);
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("pull").arg(model);
    let models = models_path.trim();
    if !models.is_empty() {
        let _ = fs::create_dir_all(models);
        cmd.env("OLLAMA_MODELS", models);
    }
    if let Some(h) = host.filter(|s| !s.trim().is_empty()) {
        cmd.env("OLLAMA_HOST", h);
    }
    #[cfg(windows)]
    {
        cmd.creation_flags(0x08000000);
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ollama pull: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "ollama pull failed: {}",
            if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            }
        ));
    }
    Ok(())
}

/// Default Ollama models directory when `OLLAMA_MODELS` is unset.
pub fn default_ollama_models_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("OLLAMA_MODELS") {
        let p = PathBuf::from(custom.trim());
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    #[cfg(windows)]
    {
        if let Ok(home) = std::env::var("USERPROFILE") {
            return PathBuf::from(home).join(".ollama").join("models");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".ollama").join("models");
    }
    PathBuf::from(".ollama").join("models")
}

/// Resolve a user-configured Ollama path to an executable.
/// Accepts empty (PATH), a file path, or an install directory.
pub fn resolve_ollama_binary(hint: &str) -> PathBuf {
    let trimmed = hint.trim();
    if trimmed.is_empty() {
        return default_ollama_binary_candidates()
            .into_iter()
            .find(|p| p.is_file())
            .unwrap_or_else(|| PathBuf::from("ollama"));
    }
    let path = PathBuf::from(trimmed);
    if path.is_file() {
        return path;
    }
    if path.is_dir() {
        #[cfg(windows)]
        let candidates = ["ollama.exe", "ollama"];
        #[cfg(not(windows))]
        let candidates = ["ollama", "bin/ollama"];
        for name in candidates {
            let candidate = path.join(name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    // Path might not exist yet / be a custom name — still try it as-is.
    path
}

fn default_ollama_binary_candidates() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            out.push(PathBuf::from(local).join("Programs").join("Ollama").join("ollama.exe"));
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            out.push(PathBuf::from(pf).join("Ollama").join("ollama.exe"));
        }
        out.push(PathBuf::from(r"C:\Program Files\Ollama\ollama.exe"));
    }
    #[cfg(target_os = "macos")]
    {
        out.push(PathBuf::from("/usr/local/bin/ollama"));
        out.push(PathBuf::from("/opt/homebrew/bin/ollama"));
        out.push(PathBuf::from("/Applications/Ollama.app/Contents/Resources/ollama"));
    }
    #[cfg(target_os = "linux")]
    {
        out.push(PathBuf::from("/usr/local/bin/ollama"));
        out.push(PathBuf::from("/usr/bin/ollama"));
        if let Ok(home) = std::env::var("HOME") {
            out.push(PathBuf::from(home).join(".local/bin/ollama"));
        }
    }
    out
}

async fn ollama_pull_model(root: &str, model: &str) -> Result<(), String> {
    let url = format!("{root}/api/pull");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60 * 45))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .post(&url)
        .json(&json!({ "name": model, "stream": false }))
        .send()
        .await
        .map_err(|e| format!("Ollama pull failed for '{model}': {e}"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .unwrap_or_else(|_| json!({ "error": "empty response" }));
    if !status.is_success() {
        return Err(api_error("Ollama pull", status, &body));
    }
    if let Some(err) = body.get("error").and_then(Value::as_str) {
        if !err.is_empty() {
            return Err(format!("Ollama pull error: {err}"));
        }
    }
    Ok(())
}

/// Ensure Ollama is reachable and the configured model is already installed.
/// Does **not** auto-pull — the user must pick/install a model in Settings.
/// Returns the model name that should be used for the next request.
pub async fn ensure_ollama_ready(settings: &AiSettings) -> Result<String, String> {
    let root = ollama_root(&settings.endpoint);
    if root.is_empty() {
        return Err("Ollama endpoint is empty".into());
    }

    let mut last_err = String::new();
    let mut models = None;
    // Prefer daemon with configured models dir so later ops don't hit C:\ default.
    if !settings.ollama_models_path.trim().is_empty() {
        let _ = ensure_ollama_daemon(settings).await;
    }
    for attempt in 0..4 {
        match ollama_list_models(&root).await {
            Ok(list) => {
                models = Some(list);
                break;
            }
            Err(e) => {
                last_err = e;
                if attempt == 0 {
                    try_start_ollama(&settings.ollama_binary_path, &settings.ollama_models_path);
                }
                tokio::time::sleep(std::time::Duration::from_millis(700 + attempt * 400)).await;
            }
        }
    }
    let installed = models.ok_or_else(|| {
        let hint_path = settings.ollama_binary_path.trim();
        let resolved = resolve_ollama_binary(hint_path);
        if !ollama_binary_exists(hint_path) {
            format!(
                "{last_err}. Ollama is not installed (or not found). Install from https://ollama.com, then set the path in Settings → AI."
            )
        } else if hint_path.is_empty() {
            format!(
                "{last_err}. Ollama was found at {} but is not responding. Open the Ollama app once, or set the binary path in Settings → AI.",
                resolved.display()
            )
        } else {
            format!(
                "{last_err}. Could not start Ollama from '{}'. Check Settings → AI → Ollama path (resolved to {}).",
                hint_path,
                resolved.display()
            )
        }
    })?;

    let wanted_raw = settings.model.trim();
    if wanted_raw.is_empty() {
        if let Some(first) = installed.first() {
            return Ok(first.clone());
        }
        return Err(
            "Ollama is running but no model is installed. Open Settings → AI, enter a model name (or pick a .gguf file), and click Install model."
                .into(),
        );
    }

    let has = |list: &[String], name: &str| list.iter().any(|m| model_name_matches(m, name));
    if has(&installed, wanted_raw) {
        return Ok(wanted_raw.to_string());
    }
    if let Some(local) = installed
        .iter()
        .find(|m| model_name_matches(m, wanted_raw))
        .cloned()
    {
        return Ok(local);
    }

    Err(format!(
        "Model '{wanted_raw}' is not installed in Ollama. Open Settings → AI, enter the model name you want (e.g. qwen2.5:7b), and click Install model. Installed: {}.",
        if installed.is_empty() {
            "none".into()
        } else {
            installed.join(", ")
        }
    ))
}

fn ollama_binary_exists(hint: &str) -> bool {
    let resolved = resolve_ollama_binary(hint);
    if resolved.is_file() {
        return true;
    }
    // PATH lookup: try `ollama --version` quickly
    std::process::Command::new(if hint.trim().is_empty() {
        "ollama"
    } else {
        resolved.to_str().unwrap_or("ollama")
    })
    .arg("--version")
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false)
}

/// Probe whether Ollama is installed / running and list local models.
#[tauri::command(rename_all = "camelCase")]
pub async fn detect_ollama(
    endpoint: Option<String>,
    binary_path: Option<String>,
) -> Result<serde_json::Value, String> {
    let settings = read_settings();
    let hint = binary_path
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| settings.ai.ollama_binary_path.clone());
    let models_dir = {
        let custom = settings.ai.ollama_models_path.trim();
        if custom.is_empty() {
            default_ollama_models_dir()
        } else {
            PathBuf::from(custom)
        }
    };
    let resolved = resolve_ollama_binary(&hint);
    let installed_bin = ollama_binary_exists(&hint);
    let root = ollama_root(
        &endpoint
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| settings.ai.endpoint.clone()),
    );

    let mut running = false;
    let mut models = Vec::new();
    let mut api_error = None;
    match ollama_list_models(&root).await {
        Ok(list) => {
            running = true;
            models = list;
        }
        Err(e) => {
            api_error = Some(e);
            if installed_bin {
                try_start_ollama(&hint, &settings.ai.ollama_models_path);
                tokio::time::sleep(std::time::Duration::from_millis(900)).await;
                match ollama_list_models(&root).await {
                    Ok(list) => {
                        running = true;
                        models = list;
                        api_error = None;
                    }
                    Err(e2) => api_error = Some(e2),
                }
            }
        }
    }

    Ok(json!({
        "installed": installed_bin || running,
        "running": running,
        "binaryPath": if resolved.exists() { resolved.to_string_lossy().to_string() } else { String::new() },
        "modelsPath": models_dir.to_string_lossy().to_string(),
        "modelsPathConfigured": !settings.ai.ollama_models_path.trim().is_empty(),
        "defaultModel": DEFAULT_OLLAMA_MODEL,
        "endpoint": root,
        "models": models,
        "needsModel": running && models.is_empty(),
        "error": api_error,
        "suggestedModels": [
            DEFAULT_OLLAMA_MODEL,
            "llama3.1:8b",
            "llama3.2:3b",
            "phi3:mini"
        ],
        "suggestedModelNotes": {
            "qwen2.5:7b": "Default — better crash plans",
            "llama3.1:8b": "Strong alternative",
            "llama3.2:3b": "Fast / weaker plans",
            "phi3:mini": "Fast / weaker plans"
        },
    }))
}

/// Explicitly pull a model the user chose (by Ollama tag name).
#[tauri::command(rename_all = "camelCase")]
pub async fn pull_ollama_model(
    model: String,
    endpoint: Option<String>,
    binary_path: Option<String>,
    models_path: Option<String>,
) -> Result<serde_json::Value, String> {
    let name = model.trim().to_string();
    if name.is_empty() {
        return Err("Enter a model name to install (e.g. qwen2.5:7b)".into());
    }
    // Reject obvious filesystem paths here — use import_ollama_gguf instead.
    if name.contains('\\') || name.contains('/') || name.ends_with(".gguf") {
        return Err(
            "That looks like a file path. Use “Import .gguf” for local model files, or enter an Ollama tag like qwen2.5:7b."
                .into(),
        );
    }

    let settings = read_settings();
    let hint = binary_path.unwrap_or_else(|| settings.ai.ollama_binary_path.clone());
    let root_user = ollama_root(
        &endpoint
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| settings.ai.endpoint.clone()),
    );

    let mut ai = settings.ai.clone();
    if !root_user.is_empty() {
        ai.endpoint = root_user.clone();
    }
    if !hint.trim().is_empty() {
        ai.ollama_binary_path = hint.clone();
    }
    // Prefer path from the UI (may not be flushed to disk yet).
    if let Some(p) = models_path {
        ai.ollama_models_path = p;
    }
    let models_path = ai.ollama_models_path.trim().to_string();
    if !models_path.is_empty() {
        let _ = fs::create_dir_all(&models_path);
        // Persist into settings so later chat/detect use the same folder.
        let mut disk = read_settings();
        disk.ai.ollama_models_path = models_path.clone();
        let _ = write_settings(&disk);
    }

    let before = if models_path.is_empty() {
        (0, 0)
    } else {
        ollama_storage_stats(Path::new(&models_path))
    };

    // Custom path: pull against a private managed daemon so the tray on :11434
    // cannot swallow the download into %USERPROFILE%\.ollama\models.
    let pull_root = if models_path.is_empty() {
        ensure_ollama_daemon(&ai).await?
    } else {
        ensure_managed_pull_daemon(&ai).await?
    };

    let pull_host = pull_root
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    // Prefer HTTP pull against the daemon that has OLLAMA_MODELS; CLI is a backup.
    match ollama_pull_model(&pull_root, &name).await {
        Ok(()) => {}
        Err(api_err) => {
            if let Err(cli_err) =
                ollama_pull_model_cli(&hint, &models_path, &name, Some(pull_host)).await
            {
                return Err(format!("{api_err} | CLI fallback: {cli_err}"));
            }
        }
    }

    if !models_path.is_empty() {
        verify_pull_landed_in_path(&models_path, before, &name)?;
        // Bring :11434 (or user endpoint) up on the same models dir for chat.
        if let Err(relaunch_err) = relaunch_user_endpoint_daemon(&ai).await {
            // Soft: weights are verified on disk.
            eprintln!("[tuffbox] ollama relaunch after pull: {relaunch_err}");
        }
    }

    let list_root = ollama_root(&ai.endpoint);
    let models = match ollama_list_models(&list_root).await {
        Ok(m) => m,
        Err(_) => ollama_list_models(&pull_root).await.unwrap_or_default(),
    };

    // Persist as active model when pull succeeds.
    let mut next = read_settings();
    next.ai.provider = "ollama".into();
    next.ai.model = name.clone();
    if !models_path.is_empty() {
        next.ai.ollama_models_path = models_path.clone();
    }
    if next.ai.endpoint.trim().is_empty() {
        next.ai.endpoint = list_root.clone();
    }
    let _ = write_settings(&next);

    Ok(json!({
        "ok": true,
        "model": name,
        "models": models,
        "modelsPath": if models_path.is_empty() {
            default_ollama_models_dir().to_string_lossy().to_string()
        } else {
            models_path
        },
    }))
}

/// Import a local GGUF (or similar) file into Ollama under a user-chosen name.
#[tauri::command(rename_all = "camelCase")]
pub async fn import_ollama_gguf(
    file_path: String,
    model_name: String,
    binary_path: Option<String>,
) -> Result<serde_json::Value, String> {
    let path = PathBuf::from(file_path.trim());
    if !path.is_file() {
        return Err(format!("Model file not found: {}", path.display()));
    }
    let mut name = model_name.trim().to_string();
    if name.is_empty() {
        name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("local-model")
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' })
            .collect();
    }
    if name.is_empty() {
        return Err("Enter a name for the imported model".into());
    }

    let settings = read_settings();
    let hint = binary_path.unwrap_or_else(|| settings.ai.ollama_binary_path.clone());
    let exe = resolve_ollama_binary(&hint);
    if !ollama_binary_exists(&hint) {
        return Err("Ollama is not installed. Install it first, then import the model.".into());
    }

    let abs = std::fs::canonicalize(&path).unwrap_or(path.clone());
    let from_line = format!("FROM {}", abs.display());
    let tmp = std::env::temp_dir().join(format!("tuffbox-modelfile-{name}"));
    fs::write(&tmp, format!("{from_line}\n")).map_err(|e| e.to_string())?;

    // Bounce onto configured models path before create (managed port when custom).
    let mut ai = settings.ai.clone();
    if !hint.trim().is_empty() {
        ai.ollama_binary_path = hint.clone();
    }
    let models_dir = ai.ollama_models_path.trim().to_string();
    let before = if models_dir.is_empty() {
        (0, 0)
    } else {
        ollama_storage_stats(Path::new(&models_dir))
    };
    let create_root = if models_dir.is_empty() {
        ensure_ollama_daemon(&ai).await?
    } else {
        ensure_managed_pull_daemon(&ai).await?
    };
    let create_host = create_root
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    let mut cmd = tokio::process::Command::new(&exe);
    cmd.arg("create").arg(&name).arg("-f").arg(&tmp);
    if !models_dir.is_empty() {
        cmd.env("OLLAMA_MODELS", &models_dir);
    }
    cmd.env("OLLAMA_HOST", create_host);
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to run ollama create: {e}"))?;
    let _ = fs::remove_file(&tmp);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "ollama create failed: {}",
            if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            }
        ));
    }

    if !models_dir.is_empty() {
        verify_pull_landed_in_path(&models_dir, before, &name)?;
        let _ = relaunch_user_endpoint_daemon(&ai).await;
    }

    let mut next = read_settings();
    next.ai.provider = "ollama".into();
    next.ai.model = name.clone();
    let _ = write_settings(&next);

    let root = ollama_root(&next.ai.endpoint);
    let models = ollama_list_models(&root).await.unwrap_or_default();
    Ok(json!({
        "ok": true,
        "model": name,
        "models": models,
        "modelsPath": if models_dir.is_empty() {
            default_ollama_models_dir().to_string_lossy().to_string()
        } else {
            models_dir
        },
    }))
}

/// Status helper for Settings / Diagnostics (does not pull).
#[tauri::command(rename_all = "camelCase")]
pub async fn ensure_ollama_model() -> Result<serde_json::Value, String> {
    let settings = read_settings();
    if settings.ai.provider != "ollama" {
        return Ok(json!({
            "ok": true,
            "provider": settings.ai.provider,
            "skipped": true,
        }));
    }
    let model = ensure_ollama_ready(&settings.ai).await?;
    Ok(json!({
        "ok": true,
        "provider": "ollama",
        "model": model,
        "endpoint": ollama_root(&read_settings().ai.endpoint),
    }))
}

pub async fn call_ai(settings: &AiSettings, prompt: &str) -> Result<Value, String> {
    call_ai_once(settings, prompt).await
}

/// Multi-turn chat with a custom system prompt (no crash ActionPlan baked in).
/// When `json_mode` is true, providers are asked for JSON and the content is parsed.
pub async fn call_ai_messages(
    settings: &AiSettings,
    system: &str,
    messages: &[Value],
    json_mode: bool,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let mut api_messages = Vec::with_capacity(messages.len() + 1);
    api_messages.push(json!({"role": "system", "content": system}));
    for m in messages {
        api_messages.push(m.clone());
    }

    let content = if settings.provider == "ollama" {
        let model = ensure_ollama_ready(settings).await?;
        let mut body = json!({
            "model": model,
            "stream": false,
            "messages": api_messages,
        });
        if json_mode {
            body["format"] = json!("json");
        }
        let response = client
            .post(ollama_chat_url(&settings.endpoint))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Ollama request failed (is Ollama running?): {e}"))?;
        let status = response.status();
        let body_text = response.text().await.map_err(|e| e.to_string())?;
        let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| {
            json!({ "error": body_text.chars().take(500).collect::<String>() })
        });
        if !status.is_success() {
            return Err(api_error("Ollama", status, &body));
        }
        body.get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| "Ollama response did not contain message.content".to_string())?
            .to_string()
    } else {
        let token = secret("ai").ok();
        let mut payload = json!({
            "model": settings.model,
            "temperature": 0.3,
            "messages": api_messages,
        });
        if json_mode {
            payload["response_format"] = json!({"type": "json_object"});
        }
        let mut req = client
            .post(openai_chat_url(&settings.endpoint))
            .header(USER_AGENT, APP_USER_AGENT)
            .json(&payload);
        if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
            req = req.bearer_auth(token);
        }
        let response = req
            .send()
            .await
            .map_err(|e| format!("AI request failed: {e}"))?;
        let status = response.status();
        let body_text = response.text().await.map_err(|e| e.to_string())?;
        let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| {
            json!({ "error": body_text.chars().take(500).collect::<String>() })
        });
        if !status.is_success() {
            return Err(api_error("AI provider", status, &body));
        }
        body.get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| "AI response did not contain choices[0].message.content".to_string())?
            .to_string()
    };

    let trimmed = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    if json_mode {
        serde_json::from_str(trimmed)
            .or_else(|_| {
                // Some models wrap JSON in prose; try extracting outermost object.
                if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
                    if end > start {
                        return serde_json::from_str(&trimmed[start..=end]);
                    }
                }
                Err(serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "not an object",
                )))
            })
            .map_err(|e| format!("AI returned invalid JSON: {e}"))
    } else {
        Ok(json!({ "content": trimmed }))
    }
}

/// Call AI and ensure the response parses as ActionPlan; one repair retry on failure.
pub async fn call_ai_crash_explain(settings: &AiSettings, prompt: &str) -> Result<Value, String> {
    match call_ai_once(settings, prompt).await {
        Ok(value) => {
            let raw = serde_json::to_string(&value).unwrap_or_default();
            match tuffbox_core::action_plan::parse_action_plan(&raw) {
                Ok(plan) => Ok(serde_json::to_value(plan).unwrap_or(value)),
                Err(_) => Ok(value),
            }
        }
        Err(first_err) => {
            if !first_err.to_lowercase().contains("invalid json")
                && !first_err.to_lowercase().contains("did not contain")
            {
                return Err(first_err);
            }
            let repair = format!(
                "{prompt}\n\nYour previous answer was invalid JSON ({first_err}).\n{}\nReturn ONLY the JSON object.",
                tuffbox_core::ai_explanation::CRASH_JSON_SCHEMA_HINT
            );
            let value = call_ai_once(settings, &repair).await?;
            let raw = serde_json::to_string(&value).unwrap_or_default();
            let plan = tuffbox_core::action_plan::parse_action_plan(&raw)
                .map_err(|e| format!("AI returned invalid JSON after retry: {e}"))?;
            Ok(serde_json::to_value(plan).map_err(|e| e.to_string())?)
        }
    }
}

async fn call_ai_once(settings: &AiSettings, prompt: &str) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;
    let content = if settings.provider == "ollama" {
        let model = ensure_ollama_ready(settings).await?;
        let response = client
            .post(ollama_chat_url(&settings.endpoint))
            .json(&json!({
                "model": model,
                "stream": false,
                "format": "json",
                "messages": [
                    {"role": "system", "content": format!("{}\n\n{}", tuffbox_core::action_plan::ACTION_PLAN_SYSTEM_PROMPT, tuffbox_core::ai_explanation::CRASH_JSON_SCHEMA_HINT)},
                    {"role": "user", "content": prompt}
                ]
            }))
            .send()
            .await
            .map_err(|e| format!("Ollama request failed (is Ollama running?): {e}"))?;
        let status = response.status();
        let body_text = response.text().await.map_err(|e| e.to_string())?;
        let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| {
            json!({ "error": body_text.chars().take(500).collect::<String>() })
        });
        if !status.is_success() {
            return Err(api_error("Ollama", status, &body));
        }
        body.get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| "Ollama response did not contain message.content".to_string())?
            .to_string()
    } else {
        // OpenAI-compatible / Hermes-style: API key optional for local endpoints.
        let token = secret("ai").ok();
        let mut req = client
            .post(openai_chat_url(&settings.endpoint))
            .header(USER_AGENT, APP_USER_AGENT)
            .json(&json!({
                "model": settings.model,
                "temperature": 0.2,
                "response_format": {"type": "json_object"},
                "messages": [
                    {"role": "system", "content": format!("{}\n\n{}", tuffbox_core::action_plan::ACTION_PLAN_SYSTEM_PROMPT, tuffbox_core::ai_explanation::CRASH_JSON_SCHEMA_HINT)},
                    {"role": "user", "content": prompt}
                ]
            }));
        if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
            req = req.bearer_auth(token);
        }
        let response = req
            .send()
            .await
            .map_err(|e| format!("AI request failed: {e}"))?;
        let status = response.status();
        let body_text = response.text().await.map_err(|e| e.to_string())?;
        let body: Value = serde_json::from_str(&body_text).unwrap_or_else(|_| {
            json!({ "error": body_text.chars().take(500).collect::<String>() })
        });
        if !status.is_success() {
            return Err(api_error("AI provider", status, &body));
        }
        body.get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| "AI response did not contain choices[0].message.content".to_string())?
            .to_string()
    };
    let trimmed = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    // Prefer ActionPlan parse; fall back to raw JSON.
    match tuffbox_core::action_plan::parse_action_plan(trimmed) {
        Ok(plan) => serde_json::to_value(plan).map_err(|e| e.to_string()),
        Err(_) => match tuffbox_core::ai_explanation::parse_crash_response(trimmed) {
            Ok(parsed) => serde_json::to_value(parsed).map_err(|e| e.to_string()),
            Err(_) => {
                serde_json::from_str(trimmed).map_err(|e| format!("AI returned invalid JSON: {e}"))
            }
        },
    }
}

/// List local Ollama model names via `/api/tags`.
#[tauri::command(rename_all = "camelCase")]
pub async fn list_ollama_models(endpoint: Option<String>) -> Result<Vec<String>, String> {
    let settings = read_settings();
    let base = endpoint
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| settings.ai.endpoint.clone());
    let root = ollama_root(&base);
    ollama_list_models(&root).await.map(|mut names| {
        names.sort();
        names
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_provider_urls() {
        assert_eq!(
            openai_chat_url("http://localhost:1234/v1"),
            "http://localhost:1234/v1/chat/completions"
        );
        assert_eq!(
            ollama_chat_url("http://127.0.0.1:11434"),
            "http://127.0.0.1:11434/api/chat"
        );
    }

    #[test]
    fn normalizes_release_versions() {
        assert_eq!(
            normalized_semver("v1.2.3"),
            Some(semver::Version::new(1, 2, 3))
        );
    }
}
