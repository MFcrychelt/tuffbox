//! HTTP client to the TuffBox launcher overlay IPC proxy.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;

static IPC_BASE: Lazy<Mutex<String>> = Lazy::new(|| {
    let ep = std::env::var("TUFFBOX_OVERLAY_IPC").unwrap_or_else(|_| "127.0.0.1:8799".into());
    let base = if ep.starts_with("http") {
        ep
    } else {
        format!("http://{ep}")
    };
    Mutex::new(base)
});

pub fn log_debug(msg: &str) -> Result<(), String> {
    // Best-effort: also hit /health so we know IPC is up.
    let _ = get_json("/health");
    eprintln!("[tuffbox-overlay] {msg}");
    Ok(())
}

pub fn ipc_base() -> String {
    IPC_BASE.lock().clone()
}

pub fn get_json(path: &str) -> Result<Value, String> {
    let url = format!("{}{}", ipc_base(), path);
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(8))
        .call()
        .map_err(|e| e.to_string())?;
    resp.into_json().map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeedItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub thumbnail_url: String,
    #[serde(default)]
    pub channel: String,
}

pub fn fetch_youtube_feed() -> Vec<FeedItem> {
    match get_json("/youtube-feed") {
        Ok(v) => v
            .get("items")
            .and_then(|i| i.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|row| serde_json::from_value(row.clone()).ok())
                    .collect()
            })
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn fetch_friends() -> Value {
    get_json("/friends").unwrap_or_else(|e| serde_json::json!({ "error": e }))
}

pub fn fetch_chat() -> Value {
    get_json("/chat").unwrap_or_else(|e| serde_json::json!({ "error": e }))
}

pub fn resolve_youtube(id: &str) -> Option<String> {
    let path = format!("/youtube-resolve?id={id}");
    get_json(&path)
        .ok()
        .and_then(|v| v.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()))
}
