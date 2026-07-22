use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tuffbox_core::ProjectManifest;

const KEYRING_SERVICE: &str = "dev.tuffbox.ide";
const DEFAULT_GITHUB_REPOSITORY: &str = "MFcrychelt/tuffbox";
const APP_USER_AGENT: &str = "TuffBox-IDE/0.1";

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
}

fn default_diagnose_mode() -> String {
    "server".into()
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            endpoint: "http://127.0.0.1:11434".to_string(),
            model: "qwen2.5-coder:7b".to_string(),
            diagnose_mode: default_diagnose_mode(),
            crash_kb_endpoint: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationSettings {
    pub github_repository: String,
    pub ai: AiSettings,
}

impl Default for IntegrationSettings {
    fn default() -> Self {
        Self {
            github_repository: DEFAULT_GITHUB_REPOSITORY.to_string(),
            ai: AiSettings::default(),
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

fn read_settings() -> IntegrationSettings {
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
        .map_err(|e| e.to_string())
}

fn keyring_entry(kind: &str) -> Result<keyring::Entry, String> {
    let account = match kind {
        "github" => "github-token",
        "modrinth" => "modrinth-token",
        "curseforge" => "curseforge-token",
        "ai" => "ai-api-key",
        "crash_kb" => "crash-kb-token",
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
    }
}

#[tauri::command]
pub fn save_integration_settings(mut settings: IntegrationSettings) -> Result<(), String> {
    if settings.github_repository.split('/').count() != 2 {
        return Err("GitHub repository must use owner/repository format".to_string());
    }
    if settings.ai.endpoint.trim().is_empty() || settings.ai.model.trim().is_empty() {
        return Err("AI endpoint and model are required".to_string());
    }
    if !matches!(
        settings.ai.provider.as_str(),
        "ollama" | "openai-compatible"
    ) {
        return Err("AI provider must be ollama or openai-compatible".to_string());
    }
    let mode = tuffbox_core::action_plan::DiagnoseMode::parse(&settings.ai.diagnose_mode);
    settings.ai.diagnose_mode = mode.as_str().to_string();
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

pub async fn call_ai(settings: &AiSettings, prompt: &str) -> Result<Value, String> {
    call_ai_once(settings, prompt).await
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
    let client = reqwest::Client::new();
    let content = if settings.provider == "ollama" {
        let response = client
            .post(ollama_chat_url(&settings.endpoint))
            .json(&json!({
                "model": settings.model,
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
        let body: Value = response.json().await.map_err(|e| e.to_string())?;
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
        let body: Value = response.json().await.map_err(|e| e.to_string())?;
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
    let root = base
        .trim_end_matches('/')
        .trim_end_matches("/v1")
        .trim_end_matches("/api/chat")
        .trim_end_matches("/api/tags");
    let url = format!("{root}/api/tags");

    let response = reqwest::Client::new()
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
    names.sort();
    Ok(names)
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
