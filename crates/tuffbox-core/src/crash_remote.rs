//! Remote private crash knowledge-base client.
//!
//! The full KB corpus never ships in the launcher. Clients may only:
//! - `lookup` — top-N similar cases for the current fingerprint (local AI RAG)
//! - `diagnose` — server-side plan (KB ± LLM)
//!
//! Offline fallback uses the thin builtin seed in `crash_kb`.

use crate::action_plan::{parse_action_plan_value, ActionPlan, LauncherAction};
use crate::ai_explanation::AiAction;
use crate::crash_kb::{CrashFingerprint, SimilarCaseHit};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const APP_USER_AGENT: &str = "TuffBox-IDE/0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashLookupRequest {
    pub fingerprint: CrashFingerprint,
    #[serde(default)]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub mc_version: Option<String>,
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashLookupHit {
    pub id: String,
    pub score: f64,
    pub solution: String,
    #[serde(default)]
    pub suspected_mods: Vec<String>,
    #[serde(default)]
    pub actions: Vec<LauncherAction>,
    #[serde(default)]
    pub fingerprint_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashLookupResponse {
    #[serde(default)]
    pub kb_version: Option<String>,
    #[serde(default)]
    pub hits: Vec<CrashLookupHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashDiagnoseRequest {
    pub fingerprint: CrashFingerprint,
    #[serde(default)]
    pub context: Option<Value>,
    #[serde(default)]
    pub excerpt: Option<String>,
    /// When true, server may skip LLM if a strong KB match exists.
    #[serde(default)]
    pub prefer_kb_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashDiagnoseResponse {
    pub plan: ActionPlan,
    #[serde(default)]
    pub kb_version: Option<String>,
    #[serde(default)]
    pub used_llm: bool,
}

/// Convert remote lookup hits into the prompt/RAG SimilarCaseHit shape.
pub fn hits_to_similar_cases(hits: &[CrashLookupHit]) -> Vec<SimilarCaseHit> {
    hits.iter()
        .map(|h| SimilarCaseHit {
            id: h.id.clone(),
            score: h.score,
            solution: h.solution.clone(),
            suspected_mods: h.suspected_mods.clone(),
            actions: h
                .actions
                .iter()
                .map(|a| AiAction {
                    action_type: match a.op.as_str() {
                        "install_mod" => "install".into(),
                        "remove_mod" => "remove".into(),
                        "disable_mod" => "disable".into(),
                        "update_mod" | "change_mod_version" | "reinstall_mod" => "update".into(),
                        "edit_config" => "config_change".into(),
                        other => other.into(),
                    },
                    mod_id: a.mod_id.clone(),
                    description: a.reason.clone().unwrap_or_default(),
                    risk: a.risk.clone(),
                })
                .collect(),
            fingerprint_key: h.fingerprint_key.clone(),
            source: "remote".into(),
        })
        .collect()
}

fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{base}/{path}")
}

/// POST /v1/crash/lookup — returns top-N cases for local-AI RAG (no full corpus).
pub fn lookup_remote(
    base_url: &str,
    token: Option<&str>,
    request: &CrashLookupRequest,
) -> Result<CrashLookupResponse, String> {
    if base_url.trim().is_empty() {
        return Err("crash KB endpoint is not configured".into());
    }
    let url = join_url(base_url, "/v1/crash/lookup");
    let client = reqwest::blocking::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.post(&url).json(request);
    if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
        req = req.bearer_auth(token);
    }
    let response = req.send().map_err(|e| format!("crash KB lookup failed: {e}"))?;
    let status = response.status();
    let body: Value = response.json().map_err(|e| e.to_string())?;
    if !status.is_success() {
        let msg = body
            .get("message")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("request rejected");
        return Err(format!("crash KB lookup {status}: {msg}"));
    }
    serde_json::from_value(body).map_err(|e| format!("invalid lookup response: {e}"))
}

/// POST /v1/crash/diagnose — server returns a ready ActionPlan.
pub fn diagnose_remote(
    base_url: &str,
    token: Option<&str>,
    request: &CrashDiagnoseRequest,
) -> Result<CrashDiagnoseResponse, String> {
    if base_url.trim().is_empty() {
        return Err("crash KB endpoint is not configured".into());
    }
    let url = join_url(base_url, "/v1/crash/diagnose");
    let client = reqwest::blocking::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.post(&url).json(request);
    if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
        req = req.bearer_auth(token);
    }
    let response = req
        .send()
        .map_err(|e| format!("crash KB diagnose failed: {e}"))?;
    let status = response.status();
    let body: Value = response.json().map_err(|e| e.to_string())?;
    if !status.is_success() {
        let msg = body
            .get("message")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("request rejected");
        return Err(format!("crash KB diagnose {status}: {msg}"));
    }

    // Accept either { plan: {...} } or a bare ActionPlan object.
    if body.get("plan").is_some() {
        let mut resp: CrashDiagnoseResponse =
            serde_json::from_value(body).map_err(|e| format!("invalid diagnose response: {e}"))?;
        // Re-normalize via parser for legacy fields inside plan.
        if let Ok(normalized) =
            parse_action_plan_value(&serde_json::to_value(&resp.plan).unwrap_or(json!({})))
        {
            resp.plan = normalized;
        }
        Ok(resp)
    } else {
        let plan = parse_action_plan_value(&body)?;
        Ok(CrashDiagnoseResponse {
            plan,
            kb_version: body
                .get("kbVersion")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            used_llm: body
                .get("usedLlm")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        })
    }
}

/// Async wrappers for Tauri (uses reqwest async).
pub async fn lookup_remote_async(
    base_url: &str,
    token: Option<&str>,
    request: &CrashLookupRequest,
) -> Result<CrashLookupResponse, String> {
    if base_url.trim().is_empty() {
        return Err("crash KB endpoint is not configured".into());
    }
    let url = join_url(base_url, "/v1/crash/lookup");
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.post(&url).json(request);
    if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
        req = req.bearer_auth(token);
    }
    let response = req
        .send()
        .await
        .map_err(|e| format!("crash KB lookup failed: {e}"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !status.is_success() {
        let msg = body
            .get("message")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("request rejected");
        return Err(format!("crash KB lookup {status}: {msg}"));
    }
    serde_json::from_value(body).map_err(|e| format!("invalid lookup response: {e}"))
}

pub async fn diagnose_remote_async(
    base_url: &str,
    token: Option<&str>,
    request: &CrashDiagnoseRequest,
) -> Result<CrashDiagnoseResponse, String> {
    if base_url.trim().is_empty() {
        return Err("crash KB endpoint is not configured".into());
    }
    let url = join_url(base_url, "/v1/crash/diagnose");
    let client = reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.post(&url).json(request);
    if let Some(token) = token.filter(|t| !t.trim().is_empty()) {
        req = req.bearer_auth(token);
    }
    let response = req
        .send()
        .await
        .map_err(|e| format!("crash KB diagnose failed: {e}"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| e.to_string())?;
    if !status.is_success() {
        let msg = body
            .get("message")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or("request rejected");
        return Err(format!("crash KB diagnose {status}: {msg}"));
    }
    if body.get("plan").is_some() {
        let mut resp: CrashDiagnoseResponse =
            serde_json::from_value(body).map_err(|e| format!("invalid diagnose response: {e}"))?;
        if let Ok(normalized) =
            parse_action_plan_value(&serde_json::to_value(&resp.plan).unwrap_or(json!({})))
        {
            resp.plan = normalized;
        }
        Ok(resp)
    } else {
        let plan = parse_action_plan_value(&body)?;
        Ok(CrashDiagnoseResponse {
            plan,
            kb_version: body
                .get("kbVersion")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            used_llm: body
                .get("usedLlm")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        })
    }
}
