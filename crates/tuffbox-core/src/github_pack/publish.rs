//! Publish a staged pack tree through GitHub Git Data API (one visible commit).

use crate::github_pack::client::{
    blob_bytes, create_blob, create_commit, create_tree, default_branch, patch_json, post_json,
    recursive_tree, ref_sha, update_ref, GitHubApi, GitHubError, TreeEntry,
};
use crate::github_pack::staging::StagedRepo;
use crate::github_pack::types::{RepoTransportMeta, REPO_TRANSPORT_FILE};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PublishError {
    #[error("{0}")]
    GitHub(#[from] GitHubError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("publish is a no-op (content digest unchanged)")]
    NoOp,
    #[error("branch update conflict: {0}")]
    Conflict(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishResult {
    pub commit_sha: String,
    pub branch: String,
    pub noop: bool,
    pub share_url: String,
    pub release_url: Option<String>,
    pub two_phase: bool,
}

pub fn publish_staged_tree(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    branch: Option<&str>,
    staged: &StagedRepo,
    message: &str,
) -> Result<PublishResult, PublishError> {
    let branch = match branch {
        Some(b) if !b.is_empty() => b.to_string(),
        _ => default_branch(api, owner, repo)?,
    };
    let git_ref = format!("heads/{branch}");
    let current_sha = ref_sha(api, owner, repo, &git_ref)?;

    if let Some(sha) = &current_sha {
        if remote_digest_matches(api, owner, repo, sha, &staged.transport)? {
            return Err(PublishError::NoOp);
        }
    }

    let two_phase = !staged.pending_release_assets.is_empty();
    let commit_sha = commit_tree(
        api,
        owner,
        repo,
        &git_ref,
        current_sha.as_deref(),
        staged,
        message,
    )
    .map_err(map_conflict)?;

    let mut release_url = None;
    if two_phase {
        let tag = staged
            .transport
            .release_tag
            .clone()
            .unwrap_or_else(|| format!("v{}", staged.transport.pack_version));
        let created = post_json(
            api,
            &format!("/repos/{owner}/{repo}/releases"),
            &json!({
                "tag_name": tag,
                "name": tag,
                "draft": true,
                "target_commitish": commit_sha,
            }),
        )?;
        let upload_url = created
            .get("upload_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        for asset in &staged.pending_release_assets {
            let bytes = fs::read(&asset.abs_path)?;
            api.upload_release_asset(upload_url, &asset.file_name, &bytes)?;
        }
        mark_transport_ready(&staged.root, &staged.transport)?;
        let restaged_files = reread_files_for_ready_marker(staged)?;
        commit_files(
            api,
            owner,
            repo,
            &git_ref,
            Some(&commit_sha),
            &staged.root,
            &restaged_files,
            &format!("{message} (mark ready)"),
        )
        .map_err(map_conflict)?;
        if let Some(id) = created.get("id").and_then(|v| v.as_u64()) {
            let published = patch_json(
                api,
                &format!("/repos/{owner}/{repo}/releases/{id}"),
                &json!({ "draft": false }),
            )?;
            release_url = published
                .get("html_url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
    } else if let Some(tag) = &staged.transport.release_tag {
        match post_json(
            api,
            &format!("/repos/{owner}/{repo}/git/refs"),
            &json!({ "ref": format!("refs/tags/{tag}"), "sha": commit_sha }),
        ) {
            Ok(_) => {}
            Err(GitHubError::Conflict(msg)) => {
                return Err(PublishError::Conflict(format!("tag {tag}: {msg}")));
            }
            Err(e) => return Err(e.into()),
        }
    }

    let final_sha = ref_sha(api, owner, repo, &git_ref)?.unwrap_or(commit_sha);
    Ok(PublishResult {
        share_url: format!("https://github.com/{owner}/{repo}"),
        commit_sha: final_sha,
        branch,
        noop: false,
        release_url,
        two_phase,
    })
}

fn map_conflict(err: GitHubError) -> PublishError {
    match err {
        GitHubError::Conflict(msg) => PublishError::Conflict(msg),
        other => PublishError::GitHub(other),
    }
}

fn remote_digest_matches(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    commit_sha: &str,
    local: &RepoTransportMeta,
) -> Result<bool, PublishError> {
    if local.content_digest.is_empty() {
        return Ok(false);
    }
    let tree = recursive_tree(api, owner, repo, commit_sha)?;
    let Some(entry) = tree.iter().find(|e| e.path == REPO_TRANSPORT_FILE) else {
        return Ok(false);
    };
    let bytes = blob_bytes(api, owner, repo, &entry.sha)?;
    let remote: RepoTransportMeta = serde_json::from_slice(&bytes)?;
    Ok(remote.content_digest == local.content_digest && remote.status == local.status)
}

fn commit_tree(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    git_ref: &str,
    parent: Option<&str>,
    staged: &StagedRepo,
    message: &str,
) -> Result<String, GitHubError> {
    let paths: Vec<String> = staged.files.iter().map(|f| f.relative_path.clone()).collect();
    commit_files(
        api,
        owner,
        repo,
        git_ref,
        parent,
        &staged.root,
        &paths,
        message,
    )
}

fn reread_files_for_ready_marker(staged: &StagedRepo) -> Result<Vec<String>, PublishError> {
    Ok(staged.files.iter().map(|f| f.relative_path.clone()).collect())
}

fn mark_transport_ready(root: &std::path::Path, current: &RepoTransportMeta) -> Result<(), PublishError> {
    let path = root.join(REPO_TRANSPORT_FILE);
    let mut meta: RepoTransportMeta = if path.is_file() {
        serde_json::from_slice(&fs::read(&path)?)?
    } else {
        current.clone()
    };
    meta.status = "ready".into();
    fs::write(path, serde_json::to_vec_pretty(&meta)?)?;
    Ok(())
}

fn commit_files(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    git_ref: &str,
    parent: Option<&str>,
    root: &std::path::Path,
    relative_paths: &[String],
    message: &str,
) -> Result<String, GitHubError> {
    let mut tree_entries: Vec<Value> = Vec::new();
    for rel in relative_paths {
        let bytes = fs::read(root.join(rel)).map_err(|e| GitHubError::Request(e.to_string()))?;
        let sha = create_blob(api, owner, repo, &bytes)?;
        tree_entries.push(json!({
            "path": rel,
            "mode": "100644",
            "type": "blob",
            "sha": sha,
        }));
    }

    let base_tree = if let Some(parent_sha) = parent {
        let existing = recursive_tree(api, owner, repo, parent_sha)?;
        let previous_managed = previous_managed_files(api, owner, repo, &existing)?;
        for old in previous_managed {
            if !relative_paths.iter().any(|p| p == &old) {
                tree_entries.push(json!({
                    "path": old,
                    "mode": "100644",
                    "type": "blob",
                    "sha": Value::Null,
                }));
            }
        }
        existing
            .iter()
            .find(|e| e.kind == "tree" && e.path.is_empty())
            .map(|e| e.sha.clone())
            .or_else(|| Some(parent_sha.to_string()))
    } else {
        None
    };

    let tree_sha = create_tree(
        api,
        owner,
        repo,
        base_tree.as_deref(),
        &tree_entries,
    )?;
    let parents: Vec<String> = parent.into_iter().map(|s| s.to_string()).collect();
    let commit_sha = create_commit(api, owner, repo, message, &tree_sha, &parents)?;
    update_ref(api, owner, repo, git_ref, &commit_sha, parent.is_none())?;
    Ok(commit_sha)
}

fn previous_managed_files(
    api: &dyn GitHubApi,
    owner: &str,
    repo: &str,
    tree: &[TreeEntry],
) -> Result<Vec<String>, GitHubError> {
    let Some(entry) = tree.iter().find(|e| e.path == REPO_TRANSPORT_FILE) else {
        return Ok(Vec::new());
    };
    let bytes = blob_bytes(api, owner, repo, &entry.sha)?;
    let meta: RepoTransportMeta =
        serde_json::from_slice(&bytes).map_err(|e| GitHubError::Request(e.to_string()))?;
    Ok(meta.managed_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github_pack::client::MockGitHub;
    use crate::github_pack::staging::{stage_repo_tree, StageOptions};
    use crate::github_pack::types::TRANSPORT_SCHEMA_VERSION;
    use crate::manifest::{
        ContentType, FileHashes, LoaderKind, LoaderSpec, MinecraftSpec, ModSource, ModSpec,
        ProjectManifest, ProjectMetadata, Side, SourceKind,
    };
    use std::fs;

    fn sample_manifest(version: &str, extra: Vec<ModSpec>) -> ProjectManifest {
        let mut mods = vec![ModSpec {
            id: "sodium".into(),
            name: "Sodium".into(),
            source: ModSource {
                kind: SourceKind::Modrinth,
                project_id: Some("AANobbMI".into()),
                file_id: Some("ver1".into()),
                url: Some("https://example.com/sodium.jar".into()),
                path: None,
                icon_url: None,
                categories: vec![],
            },
            version: "0.6.0".into(),
            file_name: Some("sodium.jar".into()),
            hashes: Some(FileHashes {
                sha1: Some("deadbeef".into()),
                sha512: None,
            }),
            side: Side::Client,
            dependencies: vec![],
            status: vec![],
            content_type: ContentType::Mod,
            authors: vec![],
            option: None,
        }];
        mods.extend(extra);
        ProjectManifest {
            schema_version: crate::manifest::CURRENT_PROJECT_SCHEMA_VERSION.into(),
            project: ProjectMetadata {
                id: "demo".into(),
                name: "Demo Pack".into(),
                version: version.into(),
                description: None,
                authors: vec!["Tester".into()],
            },
            minecraft: MinecraftSpec {
                version: "1.21.1".into(),
            },
            loader: LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.0".into(),
            },
            brief: None,
            listing: None,
            java: None,
            profiles: vec![],
            mods,
            overrides: None,
        }
    }

    fn custom_jar_mod() -> ModSpec {
        ModSpec {
            id: "custom-lib".into(),
            name: "Custom Lib".into(),
            source: ModSource {
                kind: SourceKind::Local,
                project_id: None,
                file_id: None,
                url: None,
                path: Some("mods/custom-lib.jar".into()),
                icon_url: None,
                categories: vec![],
            },
            version: "1.0.0".into(),
            file_name: Some("custom-lib.jar".into()),
            hashes: Some(FileHashes {
                sha1: Some("aa".into()),
                sha512: None,
            }),
            side: Side::Both,
            dependencies: vec![],
            status: vec![],
            content_type: ContentType::Mod,
            authors: vec![],
            option: None,
        }
    }

    #[test]
    fn publish_creates_one_commit_and_noop_on_repeat() {
        let api = MockGitHub::new("acme", "demo");
        let project = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        let manifest = sample_manifest("1.0.0", vec![]);
        let manifest_path = project.path().join("demo.tuffbox.json");
        fs::write(&manifest_path, "{}").unwrap();
        let staged = stage_repo_tree(
            &manifest,
            &manifest_path,
            staging.path(),
            None,
            StageOptions {
                owner: Some("acme".into()),
                repo: Some("demo".into()),
                ..StageOptions::default()
            },
        )
        .unwrap();
        let first = publish_staged_tree(&api, "acme", "demo", None, &staged, "v1").unwrap();
        assert!(!first.noop);
        assert!(!first.commit_sha.is_empty());
        let inspected = crate::github_pack::inspect_public_pack(&api, "acme", "demo").unwrap();
        assert_eq!(inspected["fullName"], "acme/demo");
        assert_eq!(inspected["ready"], true);
        let files = api.head_files();
        assert!(files.contains_key("pack.toml"));
        assert!(files.contains_key("demo.tuffbox.json"));
        let again = publish_staged_tree(&api, "acme", "demo", None, &staged, "v1-again");
        assert!(matches!(again, Err(PublishError::NoOp)));
    }

    #[test]
    fn publish_rejects_non_fast_forward() {
        let api = MockGitHub::new("acme", "demo");
        let project = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        let manifest = sample_manifest("1.0.0", vec![]);
        let manifest_path = project.path().join("demo.tuffbox.json");
        fs::write(&manifest_path, "{}").unwrap();
        let staged = stage_repo_tree(
            &manifest,
            &manifest_path,
            staging.path(),
            None,
            StageOptions::default(),
        )
        .unwrap();
        publish_staged_tree(&api, "acme", "demo", None, &staged, "v1").unwrap();
        api.fail_next_ref_update();
        let staging2 = tempfile::tempdir().unwrap();
        let manifest2 = sample_manifest("1.1.0", vec![]);
        let staged2 = stage_repo_tree(
            &manifest2,
            &manifest_path,
            staging2.path(),
            None,
            StageOptions::default(),
        )
        .unwrap();
        let err = publish_staged_tree(&api, "acme", "demo", None, &staged2, "v2").unwrap_err();
        assert!(matches!(err, PublishError::Conflict(_)));
    }

    #[test]
    fn two_phase_publish_uploads_oversized_jar() {
        let api = MockGitHub::new("acme", "demo");
        let project = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join("mods")).unwrap();
        fs::write(project.path().join("mods/custom-lib.jar"), b"oversized-bytes").unwrap();
        let manifest = sample_manifest("2.0.0", vec![custom_jar_mod()]);
        let manifest_path = project.path().join("demo.tuffbox.json");
        fs::write(&manifest_path, "{}").unwrap();
        let staged = stage_repo_tree(
            &manifest,
            &manifest_path,
            staging.path(),
            None,
            StageOptions {
                owner: Some("acme".into()),
                repo: Some("demo".into()),
                custom_jar_git_limit: Some(4),
                ..StageOptions::default()
            },
        )
        .unwrap();
        assert!(!staged.pending_release_assets.is_empty());
        assert_eq!(staged.transport.status, "publishing");
        let result = publish_staged_tree(&api, "acme", "demo", None, &staged, "v2").unwrap();
        assert!(result.two_phase);
        assert!(api
            .uploaded_assets()
            .iter()
            .any(|n| n == "custom-lib.jar"));
        let files = api.head_files();
        let meta: RepoTransportMeta =
            serde_json::from_slice(&files[REPO_TRANSPORT_FILE]).unwrap();
        assert_eq!(meta.status, "ready");
        assert_eq!(meta.schema_version, TRANSPORT_SCHEMA_VERSION);
        assert!(!files.contains_key("mods/custom-lib.jar"));
    }
}
