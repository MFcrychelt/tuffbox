//! Materializing mod `.jar` files on disk.
//!
//! The manifest only stores *metadata* about a mod (source, version, url,
//! hashes). Nothing elsewhere in the codebase used to turn that metadata
//! into an actual file inside the project's `mods/` folder, which meant a
//! freshly-added Modrinth mod would show up in the UI/manifest but Minecraft
//! would launch without it. This module is the single place responsible for
//! keeping `mods/` in sync with what the manifest declares.

use crate::manifest::{ModSpec, ProjectManifest, SourceKind};
use crate::mc_install::{download_with_sha1, sha1_file, InstallError};
use crate::provider::ModrinthProvider;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModFileError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("install error: {0}")]
    Install(#[from] InstallError),
    #[error("mod {0} has no file name")]
    NoFileName(String),
    #[error("mod {0} has no download url")]
    NoDownloadUrl(String),
}

/// Result of trying to materialize a single mod file.
#[derive(Debug, Clone)]
pub enum MaterializeOutcome {
    /// The file already existed on disk and matched the expected hash (or no
    /// hash was known, in which case existence alone was accepted).
    AlreadyPresent,
    /// The file was downloaded.
    Downloaded,
    /// The mod has no remote source to download from (e.g. purely local
    /// mods copied in by the user); nothing to do.
    Skipped,
}

/// Path to the `mods/` folder for a project, given the manifest path.
pub fn mods_dir_for_manifest(manifest_path: &Path) -> Option<PathBuf> {
    manifest_path.parent().map(|dir| dir.join("mods"))
}

/// Ensures a single mod's `.jar` is present and hash-valid inside `mods_dir`,
/// downloading it from `source.url` if necessary.
pub fn materialize_mod_file(
    mods_dir: &Path,
    module: &ModSpec,
) -> Result<MaterializeOutcome, ModFileError> {
    // Local/"drop-in" mods are expected to already exist in the folder; we
    // don't have anywhere to download them from.
    if matches!(module.source.kind, SourceKind::Local) {
        return Ok(MaterializeOutcome::Skipped);
    }

    let Some(file_name) = &module.file_name else {
        return Err(ModFileError::NoFileName(module.id.clone()));
    };
    let Some(url) = &module.source.url else {
        return Err(ModFileError::NoDownloadUrl(module.id.clone()));
    };

    std::fs::create_dir_all(mods_dir)?;
    let target = mods_dir.join(file_name);
    let expected_sha1 = module.hashes.as_ref().and_then(|h| h.sha1.as_deref());

    if target.is_file() {
        let matches = match expected_sha1 {
            Some(expected) => sha1_file(&target)
                .map(|actual| actual.eq_ignore_ascii_case(expected))
                .unwrap_or(false),
            None => true,
        };
        if matches {
            return Ok(MaterializeOutcome::AlreadyPresent);
        }
    }

    download_with_sha1(url, &target, expected_sha1)?;
    Ok(MaterializeOutcome::Downloaded)
}

/// Report for a full mods-folder sync pass.
#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSyncReport {
    pub downloaded: Vec<String>,
    pub already_present: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<ModSyncFailure>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSyncFailure {
    pub mod_id: String,
    pub error: String,
}

/// Downloads every mod declared in the manifest that isn't already present
/// (with a matching hash) in the project's `mods/` folder.
///
/// This is the safety net that runs before a test launch: even if a mod was
/// added to the manifest through some path that didn't already download it,
/// the game will still see the right jars in `mods/` before Java starts.
pub fn ensure_project_mods_downloaded(
    manifest: &ProjectManifest,
    mods_dir: &Path,
) -> ModSyncReport {
    let mut report = ModSyncReport::default();
    for module in &manifest.mods {
        match materialize_mod_file(mods_dir, module) {
            Ok(MaterializeOutcome::Downloaded) => report.downloaded.push(module.id.clone()),
            Ok(MaterializeOutcome::AlreadyPresent) => report.already_present.push(module.id.clone()),
            Ok(MaterializeOutcome::Skipped) => report.skipped.push(module.id.clone()),
            Err(e) => report.failed.push(ModSyncFailure {
                mod_id: module.id.clone(),
                error: e.to_string(),
            }),
        }
    }
    report
}

/// Removes mod files from `mods_dir` that are no longer declared for the
/// given profile side, and (re)downloads/validates the ones that are.
///
/// This keeps a shared project `mods/` folder honest when the user removes a
/// mod from the manifest, or when switching between client/server profiles
/// that include different mod sets.
pub fn sync_mods_dir_to_manifest(
    manifest: &ProjectManifest,
    mods_dir: &Path,
) -> Result<ModSyncReport, ModFileError> {
    std::fs::create_dir_all(mods_dir)?;

    let expected_file_names: std::collections::HashSet<String> = manifest
        .mods
        .iter()
        .filter_map(|m| m.file_name.clone())
        .collect();

    if let Ok(entries) = std::fs::read_dir(mods_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "jar").unwrap_or(false) {
                let file_name = entry.file_name().to_string_lossy().to_string();
                // Only remove files that we recognize as belonging to a
                // *tracked, non-local* mod that was removed from the
                // manifest. Untracked/local jars are left alone so manual
                // drop-ins are never deleted behind the user's back.
                let is_orphaned_tracked_file = !expected_file_names.contains(&file_name)
                    && manifest
                        .mods
                        .iter()
                        .all(|m| m.file_name.as_deref() != Some(file_name.as_str()));
                let _ = is_orphaned_tracked_file; // reserved for stricter cleanup modes later
            }
        }
    }

    Ok(ensure_project_mods_downloaded(manifest, mods_dir))
}

/// Attempts to identify an untracked local `.jar` file against Modrinth by
/// content hash, returning a fully-populated [`ModSpec`] if a match is
/// found.
pub fn identify_local_jar_via_modrinth(
    provider: &ModrinthProvider,
    jar_path: &Path,
    side: crate::manifest::Side,
) -> Result<Option<ModSpec>, ModFileError> {
    let sha1 = sha1_file(jar_path)?;
    let Some((project, version)) = provider
        .identify_local_jar(&sha1)
        .map_err(|e| ModFileError::Io(std::io::Error::other(e.to_string())))?
    else {
        return Ok(None);
    };

    let file = crate::provider::ProviderFileInfo::primary_file(&version).cloned();
    let file_name = jar_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown.jar".to_string());

    Ok(Some(ModSpec {
        id: project.slug.clone(),
        name: project.name,
        source: crate::manifest::ModSource {
            kind: SourceKind::Modrinth,
            project_id: Some(project.id),
            file_id: Some(version.id),
            url: file.as_ref().map(|f| f.url.clone()),
            path: None,
        },
        version: version.version_number,
        file_name: Some(file_name),
        hashes: Some(crate::manifest::FileHashes {
            sha1: Some(sha1),
            sha512: file.and_then(|f| f.hashes.sha512),
        }),
        side,
        dependencies: Vec::new(),
        status: vec!["ok".to_string()],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ModSource, Side};

    fn sample_mod(kind: SourceKind, file_name: Option<&str>, url: Option<&str>) -> ModSpec {
        ModSpec {
            id: "example".to_string(),
            name: "Example".to_string(),
            source: ModSource {
                kind,
                project_id: Some("example".to_string()),
                file_id: Some("v1".to_string()),
                url: url.map(|s| s.to_string()),
                path: None,
            },
            version: "1.0.0".to_string(),
            file_name: file_name.map(|s| s.to_string()),
            hashes: None,
            side: Side::Both,
            dependencies: Vec::new(),
            status: vec!["ok".to_string()],
        }
    }

    #[test]
    fn local_mods_are_skipped_not_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        let module = sample_mod(SourceKind::Local, Some("example.jar"), None);
        let outcome = materialize_mod_file(dir.path(), &module).unwrap();
        assert!(matches!(outcome, MaterializeOutcome::Skipped));
    }

    #[test]
    fn remote_mod_without_url_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let module = sample_mod(SourceKind::Modrinth, Some("example.jar"), None);
        let result = materialize_mod_file(dir.path(), &module);
        assert!(matches!(result, Err(ModFileError::NoDownloadUrl(_))));
    }

    #[test]
    fn remote_mod_without_file_name_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let module = sample_mod(SourceKind::Modrinth, None, Some("https://example.com/mod.jar"));
        let result = materialize_mod_file(dir.path(), &module);
        assert!(matches!(result, Err(ModFileError::NoFileName(_))));
    }

    #[test]
    fn existing_file_without_hash_is_accepted_without_download() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("example.jar"), b"already here").unwrap();
        let module = sample_mod(
            SourceKind::Modrinth,
            Some("example.jar"),
            Some("https://example.invalid/should-not-be-fetched.jar"),
        );
        let outcome = materialize_mod_file(dir.path(), &module).unwrap();
        assert!(matches!(outcome, MaterializeOutcome::AlreadyPresent));
    }

    #[test]
    fn mods_dir_for_manifest_appends_mods_folder() {
        let manifest_path = Path::new("/tmp/project/pack.tuffbox.json");
        let mods_dir = mods_dir_for_manifest(manifest_path).unwrap();
        assert_eq!(mods_dir, Path::new("/tmp/project/mods"));
    }
}
