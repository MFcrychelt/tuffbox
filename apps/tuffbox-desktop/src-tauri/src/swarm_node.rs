//! Phase C: spawn/attach `tuffswarm-node` and prefer its control HTTP over hub.

use once_cell::sync::Lazy;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use crate::integrations;

static NODE_CHILD: Lazy<Mutex<Option<Child>>> = Lazy::new(|| Mutex::new(None));
/// Ephemeral bearer for the node we spawned (process memory only).
static CONTROL_TOKEN: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

fn control_token() -> Option<String> {
    CONTROL_TOKEN.lock().ok().and_then(|g| g.clone())
}

fn set_control_token(token: String) {
    if let Ok(mut g) = CONTROL_TOKEN.lock() {
        *g = Some(token);
    }
}

fn is_p2p_control_base(base: &str) -> bool {
    let swarm = integrations::swarm_settings();
    let control = swarm.p2p_control_url.trim().trim_end_matches('/');
    base.trim().trim_end_matches('/') == control
}

/// Token for a transport base: P2P control uses spawn token; hub uses crash_kb secret.
pub fn auth_token_for_base(base: &str) -> Option<String> {
    if is_p2p_control_base(base) {
        control_token()
    } else {
        integrations::secret_optional("crash_kb")
    }
}

fn new_ephemeral_token() -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        format!(
            "tuffswarm-{}-{}",
            std::process::id(),
            tuffbox_core::time_util::compact_now()
        )
        .as_bytes(),
    );
    hex::encode(hasher.finalize())
}

/// Ordered capsule HTTP bases: P2P control (if healthy) then hub/KB fallback.
pub async fn capsule_transport_bases() -> Vec<String> {
    let swarm = integrations::swarm_settings();
    let mut bases = Vec::new();
    if swarm.enabled && swarm.p2p_enabled {
        let control = swarm.p2p_control_url.trim().trim_end_matches('/').to_string();
        if !control.is_empty() {
            let _ = ensure_node_running(&control).await;
            if p2p_healthy(&control).await {
                bases.push(control);
            }
        }
    }
    if let Some(hub) = integrations::swarm_network_base() {
        if !bases.iter().any(|b| b == &hub) {
            bases.push(hub);
        }
    }
    bases
}

pub async fn p2p_healthy(control_base: &str) -> bool {
    let url = format!("{}/health", control_base.trim_end_matches('/'));
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    else {
        return false;
    };
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            body.get("ok").and_then(|v| v.as_bool()).unwrap_or(true)
        }
        _ => false,
    }
}

pub async fn ensure_node_running(control_base: &str) -> Result<(), String> {
    if p2p_healthy(control_base).await {
        // Attached to an already-running node — API calls need a matching token.
        return Ok(());
    }
    {
        let mut guard = NODE_CHILD
            .lock()
            .map_err(|_| "p2p node lock poisoned".to_string())?;
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => {}
                _ => {
                    *guard = None;
                    if let Ok(mut t) = CONTROL_TOKEN.lock() {
                        *t = None;
                    }
                }
            }
        }
        if guard.is_none() {
            let bin = find_node_binary().ok_or_else(|| {
                "tuffswarm-node binary not found — build with `cargo build -p tuffswarm-node` or add it to PATH"
                    .to_string()
            })?;
            let control = control_base
                .trim_start_matches("http://")
                .trim_start_matches("https://")
                .trim_end_matches('/');

            let token = new_ephemeral_token();
            set_control_token(token.clone());

            let mut cmd = Command::new(&bin);
            cmd.arg("--control")
                .arg(control)
                .arg("--control-token")
                .arg(&token)
                .arg("--listen")
                .arg("/ip4/0.0.0.0/tcp/0")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                cmd.creation_flags(CREATE_NO_WINDOW);
            }
            let child = cmd
                .spawn()
                .map_err(|e| format!("failed to spawn {}: {e}", bin.display()))?;
            *guard = Some(child);
        }
    }

    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if p2p_healthy(control_base).await {
            return Ok(());
        }
    }
    Err("tuffswarm-node did not become healthy in time".into())
}

fn find_node_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["tuffswarm-node.exe", "tuffswarm-node"] {
                let cand = dir.join(name);
                if cand.is_file() {
                    return Some(cand);
                }
            }
            for profile in ["debug", "release"] {
                let cand = dir.join("../../../target").join(profile).join(
                    if cfg!(windows) {
                        "tuffswarm-node.exe"
                    } else {
                        "tuffswarm-node"
                    },
                );
                if cand.is_file() {
                    return Some(cand);
                }
            }
            // Workspace cargo target (dev).
            for profile in ["debug", "release"] {
                let cand = dir
                    .join("../../../../target")
                    .join(profile)
                    .join(if cfg!(windows) {
                        "tuffswarm-node.exe"
                    } else {
                        "tuffswarm-node"
                    });
                if cand.is_file() {
                    return Some(cand);
                }
            }
        }
    }
    if let Ok(td) = std::env::var("CARGO_TARGET_DIR") {
        for profile in ["debug", "release"] {
            let cand = PathBuf::from(&td).join(profile).join(if cfg!(windows) {
                "tuffswarm-node.exe"
            } else {
                "tuffswarm-node"
            });
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    which("tuffswarm-node")
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.to_string()
        });
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_p2p_node_status() -> Result<Value, String> {
    let swarm = integrations::swarm_settings();
    if !swarm.enabled || !swarm.p2p_enabled {
        return Ok(serde_json::json!({
            "enabled": false,
            "healthy": false,
            "controlUrl": swarm.p2p_control_url,
        }));
    }
    let base = swarm.p2p_control_url.trim().trim_end_matches('/').to_string();
    let healthy = p2p_healthy(&base).await;
    let mut status = serde_json::json!({
        "enabled": true,
        "healthy": healthy,
        "controlUrl": base,
        "tokenPresent": control_token().is_some(),
    });
    if healthy {
        let url = format!("{base}/v1/node/status");
        if let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
        {
            let mut req = client.get(&url);
            if let Some(token) = control_token() {
                req = req.bearer_auth(token);
            }
            if let Ok(resp) = req.send().await {
                if let Ok(body) = resp.json::<Value>().await {
                    status["node"] = body;
                }
            }
        }
    }
    Ok(status)
}

/// Lookup capsules across P2P then hub; merge hits (first base preferred for order).
pub async fn lookup_across_transports(
    req: &tuffbox_core::crash_remote::CrashLookupRequest,
) -> Option<tuffbox_core::crash_remote::CrashLookupResponse> {
    let bases = capsule_transport_bases().await;
    if bases.is_empty() {
        return None;
    }
    let mut merged: Vec<tuffbox_core::crash_remote::CrashLookupHit> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for base in &bases {
        let token = auth_token_for_base(base);
        if let Ok(resp) =
            tuffbox_core::crash_remote::lookup_remote_async(base, token.as_deref(), req).await
        {
            for hit in resp.hits {
                if seen.insert(hit.id.clone()) {
                    merged.push(hit);
                }
            }
        }
    }
    if merged.is_empty() {
        None
    } else {
        Some(tuffbox_core::crash_remote::CrashLookupResponse {
            hits: merged,
            kb_version: None,
        })
    }
}

/// Try diagnose on each transport (P2P has no diagnose — hub/KB will succeed if present).
pub async fn diagnose_across_transports(
    req: &tuffbox_core::crash_remote::CrashDiagnoseRequest,
) -> Result<tuffbox_core::crash_remote::CrashDiagnoseResponse, String> {
    let bases = capsule_transport_bases().await;
    if bases.is_empty() {
        return Err("no swarm transport (enable P2P node or set hub URL)".into());
    }
    let mut last_err = "diagnose failed on all transports".to_string();
    for base in &bases {
        let token = auth_token_for_base(base);
        match tuffbox_core::crash_remote::diagnose_remote_async(base, token.as_deref(), req).await
        {
            Ok(resp) => return Ok(resp),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn ensure_p2p_node() -> Result<Value, String> {
    integrations::require_swarm_enabled()?;
    let swarm = integrations::set_swarm_p2p(true, None)?;
    let base = swarm.p2p_control_url.trim().trim_end_matches('/').to_string();
    ensure_node_running(&base).await?;
    get_p2p_node_status().await
}
