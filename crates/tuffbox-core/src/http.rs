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
                if resp.status().is_server_error() && attempt < MAX_RETRIES {
                    last_err = Some(resp.error_for_status().unwrap_err());
                    continue;
                }
                return Ok(resp);
            }
            Err(e) if retryable(&e) && attempt < MAX_RETRIES => {
                last_err = Some(e);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap())
}

pub fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T, reqwest::Error> {
    fetch(url)?.json()
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
