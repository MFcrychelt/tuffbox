use serde::{Deserialize, Serialize};

pub const TRANSPORT_SCHEMA_VERSION: u32 = 1;
pub const REPO_TRANSPORT_FILE: &str = ".tuffbox/repo-transport.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransportKind {
    Github,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseAssetRef {
    pub mod_id: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepoTransportMeta {
    pub schema_version: u32,
    pub manifest_file: String,
    pub lockfile_file: String,
    pub pack_version: String,
    #[serde(default)]
    pub release_tag: Option<String>,
    /// `ready` | `publishing`
    #[serde(default = "ready_status")]
    pub status: String,
    #[serde(default)]
    pub release_assets: Vec<ReleaseAssetRef>,
    #[serde(default)]
    pub managed_files: Vec<String>,
    /// SHA-256 of sorted `path + NUL + sha256(bytes)` for every managed file
    /// except `.tuffbox/repo-transport.json`. Used for no-op publish detection.
    #[serde(default)]
    pub content_digest: String,
    #[serde(default)]
    pub signer_public_key: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
}

fn ready_status() -> String {
    "ready".into()
}

impl RepoTransportMeta {
    pub fn is_ready(&self) -> bool {
        self.status == "ready"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LocalTransportMeta {
    pub schema_version: u32,
    pub kind: TransportKind,
    pub repo: String,
    #[serde(default)]
    pub git_ref: Option<String>,
    pub manifest_file: String,
    pub installed_version: String,
    #[serde(default)]
    pub installed_commit_sha: Option<String>,
    #[serde(default)]
    pub installed_at: Option<String>,
    #[serde(default)]
    pub pinned_signer_public_key: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_transport_round_trip() {
        let meta = RepoTransportMeta {
            schema_version: TRANSPORT_SCHEMA_VERSION,
            manifest_file: "demo.tuffbox.json".into(),
            lockfile_file: "demo.tuffbox.lock.json".into(),
            pack_version: "1.2.3".into(),
            release_tag: Some("v1.2.3".into()),
            status: "ready".into(),
            release_assets: vec![ReleaseAssetRef {
                mod_id: "custom-lib".into(),
                file_name: "custom-lib.jar".into(),
            }],
            managed_files: vec!["pack.toml".into(), "index.toml".into()],
            content_digest: "abc".into(),
            signer_public_key: None,
            signature: None,
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        let back: RepoTransportMeta = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, back);
        assert!(back.is_ready());
    }
}
