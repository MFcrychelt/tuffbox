use std::sync::LazyLock;
use std::time::Duration;

use reqwest::StatusCode;

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
const MAX_RATE_LIMIT_RETRIES: u32 = 4;
const MAX_REDIRECTS: u8 = 5;
/// Cap on how long we'll wait for a Retry-After backoff before giving up.
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 60;

fn retryable(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request()
}

/// Parses the `Retry-After` header from a 429 response.
/// Supports both delta-seconds (e.g. "5") and HTTP-date formats (e.g. "Wed, 21 Oct 2026 07:28:00 GMT").
/// Returns the number of seconds to wait.
fn parse_retry_after(value: &str) -> Option<u64> {
    let trimmed = value.trim();
    if let Ok(secs) = trimmed.parse::<u64>() {
        return Some(secs);
    }
    // Try HTTP-date format
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(trimmed) {
        let now = chrono::Utc::now();
        let diff = (date.with_timezone(&chrono::Utc) - now).num_seconds();
        if diff > 0 {
            return Some(diff as u64);
        }
    }
    None
}

/// Follows HTTP 3xx redirects manually, with workarounds for malformed `Location` headers
/// (e.g. protocol-relative `//cdn.example.com/path` or path-relative `/new-path`).
/// Respects a maximum redirect depth to prevent infinite loops.
///
/// Returns the final response. If the redirect chain exceeds `MAX_REDIRECTS`,
/// the last 3xx response is returned (its status code will be visible to
/// the caller, who can then surface it as an error).
fn follow_redirects(
    mut current_url: String,
) -> Result<reqwest::blocking::Response, reqwest::Error> {
    let mut last_response: Option<reqwest::blocking::Response> = None;

    for _ in 0..=MAX_REDIRECTS {
        let resp = HTTP.get(&current_url).send()?;
        let status = resp.status();
        last_response = Some(resp);

        if !status.is_redirection() {
            return Ok(last_response.unwrap());
        }

        // Extract Location header, handling both parsed-URL and raw-string forms
        let location = last_response
            .as_ref()
            .unwrap()
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let Some(loc) = location else {
            // No Location header — treat as final response (server bug, but we honour it)
            return Ok(last_response.unwrap());
        };

        // Resolve relative URLs
        let next_url = if loc.starts_with("//") {
            // Protocol-relative — prepend scheme from current URL
            if let Some(scheme_end) = current_url.find("://") {
                format!("{}{}", &current_url[..scheme_end + 1], loc)
            } else {
                format!("https:{}", loc)
            }
        } else if loc.starts_with('/') {
            // Path-relative — replace path on current URL
            if let Some(scheme_end) = current_url.find("://") {
                let after_scheme = &current_url[scheme_end + 3..];
                if let Some(path_start) = after_scheme.find('/') {
                    let host = &after_scheme[..path_start];
                    format!("{}://{}{}", &current_url[..scheme_end], host, loc)
                } else {
                    format!("{}{}", current_url, loc)
                }
            } else {
                loc.clone()
            }
        } else if loc.starts_with("http://") || loc.starts_with("https://") {
            loc.clone()
        } else {
            // Relative path — resolve against current URL's directory
            if let Some(last_slash) = current_url.rfind('/') {
                format!("{}/{}", &current_url[..last_slash], loc)
            } else {
                loc.clone()
            }
        };

        current_url = next_url;
    }

    // Exceeded MAX_REDIRECTS — return the last 3xx response so the caller
    // sees the actual status code.
    Ok(last_response.expect("loop ran at least once"))
}

fn fetch_with_redirects(url: &str) -> Result<reqwest::blocking::Response, reqwest::Error> {
    follow_redirects(url.to_string())
}

fn fetch(url: &str) -> Result<reqwest::blocking::Response, reqwest::Error> {
    let mut last_err: Option<reqwest::Error> = None;
    let mut rate_limit_retries: u32 = 0;

    for attempt in 0..=MAX_RETRIES {
        if attempt > 0 {
            std::thread::sleep(Duration::from_secs(2u64.pow(attempt)));
        }
        match fetch_with_redirects(url) {
            Ok(resp) => {
                let status = resp.status();

                // HTTP 429 Too Many Requests — respect Retry-After
                if status == StatusCode::TOO_MANY_REQUESTS {
                    rate_limit_retries += 1;
                    if rate_limit_retries > MAX_RATE_LIMIT_RETRIES {
                        return resp.error_for_status();
                    }
                    let delay_secs = resp
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(parse_retry_after)
                        .unwrap_or_else(|| {
                            // Exponential backoff: 10 * 2^retryCount, capped at MAX_RATE_LIMIT_WAIT_SECS
                            (10u64 * 2u64.pow(rate_limit_retries)).min(MAX_RATE_LIMIT_WAIT_SECS)
                        });
                    let capped_delay = delay_secs.min(MAX_RATE_LIMIT_WAIT_SECS);
                    last_err = Some(resp.error_for_status().unwrap_err());
                    std::thread::sleep(Duration::from_secs(capped_delay));
                    continue;
                }

                if status.is_server_error() {
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
    let body = response
        .text()
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    serde_json::from_str(&body).map_err(|e| {
        let preview = if body.len() > 300 {
            format!("{}... ({} bytes total)", &body[..300], body.len())
        } else {
            body.clone()
        };
        format!(
            "JSON decode error for {} (status {}): {}. Response: {}",
            url, status, e, preview
        )
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

/// Streams a download from `url` directly to disk at `dest`, computing a SHA1
/// hash incrementally as bytes arrive. This avoids loading the entire file
/// into memory (important for large mods/libs that can be hundreds of MB).
///
/// If `expected_sha1` is provided and the computed hash doesn't match, the
/// partial file is deleted and an error is returned.
///
/// The `progress` callback is invoked with `(bytes_received_so_far,
/// total_bytes)` after each chunk, allowing the UI to show real-time
/// download progress without buffering the whole file.
///
/// The download writes to a temporary file (`.part`) and atomically renames
/// it to `dest` only after the hash check passes — so a failed or interrupted
/// download never leaves a half-written file at `dest`.
pub fn download_streaming(
    url: &str,
    dest: &std::path::Path,
    expected_sha1: Option<&str>,
    mut progress: Option<Box<dyn FnMut(u64, u64) + Send>>,
) -> Result<(), StreamingDownloadError> {
    use sha1::{Digest, Sha1};
    use std::io::{Read, Write};

    let response = fetch_with_redirects(url).map_err(StreamingDownloadError::Http)?;
    if !response.status().is_success() {
        return Err(StreamingDownloadError::Http(
            response.error_for_status().unwrap_err(),
        ));
    }

    let total_size = response.content_length().unwrap_or(0);
    let mut hasher = Sha1::new();
    let mut received: u64 = 0;

    // Keep the temporary file in the destination directory. `NamedTempFile`
    // uses the platform replace primitive when persisted, unlike
    // `std::fs::rename`, which refuses to replace an existing file on
    // Windows. A failed download therefore leaves the previous jar intact.
    let parent = dest.parent().unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent).map_err(StreamingDownloadError::Io)?;
    let mut file = tempfile::Builder::new()
        .prefix(".tuffbox-download-")
        .suffix(".part")
        .tempfile_in(parent)
        .map_err(StreamingDownloadError::Io)?;

    let mut stream = response;
    let mut buffer = vec![0u8; 64 * 1024]; // 64KB chunks
    loop {
        let n = stream
            .read(&mut buffer)
            .map_err(StreamingDownloadError::Io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        file.write_all(&buffer[..n])
            .map_err(StreamingDownloadError::Io)?;
        received += n as u64;
        if let Some(ref mut cb) = progress {
            cb(received, total_size);
        }
    }
    file.flush().map_err(StreamingDownloadError::Io)?;
    file.as_file()
        .sync_all()
        .map_err(StreamingDownloadError::Io)?;

    let actual = format!("{:x}", hasher.finalize());
    if let Some(expected) = expected_sha1 {
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(StreamingDownloadError::ChecksumMismatch {
                expected: expected.to_string(),
                actual,
            });
        }
    }

    persist_replacing(file, dest)?;
    Ok(())
}

fn persist_replacing(
    file: tempfile::NamedTempFile,
    dest: &std::path::Path,
) -> Result<(), StreamingDownloadError> {
    file.persist(dest)
        .map(|_| ())
        .map_err(|error| StreamingDownloadError::Io(error.error))
}

#[derive(Debug, thiserror::Error)]
pub enum StreamingDownloadError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sha1 mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

#[cfg(test)]
mod download_tests {
    use super::persist_replacing;
    use std::io::Write;

    #[test]
    fn persisted_download_replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("same-name.jar");
        std::fs::write(&destination, b"old bytes").unwrap();

        let mut staged = tempfile::NamedTempFile::new_in(dir.path()).unwrap();
        staged.write_all(b"new bytes").unwrap();
        staged.flush().unwrap();
        persist_replacing(staged, &destination).unwrap();

        assert_eq!(std::fs::read(destination).unwrap(), b"new bytes");
    }
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
