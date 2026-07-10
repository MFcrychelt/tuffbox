use std::sync::LazyLock;
use std::time::Duration;

static HTTP: LazyLock<reqwest::blocking::Client> = LazyLock::new(|| {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(15))
        .tcp_keepalive(Duration::from_secs(10))
        .user_agent("TuffBox-IDE/0.1.0")
        .build()
        .expect("Failed to build HTTP client")
});

const MAX_RETRIES: u32 = 3;

fn retryable(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request()
}

fn fetch(url: &str) -> Result<reqwest::blocking::Response, reqwest::Error> {
    let mut last_err = None;
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            std::thread::sleep(Duration::from_secs(2u64.pow(attempt)));
        }
        match HTTP.get(url).send() {
            Ok(resp) => {
                if resp.status().is_server_error() {
                    if attempt < MAX_RETRIES {
                        last_err = Some(resp.error_for_status().unwrap_err());
                        continue;
                    }
                    return resp.error_for_status();
                }
                return Ok(resp);
            }
            Err(e) if retryable(&e) && attempt < MAX_RETRIES => {
                last_err = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.expect("retries exhausted with last_err set"))
}

pub fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, reqwest::Error> {
    fetch(url)?.json()
}

pub fn get_json_with_context<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, String> {
    let response = fetch(url).map_err(|e| format!("HTTP request failed: {}", e))?;
    let status = response.status();
    let body = response.text().map_err(|e| format!("Failed to read response body: {}", e))?;
    
    serde_json::from_str(&body).map_err(|e| {
        let preview = if body.len() > 300 {
            format!("{}... ({} bytes total)", &body[..300], body.len())
        } else {
            body.clone()
        };
        format!("JSON decode error for {} (status {}): {}. Response: {}", url, status, e, preview)
    })
}

/// Like [`get_json`], but treats a `404 Not Found` response as `Ok(None)`
/// instead of an error, which is the normal "no match" response for
/// lookup-style endpoints (e.g. Modrinth's hash lookup).
pub fn get_json_optional<T: serde::de::DeserializeOwned>(
    url: &str,
) -> Result<Option<T>, reqwest::Error> {
    let response = fetch(url)?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let response = response.error_for_status()?;
    Ok(Some(response.json()?))
}

pub fn get_bytes(url: &str) -> Result<Vec<u8>, reqwest::Error> {
    Ok(fetch(url)?.bytes()?.to_vec())
}

pub fn get_text(url: &str) -> Result<String, reqwest::Error> {
    fetch(url)?.text()
}

pub fn post_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    url: &str,
    body: &B,
) -> Result<T, reqwest::Error> {
    let mut last_err = None;
    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            std::thread::sleep(Duration::from_secs(2u64.pow(attempt)));
        }
        let resp = HTTP
            .post(url)
            .header("Content-Type", "application/json")
            .json(body)
            .send();
        match resp {
            Ok(r) => {
                if r.status().is_server_error() {
                    if attempt < MAX_RETRIES {
                        last_err = Some(r.error_for_status().unwrap_err());
                        continue;
                    }
                    return r.error_for_status()?.json();
                }
                return r.error_for_status()?.json();
            }
            Err(e) if retryable(&e) && attempt < MAX_RETRIES => {
                last_err = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.expect("retries exhausted with last_err set"))
}
