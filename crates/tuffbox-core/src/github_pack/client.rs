//! GitHub REST client for pack transport. Mockable; no TuffSwarm coupling.

use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;
use thiserror::Error;

use crate::github_pack::source::validate_github_ref;

const APP_USER_AGENT: &str = "TuffBox-IDE/0.1";
const MAX_GITHUB_BYTES: u64 = 512 * 1024 * 1024;
const DOWNLOAD_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("GitHub HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("GitHub conflict: {0}")]
    Conflict(String),
    #[error("GitHub not found: {0}")]
    NotFound(String),
    #[error("request failed: {0}")]
    Request(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub trait GitHubApi: Send + Sync {
    fn send_json(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), GitHubError>;

    fn get_bytes(&self, path: &str) -> Result<Vec<u8>, GitHubError>;

    fn upload_release_asset(
        &self,
        upload_url_template: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, GitHubError>;

    fn download_release_asset(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
        name: &str,
    ) -> Result<Vec<u8>, GitHubError>;
}

fn map_status(status: u16, path: &str, body: &Value) -> Result<(), GitHubError> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    let message = body
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if status == 404 {
        return Err(GitHubError::NotFound(format!("{path}: {message}")));
    }
    if status == 409 || status == 422 {
        return Err(GitHubError::Conflict(if message.is_empty() {
            format!("HTTP {status} for {path}")
        } else {
            message
        }));
    }
    Err(GitHubError::Http { status, message })
}

pub struct LiveGitHubApi {
    http: reqwest::blocking::Client,
    token: Option<String>,
    api_base: String,
}

impl LiveGitHubApi {
    pub fn new(token: Option<String>) -> Result<Self, GitHubError> {
        Self::with_base("https://api.github.com", token)
    }

    pub fn with_base(api_base: &str, token: Option<String>) -> Result<Self, GitHubError> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(APP_USER_AGENT)
            .timeout(std::time::Duration::from_secs(90))
            .build()
            .map_err(|e| GitHubError::Request(e.to_string()))?;
        Ok(Self {
            http,
            token,
            api_base: api_base.trim_end_matches('/').to_string(),
        })
    }
}

impl GitHubApi for LiveGitHubApi {
    fn send_json(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), GitHubError> {
        let url = format!("{}{path}", self.api_base);
        let parsed = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|e| GitHubError::Request(e.to_string()))?;
        let mut req = self
            .http
            .request(parsed, &url)
            .header("Accept", "application/vnd.github+json");
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req
            .send()
            .map_err(|e| GitHubError::Request(e.to_string()))?;
        let status = resp.status().as_u16();
        let value = resp.json::<Value>().unwrap_or(Value::Null);
        Ok((status, value))
    }

    fn get_bytes(&self, path: &str) -> Result<Vec<u8>, GitHubError> {
        let url = if path.starts_with("https://") {
            path.to_string()
        } else {
            format!("{}{path}", self.api_base)
        };
        let parsed = reqwest::Url::parse(&url).map_err(|e| GitHubError::Request(e.to_string()))?;
        if path.starts_with("https://")
            && !allowed_github_download_host(parsed.host_str(), &self.api_base)
        {
            return Err(GitHubError::Request(format!(
                "blocked non-GitHub download host: {}",
                parsed.host_str().unwrap_or("<none>")
            )));
        }
        let mut req = self
            .http
            .get(parsed.clone())
            .header("Accept", "application/octet-stream");
        if should_attach_author_token(parsed.host_str(), &self.api_base) {
            if let Some(token) = &self.token {
                req = req.bearer_auth(token);
            }
        }
        let resp = req
            .send()
            .map_err(|e| GitHubError::Request(e.to_string()))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(GitHubError::Http {
                status,
                message: format!("GET {path}"),
            });
        }
        if resp
            .content_length()
            .is_some_and(|size| size > MAX_GITHUB_BYTES)
        {
            return Err(GitHubError::Request(
                "GitHub download exceeds 512 MiB limit".into(),
            ));
        }
        read_response_with_limit(resp, MAX_GITHUB_BYTES)
    }

    fn upload_release_asset(
        &self,
        upload_url_template: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, GitHubError> {
        let base = upload_url_template
            .split('{')
            .next()
            .unwrap_or(upload_url_template);
        let url = format!("{base}?name={}", urlencoding_name(name));
        let mut req = self
            .http
            .post(&url)
            .header("Accept", "application/vnd.github+json")
            .header("Content-Type", "application/octet-stream")
            .body(bytes.to_vec());
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .map_err(|e| GitHubError::Request(e.to_string()))?;
        let status = resp.status().as_u16();
        let value = resp.json::<Value>().unwrap_or(Value::Null);
        map_status(status, &url, &value)?;
        Ok(value)
    }

    fn download_release_asset(
        &self,
        owner: &str,
        repo: &str,
        tag: &str,
        name: &str,
    ) -> Result<Vec<u8>, GitHubError> {
        validate_github_ref(tag).map_err(|e| GitHubError::Request(e.to_string()))?;
        let encoded_tag = encode_path_segment(tag);
        let release = get_json(
            self,
            &format!("/repos/{owner}/{repo}/releases/tags/{encoded_tag}"),
        )?;
        let asset_url = release
            .get("assets")
            .and_then(Value::as_array)
            .and_then(|assets| {
                assets.iter().find_map(|asset| {
                    (asset.get("name").and_then(Value::as_str) == Some(name))
                        .then(|| asset.get("url").and_then(Value::as_str))
                        .flatten()
                })
            })
            .ok_or_else(|| {
                GitHubError::NotFound(format!("release asset {owner}/{repo}@{tag}/{name}"))
            })?;
        self.get_bytes(asset_url)
    }
}

fn encode_path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn allowed_github_download_host(host: Option<&str>, api_base: &str) -> bool {
    let Some(host) = host else {
        return false;
    };
    if matches!(
        host,
        "api.github.com"
            | "uploads.github.com"
            | "objects.githubusercontent.com"
            | "github.com"
            | "codeload.github.com"
    ) {
        return true;
    }
    reqwest::Url::parse(api_base)
        .ok()
        .and_then(|url| url.host_str().map(|api_host| api_host == host))
        .unwrap_or(false)
}

fn should_attach_author_token(host: Option<&str>, api_base: &str) -> bool {
    let Some(host) = host else {
        return false;
    };
    if host == "api.github.com" {
        return true;
    }
    reqwest::Url::parse(api_base)
        .ok()
        .and_then(|url| url.host_str().map(|api_host| api_host == host))
        .unwrap_or(false)
}

fn read_response_with_limit(
    mut resp: reqwest::blocking::Response,
    limit: u64,
) -> Result<Vec<u8>, GitHubError> {
    let mut bytes = Vec::new();
    let mut chunk = vec![0_u8; DOWNLOAD_CHUNK_BYTES];
    loop {
        let read = resp
            .read(&mut chunk)
            .map_err(|e| GitHubError::Request(e.to_string()))?;
        if read == 0 {
            break;
        }
        let next_len = u64::try_from(bytes.len().saturating_add(read)).unwrap_or(u64::MAX);
        if next_len > limit {
            return Err(GitHubError::Request(
                "GitHub download exceeds 512 MiB limit".into(),
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(bytes)
}

fn urlencoding_name(name: &str) -> String {
    name.replace(' ', "%20")
}

pub fn get_json(api: &dyn GitHubApi, path: &str) -> Result<Value, GitHubError> {
    let (status, body) = api.send_json("GET", path, None)?;
    map_status(status, path, &body)?;
    Ok(body)
}

pub fn post_json(api: &dyn GitHubApi, path: &str, body: &Value) -> Result<Value, GitHubError> {
    let (status, value) = api.send_json("POST", path, Some(body))?;
    map_status(status, path, &value)?;
    Ok(value)
}

pub fn patch_json(api: &dyn GitHubApi, path: &str, body: &Value) -> Result<Value, GitHubError> {
    let (status, value) = api.send_json("PATCH", path, Some(body))?;
    map_status(status, path, &value)?;
    Ok(value)
}

pub fn default_branch(api: &dyn GitHubApi, owner: &str, repo: &str) -> Result<String, GitHubError> {
    let body = get_json(api, &format!("/repos/{owner}/{repo}"))?;
    Ok(body
        .get("default_branch")
        .and_then(|v| v.as_str())
        .unwrap_or("main")
        .to_string())
}

pub fn ref_sha(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    git_ref: &str,
) -> Result<Option<String>, GitHubError> {
    validate_github_ref(git_ref).map_err(|e| GitHubError::Request(e.to_string()))?;
    let encoded = encode_path_segment(git_ref);
    match get_json(api, &format!("/repos/{owner}/{repo}/git/ref/{encoded}")) {
        Ok(body) => Ok(body
            .pointer("/object/sha")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())),
        Err(GitHubError::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn inspect_public_pack(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
) -> Result<serde_json::Value, GitHubError> {
    let repo_json = get_json(api, &format!("/repos/{owner}/{repo}"))?;
    let default_branch = repo_json
        .get("default_branch")
        .and_then(|v| v.as_str())
        .unwrap_or("main")
        .to_string();
    let transport = match get_json(
        api,
        &format!("/repos/{owner}/{repo}/contents/.tuffbox/repo-transport.json"),
    ) {
        Ok(body) => {
            let content = body
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .replace('\n', "");
            let bytes = if body.get("encoding").and_then(|v| v.as_str()) == Some("base64") {
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, content)
                    .unwrap_or_default()
            } else {
                content.into_bytes()
            };
            serde_json::from_slice::<Value>(&bytes).ok()
        }
        Err(GitHubError::NotFound(_)) => None,
        Err(e) => return Err(e),
    };
    Ok(serde_json::json!({
        "owner": owner,
        "repo": repo,
        "fullName": repo_json.get("full_name").and_then(|v| v.as_str()).unwrap_or(""),
        "description": repo_json.get("description").and_then(|v| v.as_str()).unwrap_or(""),
        "defaultBranch": default_branch,
        "htmlUrl": format!("https://github.com/{owner}/{repo}"),
        "packVersion": transport.as_ref().and_then(|t| t.get("packVersion")).cloned(),
        "status": transport.as_ref().and_then(|t| t.get("status")).cloned(),
        "ready": transport
            .as_ref()
            .and_then(|t| t.get("status").and_then(|s| s.as_str()))
            .map(|s| s == "ready")
            .unwrap_or(false),
    }))
}

pub fn commit_sha(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    git_ref: &str,
) -> Result<String, GitHubError> {
    validate_github_ref(git_ref).map_err(|e| GitHubError::Request(e.to_string()))?;
    let encoded = encode_path_segment(git_ref);
    let body = get_json(api, &format!("/repos/{owner}/{repo}/commits/{encoded}"))?;
    body.get("sha")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| GitHubError::Request("commit response missing sha".into()))
}

pub fn recursive_tree(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<Vec<TreeEntry>, GitHubError> {
    let body = get_json(
        api,
        &format!("/repos/{owner}/{repo}/git/trees/{sha}?recursive=1"),
    )?;
    let tree = body
        .get("tree")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(tree
        .into_iter()
        .filter_map(|entry| {
            Some(TreeEntry {
                path: entry.get("path")?.as_str()?.to_string(),
                mode: entry
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("100644")
                    .to_string(),
                kind: entry
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("blob")
                    .to_string(),
                sha: entry.get("sha")?.as_str()?.to_string(),
            })
        })
        .collect())
}

pub fn blob_bytes(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<Vec<u8>, GitHubError> {
    let body = get_json(api, &format!("/repos/{owner}/{repo}/git/blobs/{sha}"))?;
    let content = body
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .replace('\n', "");
    let encoding = body
        .get("encoding")
        .and_then(|v| v.as_str())
        .unwrap_or("utf-8");
    if encoding == "base64" {
        base64::engine::general_purpose::STANDARD
            .decode(content)
            .map_err(|e| GitHubError::Request(e.to_string()))
    } else {
        Ok(content.into_bytes())
    }
}

#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: String,
    #[allow(dead_code)]
    pub mode: String,
    pub kind: String,
    pub sha: String,
}

pub fn create_blob(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    bytes: &[u8],
) -> Result<String, GitHubError> {
    let body = post_json(
        api,
        &format!("/repos/{owner}/{repo}/git/blobs"),
        &json!({
            "content": base64::engine::general_purpose::STANDARD.encode(bytes),
            "encoding": "base64",
        }),
    )?;
    body.get("sha")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| GitHubError::Request("blob response missing sha".into()))
}

pub fn create_tree(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    base_tree: Option<&str>,
    entries: &[Value],
) -> Result<String, GitHubError> {
    let mut body = json!({ "tree": entries });
    if let Some(base) = base_tree {
        body["base_tree"] = json!(base);
    }
    let resp = post_json(api, &format!("/repos/{owner}/{repo}/git/trees"), &body)?;
    resp.get("sha")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| GitHubError::Request("tree response missing sha".into()))
}

pub fn create_commit(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    message: &str,
    tree: &str,
    parents: &[String],
) -> Result<String, GitHubError> {
    let resp = post_json(
        api,
        &format!("/repos/{owner}/{repo}/git/commits"),
        &json!({
            "message": message,
            "tree": tree,
            "parents": parents,
        }),
    )?;
    resp.get("sha")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| GitHubError::Request("commit response missing sha".into()))
}

pub fn update_ref(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    git_ref: &str,
    sha: &str,
    create: bool,
) -> Result<(), GitHubError> {
    validate_github_ref(git_ref).map_err(|e| GitHubError::Request(e.to_string()))?;
    let encoded = encode_path_segment(git_ref);
    if create {
        post_json(
            api,
            &format!("/repos/{owner}/{repo}/git/refs"),
            &json!({ "ref": format!("refs/{git_ref}"), "sha": sha }),
        )?;
        return Ok(());
    }
    patch_json(
        api,
        &format!("/repos/{owner}/{repo}/git/refs/{encoded}"),
        &json!({ "sha": sha, "force": false }),
    )?;
    Ok(())
}

/// In-memory GitHub Git Data API used by unit tests.
pub struct MockGitHub {
    inner: Mutex<MockState>,
}

struct MockState {
    owner: String,
    repo: String,
    default_branch: String,
    next_id: u64,
    blobs: HashMap<String, Vec<u8>>,
    trees: HashMap<String, Vec<Value>>,
    commits: HashMap<String, CommitObj>,
    refs: HashMap<String, String>,
    releases: Vec<Value>,
    assets: HashMap<String, Vec<u8>>,
    conflict_next_ref: bool,
}

struct CommitObj {
    tree: String,
    parents: Vec<String>,
    #[allow(dead_code)]
    message: String,
}

impl MockGitHub {
    pub fn new(owner: &str, repo: &str) -> Self {
        Self {
            inner: Mutex::new(MockState {
                owner: owner.into(),
                repo: repo.into(),
                default_branch: "main".into(),
                next_id: 1,
                blobs: HashMap::new(),
                trees: HashMap::new(),
                commits: HashMap::new(),
                refs: HashMap::new(),
                releases: Vec::new(),
                assets: HashMap::new(),
                conflict_next_ref: false,
            }),
        }
    }

    pub fn fail_next_ref_update(&self) {
        self.inner.lock().unwrap().conflict_next_ref = true;
    }

    pub fn head_files(&self) -> HashMap<String, Vec<u8>> {
        let g = self.inner.lock().unwrap();
        let Some(commit) = g.refs.get("heads/main") else {
            return HashMap::new();
        };
        let Some(c) = g.commits.get(commit) else {
            return HashMap::new();
        };
        flatten_tree(&g, &c.tree)
    }

    pub fn head_sha(&self) -> Option<String> {
        self.inner.lock().unwrap().refs.get("heads/main").cloned()
    }

    pub fn uploaded_assets(&self) -> Vec<String> {
        self.inner.lock().unwrap().assets.keys().cloned().collect()
    }

    fn alloc(g: &mut MockState) -> String {
        let id = g.next_id;
        g.next_id += 1;
        format!("{id:040x}")
    }
}

fn flatten_tree(g: &MockState, tree_sha: &str) -> HashMap<String, Vec<u8>> {
    let mut out = HashMap::new();
    let Some(entries) = g.trees.get(tree_sha) else {
        return out;
    };
    for entry in entries {
        let path = entry.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let kind = entry.get("type").and_then(|v| v.as_str()).unwrap_or("blob");
        let sha = entry.get("sha").and_then(|v| v.as_str()).unwrap_or("");
        if sha.is_empty() || sha == "null" {
            continue;
        }
        if kind == "tree" {
            for (k, v) in flatten_tree(g, sha) {
                out.insert(format!("{path}/{k}"), v);
            }
        } else if let Some(bytes) = g.blobs.get(sha) {
            out.insert(path.to_string(), bytes.clone());
        }
    }
    out
}

impl GitHubApi for MockGitHub {
    fn send_json(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value), GitHubError> {
        let (path_only, _query) = path.split_once('?').unwrap_or((path, ""));
        let mut g = self.inner.lock().unwrap();
        let prefix = format!("/repos/{}/{}", g.owner, g.repo);
        if method == "GET" && path_only == prefix {
            return Ok((
                200,
                json!({ "default_branch": g.default_branch, "full_name": format!("{}/{}", g.owner, g.repo), "description": "demo pack" }),
            ));
        }
        if method == "GET" && path_only.starts_with(&format!("{prefix}/contents/")) {
            let rel = path_only.trim_start_matches(&format!("{prefix}/contents/"));
            let files = g
                .refs
                .get("heads/main")
                .and_then(|commit| g.commits.get(commit))
                .map(|c| flatten_tree(&g, &c.tree))
                .unwrap_or_default();
            return match files.get(rel) {
                Some(bytes) => Ok((
                    200,
                    json!({
                        "encoding": "base64",
                        "content": base64::engine::general_purpose::STANDARD.encode(bytes),
                    }),
                )),
                None => Ok((404, json!({ "message": "Not Found" }))),
            };
        }
        if method == "GET" && path_only.starts_with(&format!("{prefix}/git/ref/")) {
            let git_ref = path_only.trim_start_matches(&format!("{prefix}/git/ref/"));
            return match g.refs.get(git_ref) {
                Some(sha) => Ok((
                    200,
                    json!({ "ref": format!("refs/{git_ref}"), "object": { "sha": sha, "type": "commit" } }),
                )),
                None => Ok((404, json!({ "message": "Not Found" }))),
            };
        }
        if method == "GET" && path_only.starts_with(&format!("{prefix}/commits/")) {
            let git_ref = path_only.trim_start_matches(&format!("{prefix}/commits/"));
            let sha = if git_ref == "HEAD" || git_ref == g.default_branch {
                g.refs.get(&format!("heads/{}", g.default_branch)).cloned()
            } else {
                g.refs
                    .get(&format!("heads/{git_ref}"))
                    .cloned()
                    .or_else(|| g.refs.get(&format!("tags/{git_ref}")).cloned())
                    .or_else(|| {
                        if g.commits.contains_key(git_ref) {
                            Some(git_ref.to_string())
                        } else {
                            None
                        }
                    })
            };
            return match sha {
                Some(sha) => Ok((200, json!({ "sha": sha }))),
                None => Ok((404, json!({ "message": "Not Found" }))),
            };
        }
        if method == "GET" && path_only.starts_with(&format!("{prefix}/git/trees/")) {
            let sha = path_only.trim_start_matches(&format!("{prefix}/git/trees/"));
            let sha = g
                .commits
                .get(sha)
                .map(|c| c.tree.clone())
                .unwrap_or_else(|| sha.to_string());
            let Some(tree) = g.trees.get(&sha) else {
                return Ok((404, json!({ "message": "Not Found" })));
            };
            return Ok((200, json!({ "sha": sha, "tree": tree, "truncated": false })));
        }
        if method == "GET" && path_only.starts_with(&format!("{prefix}/git/blobs/")) {
            let sha = path_only.trim_start_matches(&format!("{prefix}/git/blobs/"));
            let Some(bytes) = g.blobs.get(sha) else {
                return Ok((404, json!({ "message": "Not Found" })));
            };
            return Ok((
                200,
                json!({
                    "sha": sha,
                    "encoding": "base64",
                    "content": base64::engine::general_purpose::STANDARD.encode(bytes),
                }),
            ));
        }
        if method == "POST" && path_only == format!("{prefix}/git/blobs") {
            let content = body
                .and_then(|b| b.get("content"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .replace('\n', "");
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(content)
                .unwrap_or_default();
            let sha = Self::alloc(&mut g);
            g.blobs.insert(sha.clone(), bytes);
            return Ok((201, json!({ "sha": sha })));
        }
        if method == "POST" && path_only == format!("{prefix}/git/trees") {
            let mut entries = body
                .and_then(|b| b.get("tree"))
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if let Some(base) = body
                .and_then(|b| b.get("base_tree"))
                .and_then(|v| v.as_str())
            {
                let base = g
                    .commits
                    .get(base)
                    .map(|c| c.tree.clone())
                    .unwrap_or_else(|| base.to_string());
                let mut merged = g.trees.get(&base).cloned().unwrap_or_default();
                for entry in entries {
                    let path = entry
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let sha_null = entry.get("sha").map(|v| v.is_null()).unwrap_or(false);
                    merged
                        .retain(|e| e.get("path").and_then(|v| v.as_str()) != Some(path.as_str()));
                    if !sha_null {
                        merged.push(entry);
                    }
                }
                entries = merged;
            }
            let sha = Self::alloc(&mut g);
            g.trees.insert(sha.clone(), entries);
            return Ok((201, json!({ "sha": sha })));
        }
        if method == "POST" && path_only == format!("{prefix}/git/commits") {
            let tree = body
                .and_then(|b| b.get("tree"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let parents = body
                .and_then(|b| b.get("parents"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let message = body
                .and_then(|b| b.get("message"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let sha = Self::alloc(&mut g);
            g.commits.insert(
                sha.clone(),
                CommitObj {
                    tree,
                    parents,
                    message,
                },
            );
            return Ok((201, json!({ "sha": sha })));
        }
        if method == "POST" && path_only == format!("{prefix}/git/refs") {
            let git_ref = body
                .and_then(|b| b.get("ref"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim_start_matches("refs/")
                .to_string();
            let sha = body
                .and_then(|b| b.get("sha"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if g.refs.contains_key(&git_ref) {
                return Ok((422, json!({ "message": "Reference already exists" })));
            }
            g.refs.insert(git_ref, sha.clone());
            return Ok((201, json!({ "sha": sha })));
        }
        if method == "PATCH" && path_only.starts_with(&format!("{prefix}/git/refs/")) {
            if g.conflict_next_ref {
                g.conflict_next_ref = false;
                return Ok((422, json!({ "message": "Update is not a fast forward" })));
            }
            let git_ref = path_only.trim_start_matches(&format!("{prefix}/git/refs/"));
            let sha = body
                .and_then(|b| b.get("sha"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let current = g.refs.get(git_ref).cloned();
            if let Some(cur) = &current {
                if let Some(commit) = g.commits.get(&sha) {
                    if !commit.parents.iter().any(|p| p == cur) && cur != &sha {
                        return Ok((422, json!({ "message": "Update is not a fast forward" })));
                    }
                }
            }
            g.refs.insert(git_ref.to_string(), sha.clone());
            return Ok((200, json!({ "object": { "sha": sha } })));
        }
        if method == "POST" && path_only == format!("{prefix}/releases") {
            let id = g.releases.len() as u64 + 1;
            let tag = body
                .and_then(|b| b.get("tag_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("v0")
                .to_string();
            let draft = body
                .and_then(|b| b.get("draft"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let rel = json!({
                "id": id,
                "tag_name": tag,
                "draft": draft,
                "upload_url": format!("https://uploads.example/{id}/assets{{?name,label}}"),
                "html_url": format!("https://github.com/{}/{}/releases/tag/{tag}", g.owner, g.repo),
            });
            g.releases.push(rel.clone());
            return Ok((201, rel));
        }
        if method == "PATCH" && path_only.starts_with(&format!("{prefix}/releases/")) {
            let id: u64 = path_only
                .trim_start_matches(&format!("{prefix}/releases/"))
                .parse()
                .unwrap_or(0);
            if let Some(rel) = g
                .releases
                .iter_mut()
                .find(|r| r.get("id").and_then(|v| v.as_u64()) == Some(id))
            {
                if let Some(draft) = body.and_then(|b| b.get("draft")) {
                    rel["draft"] = draft.clone();
                }
                return Ok((200, rel.clone()));
            }
            return Ok((404, json!({ "message": "Not Found" })));
        }
        Ok((
            404,
            json!({ "message": format!("unhandled {method} {path}") }),
        ))
    }

    fn get_bytes(&self, path: &str) -> Result<Vec<u8>, GitHubError> {
        let _ = path;
        let (prefix, files) = {
            let g = self.inner.lock().unwrap();
            let sha = g
                .refs
                .get("heads/main")
                .cloned()
                .unwrap_or_else(|| "deadbeef".into());
            let prefix = format!("{}-{}-{sha}", g.owner, g.repo);
            let files = g
                .refs
                .get("heads/main")
                .and_then(|commit| g.commits.get(commit))
                .map(|c| flatten_tree(&g, &c.tree))
                .unwrap_or_default();
            (prefix, files)
        };
        let mut tar_buf = Vec::new();
        {
            let enc = flate2::write::GzEncoder::new(&mut tar_buf, flate2::Compression::default());
            let mut builder = tar::Builder::new(enc);
            for (rel, bytes) in &files {
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_cksum();
                builder
                    .append_data(&mut header, format!("{prefix}/{rel}"), bytes.as_slice())
                    .map_err(|e| GitHubError::Request(e.to_string()))?;
            }
            builder
                .finish()
                .map_err(|e| GitHubError::Request(e.to_string()))?;
        }
        Ok(tar_buf)
    }

    fn upload_release_asset(
        &self,
        _upload_url_template: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<Value, GitHubError> {
        let mut g = self.inner.lock().unwrap();
        g.assets.insert(name.to_string(), bytes.to_vec());
        Ok(json!({ "name": name, "size": bytes.len() }))
    }

    fn download_release_asset(
        &self,
        _owner: &str,
        _repo: &str,
        _tag: &str,
        name: &str,
    ) -> Result<Vec<u8>, GitHubError> {
        self.inner
            .lock()
            .unwrap()
            .assets
            .get(name)
            .cloned()
            .ok_or_else(|| GitHubError::NotFound(format!("release asset {name}")))
    }
}
