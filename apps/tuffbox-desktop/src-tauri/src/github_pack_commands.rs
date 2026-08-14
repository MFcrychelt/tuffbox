use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::Emitter;
use tuffbox_core::github_pack::{
    apply_managed_files, commit_sha, diff_manifests, extract_github_tarball, import_repo_tree,
    inspect_public_pack, managed_from_transport, parse_github_source, pin_or_check_signer,
    publish_staged_tree, remove_obsolete_content_files, stage_repo_tree, update_available,
    verify_manifest_local_hashes, LiveGitHubApi, LocalTransportMeta, PublishError, StageOptions,
    TransportKind, TRANSPORT_SCHEMA_VERSION,
};
use tuffbox_core::{ProjectManifest, SnapshotStore};

use crate::github_auth::{author_signing_key, stored_author_token};
use crate::helpers::{
    auto_snapshot_detailed, auto_snapshot_with_managed, persist_lockfile_for_manifest, save_manifest,
};

const APP_USER_AGENT: &str = "TuffBox-IDE/0.1";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubPackStagePreview {
    pub pack_version: String,
    pub manifest_file: String,
    pub file_count: usize,
    pub managed_files: Vec<String>,
    pub share_url: Option<String>,
    pub content_digest: String,
    pub has_external_assets: bool,
}

#[tauri::command(rename_all = "camelCase")]
pub fn github_pack_parse_source(source: String) -> Result<serde_json::Value, String> {
    let src = parse_github_source(&source).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "owner": src.owner,
        "repo": src.repo,
        "ref": src.git_ref,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub fn github_pack_inspect_source(source: String) -> Result<serde_json::Value, String> {
    let src = parse_github_source(&source).map_err(|e| e.to_string())?;
    let client = anonymous_client()?;
    inspect_public_pack(&client, &src.owner, &src.repo).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn github_pack_stage_preview(path: String) -> Result<GithubPackStagePreview, String> {
    let manifest_path = PathBuf::from(&path);
    let manifest = ProjectManifest::load_from_path(&manifest_path).map_err(|e| e.to_string())?;
    let staging = tempfile::tempdir().map_err(|e| e.to_string())?;
    let staged = stage_repo_tree(
        &manifest,
        &manifest_path,
        staging.path(),
        None,
        StageOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    Ok(GithubPackStagePreview {
        pack_version: staged.transport.pack_version,
        manifest_file: staged.transport.manifest_file,
        file_count: staged.files.len(),
        managed_files: staged.transport.managed_files,
        share_url: None,
        content_digest: staged.transport.content_digest,
        has_external_assets: !staged.pending_release_assets.is_empty(),
    })
}

#[tauri::command(rename_all = "camelCase")]
pub async fn github_pack_install(
    app: tauri::AppHandle,
    source: String,
    target_dir: String,
    instance_name: Option<String>,
) -> Result<serde_json::Value, String> {
    let parsed = parse_github_source(&source).map_err(|e| e.to_string())?;
    let git_ref = parsed.git_ref.clone().unwrap_or_else(|| "HEAD".into());
    let client = LiveGitHubApi::new(stored_author_token()).map_err(|e| e.to_string())?;
    let commit = commit_sha(&client, &parsed.owner, &parsed.repo, &git_ref)
        .unwrap_or_else(|_| git_ref.clone());
    let url = format!(
        "https://api.github.com/repos/{}/{}/tarball/{}",
        parsed.owner, parsed.repo, commit
    );
    let http = reqwest::Client::new();
    let mut req = http
        .get(&url)
        .header("User-Agent", APP_USER_AGENT)
        .header("Accept", "application/vnd.github+json");
    if let Some(token) = stored_author_token() {
        req = req.bearer_auth(token);
    }
    let bytes = req
        .send()
        .await
        .map_err(|e| format!("GitHub tarball download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GitHub tarball download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    tokio::task::spawn_blocking(move || {
        install_from_tarball_bytes(
            &app,
            &bytes,
            &parsed.owner,
            &parsed.repo,
            parsed.git_ref.as_deref(),
            Some(&commit),
            &target_dir,
            instance_name,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

fn install_from_tarball_bytes(
    app: &tauri::AppHandle,
    bytes: &[u8],
    owner: &str,
    repo: &str,
    git_ref: Option<&str>,
    commit_sha_val: Option<&str>,
    target_dir: &str,
    instance_name: Option<String>,
) -> Result<serde_json::Value, String> {
    let extract_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    extract_github_tarball(bytes, extract_dir.path()).map_err(|e| e.to_string())?;
    let (mut manifest, transport) =
        import_repo_tree(extract_dir.path()).map_err(|e| e.to_string())?;
    if transport.as_ref().is_some_and(|t| t.status == "publishing") {
        return Err("This pack is still publishing oversized assets. Try again when it is ready.".into());
    }
    if let Some(incoming) = transport.as_ref().and_then(|t| t.signer_public_key.as_deref()) {
        pin_or_check_signer(None, incoming).map_err(|e| e.to_string())?;
    }
    verify_manifest_local_hashes(extract_dir.path(), &manifest).map_err(|e| e.to_string())?;
    if let Some(name) = instance_name.filter(|n| !n.trim().is_empty()) {
        manifest.project.name = name.clone();
        manifest.project.id = crate::helpers::slugify_project_name(&name);
    }
    let instance_dir = PathBuf::from(target_dir).join(&manifest.project.id);
    fs::create_dir_all(&instance_dir).map_err(|e| e.to_string())?;
    copy_tree(extract_dir.path(), &instance_dir)?;
    let manifest_path = instance_dir.join(format!("{}.tuffbox.json", manifest.project.id));
    save_manifest(&manifest_path, &manifest).map_err(|e| e.to_string())?;
    let _ = persist_lockfile_for_manifest(&manifest_path, &manifest);
    let _ = auto_snapshot_detailed(
        &manifest_path,
        "github_pack_install",
        &[],
        &["Imported GitHub pack".into()],
    );
    let local = LocalTransportMeta {
        schema_version: TRANSPORT_SCHEMA_VERSION,
        kind: TransportKind::Github,
        repo: format!("{owner}/{repo}"),
        git_ref: git_ref.map(|s| s.to_string()),
        manifest_file: transport
            .as_ref()
            .map(|t| t.manifest_file.clone())
            .unwrap_or_else(|| format!("{}.tuffbox.json", manifest.project.id)),
        installed_version: manifest.project.version.clone(),
        installed_commit_sha: commit_sha_val.map(|s| s.to_string()),
        installed_at: Some(tuffbox_core::time_util::rfc3339_now()),
        pinned_signer_public_key: transport.and_then(|t| t.signer_public_key),
    };
    write_local_transport(&instance_dir, &local)?;
    crate::download_project_mods_tracked(app, &manifest_path, &manifest, None, true);
    Ok(serde_json::json!({
        "path": manifest_path.to_string_lossy(),
        "name": manifest.project.name,
        "modCount": manifest.mods.len(),
        "provider": "github",
        "repo": format!("{owner}/{repo}"),
    }))
}

fn write_local_transport(instance_dir: &Path, local: &LocalTransportMeta) -> Result<(), String> {
    let transport_path = instance_dir.join(".tuffbox").join("transport.json");
    if let Some(parent) = transport_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(
        &transport_path,
        serde_json::to_vec_pretty(local).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

fn load_local_transport(project_dir: &Path) -> Result<LocalTransportMeta, String> {
    let transport_path = project_dir.join(".tuffbox").join("transport.json");
    serde_json::from_str(&fs::read_to_string(transport_path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    for entry in fs::read_dir(from).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src = entry.path();
        let dest = to.join(entry.file_name());
        if src.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            copy_tree(&src, &dest)?;
        } else if src.is_file() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&src, &dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn anonymous_client() -> Result<LiveGitHubApi, String> {
    LiveGitHubApi::new(stored_author_token()).map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub fn github_pack_check_update(path: String) -> Result<serde_json::Value, String> {
    let manifest_path = PathBuf::from(&path);
    let project_dir = manifest_path.parent().ok_or("manifest has no parent")?;
    let transport_path = project_dir.join(".tuffbox").join("transport.json");
    if !transport_path.is_file() {
        return Ok(serde_json::json!({ "updateAvailable": false, "reason": "not a GitHub pack" }));
    }
    let local = load_local_transport(project_dir)?;
    let parsed = parse_github_source(&local.repo).map_err(|e| e.to_string())?;
    let git_ref = local
        .git_ref
        .clone()
        .unwrap_or_else(|| "HEAD".into());
    let client = anonymous_client()?;
    let remote_sha = commit_sha(&client, &parsed.owner, &parsed.repo, &git_ref).map_err(|e| e.to_string())?;
    let manifest = ProjectManifest::load_from_path(&manifest_path).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "updateAvailable": update_available(local.installed_commit_sha.as_deref(), &remote_sha),
        "repo": local.repo,
        "installedVersion": manifest.project.version,
        "installedCommitSha": local.installed_commit_sha,
        "remoteCommitSha": remote_sha,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn github_pack_preview_update(path: String) -> Result<serde_json::Value, String> {
    let manifest_path = PathBuf::from(&path);
    let project_dir = manifest_path
        .parent()
        .ok_or("manifest has no parent")?
        .to_path_buf();
    let local = load_local_transport(&project_dir)?;
    let parsed = parse_github_source(&local.repo).map_err(|e| e.to_string())?;
    let git_ref = local.git_ref.clone().unwrap_or_else(|| "HEAD".into());
    let (bytes, remote_sha) = download_tarball(&parsed.owner, &parsed.repo, &git_ref).await?;
    tokio::task::spawn_blocking(move || {
        let extract_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        extract_github_tarball(&bytes, extract_dir.path()).map_err(|e| e.to_string())?;
        let (incoming, transport) =
            import_repo_tree(extract_dir.path()).map_err(|e| e.to_string())?;
        if transport.as_ref().is_some_and(|t| t.status == "publishing") {
            return Err("This pack is still publishing oversized assets. Try again when it is ready.".into());
        }
        let incoming_key = transport.as_ref().and_then(|t| t.signer_public_key.clone());
        let signer_state = match incoming_key.as_deref() {
            None => "unsigned",
            Some(key) => match pin_or_check_signer(local.pinned_signer_public_key.as_deref(), key) {
                Ok(_) => "ok",
                Err(_) => "changed",
            },
        };
        let current = ProjectManifest::load_from_path(&manifest_path).map_err(|e| e.to_string())?;
        let diff = diff_manifests(&current, &incoming);
        Ok(serde_json::json!({
            "repo": local.repo,
            "installedVersion": current.project.version,
            "incomingVersion": incoming.project.version,
            "remoteCommitSha": remote_sha,
            "requiresFullReinstall": diff.requires_full_reinstall(),
            "signerState": signer_state,
            "changes": diff.changes,
            "customFiles": diff.changes.iter().any(|c| matches!(
                c,
                tuffbox_core::github_pack::PackChange::ModAdded { origin: tuffbox_core::github_pack::TrustOrigin::Custom, .. }
                | tuffbox_core::github_pack::PackChange::ModBumped { origin: tuffbox_core::github_pack::TrustOrigin::Custom, .. }
            )),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command(rename_all = "camelCase")]
pub async fn github_pack_apply_update(app: tauri::AppHandle, path: String) -> Result<serde_json::Value, String> {
    let manifest_path = PathBuf::from(&path);
    let project_dir = manifest_path
        .parent()
        .ok_or("manifest has no parent")?
        .to_path_buf();
    let local = load_local_transport(&project_dir)?;
    let parsed = parse_github_source(&local.repo).map_err(|e| e.to_string())?;
    let git_ref = local.git_ref.clone().unwrap_or_else(|| "HEAD".into());
    let (bytes, remote_sha) = download_tarball(&parsed.owner, &parsed.repo, &git_ref).await?;
    tokio::task::spawn_blocking(move || {
        let extract_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        extract_github_tarball(&bytes, extract_dir.path()).map_err(|e| e.to_string())?;
        let (incoming, transport) =
            import_repo_tree(extract_dir.path()).map_err(|e| e.to_string())?;
        if transport.as_ref().is_some_and(|t| t.status == "publishing") {
            return Err("This pack is still publishing oversized assets. Try again when it is ready.".into());
        }
        if let Some(incoming_key) = transport.as_ref().and_then(|t| t.signer_public_key.as_deref()) {
            pin_or_check_signer(local.pinned_signer_public_key.as_deref(), incoming_key)
                .map_err(|e| e.to_string())?;
        }
        verify_manifest_local_hashes(extract_dir.path(), &incoming).map_err(|e| e.to_string())?;
        let current = ProjectManifest::load_from_path(&manifest_path).map_err(|e| e.to_string())?;
        let previous = managed_from_transport(
            serde_json::from_str(
                &fs::read_to_string(project_dir.join(".tuffbox").join("repo-transport.json"))
                    .unwrap_or_default(),
            )
            .ok()
            .as_ref(),
        );
        let new_managed = managed_from_transport(transport.as_ref());
        let snapshot = auto_snapshot_with_managed(
            &manifest_path,
            "github_pack_update",
            &[],
            &["GitHub pack update".into()],
            previous.iter().map(PathBuf::from).collect(),
        )
        .map_err(|e| e.to_string())?;
        let apply_result = apply_managed_files(
            extract_dir.path(),
            &project_dir,
            &previous,
            &new_managed,
        );
        if let Err(e) = apply_result {
            let _ = SnapshotStore::new(&project_dir).rollback(&snapshot.id);
            return Err(e.to_string());
        }
        let diff = diff_manifests(&current, &incoming);
        if diff.requires_full_reinstall() {
            if let Err(e) = remove_obsolete_content_files(&project_dir, &current, &incoming) {
                let _ = SnapshotStore::new(&project_dir).rollback(&snapshot.id);
                return Err(e.to_string());
            }
        }
        if let Err(e) = save_manifest(&manifest_path, &incoming) {
            let _ = SnapshotStore::new(&project_dir).rollback(&snapshot.id);
            return Err(e.to_string());
        }
        let _ = persist_lockfile_for_manifest(&manifest_path, &incoming);
        if let Err(e) = verify_manifest_local_hashes(&project_dir, &incoming) {
            let _ = SnapshotStore::new(&project_dir).rollback(&snapshot.id);
            return Err(e.to_string());
        }
        let local = LocalTransportMeta {
            schema_version: TRANSPORT_SCHEMA_VERSION,
            kind: TransportKind::Github,
            repo: local.repo,
            git_ref: local.git_ref,
            manifest_file: transport
                .as_ref()
                .map(|t| t.manifest_file.clone())
                .unwrap_or(local.manifest_file),
            installed_version: incoming.project.version.clone(),
            installed_commit_sha: Some(remote_sha),
            installed_at: Some(tuffbox_core::time_util::rfc3339_now()),
            pinned_signer_public_key: transport
                .and_then(|t| t.signer_public_key)
                .or(local.pinned_signer_public_key),
        };
        write_local_transport(&project_dir, &local)?;
        crate::download_project_mods_tracked(&app, &manifest_path, &incoming, None, true);
        Ok(serde_json::json!({
            "ok": true,
            "snapshotId": snapshot.id,
            "version": incoming.project.version,
            "changes": diff.changes,
            "requiresFullReinstall": diff.requires_full_reinstall(),
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

async fn download_tarball(
    owner: &str,
    repo: &str,
    git_ref: &str,
) -> Result<(Vec<u8>, String), String> {
    let client = anonymous_client()?;
    let commit = commit_sha(&client, owner, repo, git_ref).unwrap_or_else(|_| git_ref.to_string());
    let url = format!("https://api.github.com/repos/{owner}/{repo}/tarball/{commit}");
    let http = reqwest::Client::new();
    let mut req = http
        .get(&url)
        .header("User-Agent", APP_USER_AGENT)
        .header("Accept", "application/vnd.github+json");
    if let Some(token) = stored_author_token() {
        req = req.bearer_auth(token);
    }
    let bytes = req
        .send()
        .await
        .map_err(|e| format!("GitHub tarball download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GitHub tarball download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    Ok((bytes.to_vec(), commit))
}

#[tauri::command(rename_all = "camelCase")]
pub fn github_pack_publish(
    app: tauri::AppHandle,
    path: String,
    repository: String,
) -> Result<serde_json::Value, String> {
    let token = stored_author_token().ok_or_else(|| {
        "GitHub author token missing. Log in with device flow or paste a PAT in Settings.".to_string()
    })?;
    let src = parse_github_source(&repository).map_err(|e| e.to_string())?;
    let signer = author_signing_key().ok();
    let manifest_path = PathBuf::from(&path);
    let manifest = ProjectManifest::load_from_path(&manifest_path).map_err(|e| e.to_string())?;
    semver::Version::parse(manifest.project.version.trim()).map_err(|_| {
        format!(
            "pack version {:?} is not semver (expected like 1.2.3)",
            manifest.project.version
        )
    })?;
    let _ = persist_lockfile_for_manifest(&manifest_path, &manifest);
    let _ = app.emit(
        "github-pack-progress",
        serde_json::json!({ "phase": "staging" }),
    );
    let staging = tempfile::tempdir().map_err(|e| e.to_string())?;
    let staged = stage_repo_tree(
        &manifest,
        &manifest_path,
        staging.path(),
        None,
        StageOptions {
            owner: Some(src.owner.clone()),
            repo: Some(src.repo.clone()),
            signer,
            ..StageOptions::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "github-pack-progress",
        serde_json::json!({ "phase": "commit" }),
    );
    let client = LiveGitHubApi::new(Some(token)).map_err(|e| e.to_string())?;
    if !staged.pending_release_assets.is_empty() {
        let _ = app.emit(
            "github-pack-progress",
            serde_json::json!({ "phase": "assets" }),
        );
    }
    match publish_staged_tree(
        &client,
        &src.owner,
        &src.repo,
        src.git_ref.as_deref(),
        &staged,
        &format!(
            "TuffBox pack {} {}",
            manifest.project.name, manifest.project.version
        ),
    ) {
        Ok(result) => {
            let _ = app.emit(
                "github-pack-progress",
                serde_json::json!({ "phase": "done" }),
            );
            Ok(serde_json::json!({
                "ok": true,
                "commitSha": result.commit_sha,
                "branch": result.branch,
                "shareUrl": result.share_url,
                "releaseUrl": result.release_url,
                "twoPhase": result.two_phase,
                "preview": {
                    "packVersion": staged.transport.pack_version,
                    "fileCount": staged.files.len(),
                    "managedFiles": staged.transport.managed_files,
                },
            }))
        }
        Err(PublishError::NoOp) => Ok(serde_json::json!({
            "ok": true,
            "noop": true,
            "shareUrl": format!("https://github.com/{}/{}", src.owner, src.repo),
            "message": "Nothing to publish — remote already has this pack.",
        })),
        Err(PublishError::Conflict(msg)) => Ok(serde_json::json!({
            "ok": false,
            "conflict": true,
            "message": msg,
        })),
        Err(e) => Err(e.to_string()),
    }
}
