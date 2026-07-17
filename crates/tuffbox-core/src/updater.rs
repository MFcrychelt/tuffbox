//! Update checking for mods (safe vs risky updates) with `pin` support.
//!
//! Inspired by packwiz's per-mod updaters (`modrinth/updater.go`,
//! `curseforge/updater.go`): each [`crate::manifest::ModSpec`] carries a
//! source; this module queries the matching provider for the latest compatible
//! version and classifies the change as `Safe` (patch/minor within the same
//! release channel) or `Risky` (new major version or a different release
//! channel such as `alpha`/`beta`). Mods with a `pin` flag (stored on the
//! manifest via [`crate::manifest::ModSpec::pinned`]) are never updated.

use crate::manifest::{ModSpec, SourceKind};
use crate::provider::{ContentProvider, ModrinthProvider, ProviderSearchQuery, VersionInfo};
use semver::Version;
use std::cmp::Ordering;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("provider error: {0}")]
    Provider(#[from] crate::provider::ProviderError),
    #[error("mod {0} has no project identifier to update from")]
    NoProjectId(String),
    #[error("failed to parse version {version}: {source}")]
    VersionParse {
        version: String,
        #[source]
        source: semver::Error,
    },
}

/// Whether an available update is safe to apply automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateRisk {
    /// Same major version, same release channel (e.g. 1.2.3 → 1.2.4).
    Safe,
    /// New minor/major, or a pre-release channel (beta/alpha). Needs a
    /// confirmation step before applying.
    Risky,
}

/// A pending update for a single mod.
#[derive(Debug, Clone)]
pub struct ModUpdate {
    pub mod_id: String,
    pub current_version: String,
    pub latest_version: String,
    pub latest_version_id: String,
    pub channel: Option<String>,
    pub risk: UpdateRisk,
    /// `true` when the user pinned the mod and it must not be updated.
    pub pinned: bool,
}

/// Returns the update classification for a given current → latest pair.
pub fn classify_update(
    current: &str,
    latest: &str,
    channel: Option<&str>,
) -> Result<UpdateRisk, UpdateError> {
    let cur = parse_lenient(current)?;
    let lat = parse_lenient(latest)?;
    match cur.major.cmp(&lat.major) {
        Ordering::Less => Ok(UpdateRisk::Risky),
        Ordering::Equal => {
            // Same major: a pre-release channel makes it risky.
            if matches!(channel, Some("alpha") | Some("beta")) {
                Ok(UpdateRisk::Risky)
            } else {
                Ok(UpdateRisk::Safe)
            }
        }
        Ordering::Greater => Ok(UpdateRisk::Safe), // already newer; treat as safe/no-op
    }
}

fn parse_lenient(v: &str) -> Result<Version, UpdateError> {
    match Version::parse(v) {
        Ok(ver) => Ok(ver),
        Err(_) => {
            // Try stripping a leading "v" or taking the first numeric-ish segment.
            let trimmed = v.trim_start_matches('v');
            Version::parse(trimmed).map_err(|source| UpdateError::VersionParse {
                version: v.to_string(),
                source,
            })
        }
    }
}

/// Checks a single mod for an available update.
///
/// Only Modrinth-sourced mods (with a `project_id`) are supported; others
/// return `Ok(None)`. Pinned mods return `Ok(Some(ModUpdate { pinned: true }))`
/// so the UI can show them as intentionally frozen.
pub fn check_mod_update(
    module: &ModSpec,
    minecraft_version: &str,
    loader: &str,
) -> Result<Option<ModUpdate>, UpdateError> {
    if module.pinned() {
        return Ok(Some(ModUpdate {
            mod_id: module.id.clone(),
            current_version: module.version.clone(),
            latest_version: module.version.clone(),
            latest_version_id: module
                .source
                .file_id
                .clone()
                .unwrap_or_default(),
            channel: None,
            risk: UpdateRisk::Safe,
            pinned: true,
        }));
    }

    if !matches!(module.source.kind, SourceKind::Modrinth) {
        return Ok(None);
    }
    let Some(project_id) = &module.source.project_id else {
        return Err(UpdateError::NoProjectId(module.id.clone()));
    };

    let provider = ModrinthProvider::new();
    let query = ProviderSearchQuery {
        minecraft_version: Some(minecraft_version.to_string()),
        loader: Some(loader.to_string()),
        ..Default::default()
    };
    let versions: Vec<VersionInfo> = provider.get_versions(project_id, &query)?;
    let Some(latest) = versions.first() else {
        return Ok(None);
    };

    let risk = classify_update(&module.version, &latest.version_number, latest.version_type.as_deref())?;
    if latest.version_number == module.version {
        return Ok(None);
    }

    Ok(Some(ModUpdate {
        mod_id: module.id.clone(),
        current_version: module.version.clone(),
        latest_version: latest.version_number.clone(),
        latest_version_id: latest.id.clone(),
        channel: latest.version_type.clone(),
        risk,
        pinned: false,
    }))
}

/// Checks every mod in the manifest, returning a map of `mod_id → update`.
pub fn check_manifest_updates(
    manifest: &crate::manifest::ProjectManifest,
) -> Result<HashMap<String, ModUpdate>, UpdateError> {
    let mut out = HashMap::new();
    let loader = manifest.loader.kind.as_str();
    for module in &manifest.mods {
        if let Some(update) = check_mod_update(module, &manifest.minecraft.version, loader)? {
            out.insert(module.id.clone(), update);
        }
    }
    Ok(out)
}

/// Applies the download metadata from a resolved latest version back onto a
/// [`ModSpec`], returning a new spec with bumped version/file/hashes. The
/// caller is responsible for persisting the manifest and re-materializing the
/// file via [`crate::mod_files`].
pub fn apply_update(module: &ModSpec, latest: &VersionInfo) -> ModSpec {
    let file = crate::provider::ProviderFileInfo::select_file_for_loader(latest, &module.source.kind.as_str());
    let mut updated = module.clone();
    updated.version = latest.version_number.clone();
    updated.source.file_id = Some(latest.id.clone());
    if let Some(f) = file {
        updated.source.url = Some(f.url.clone());
        updated.file_name = Some(f.filename.clone());
        updated.hashes = Some(crate::manifest::FileHashes {
            sha1: f.hashes.sha1.clone(),
            sha512: f.hashes.sha512.clone(),
        });
    }
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_update_is_safe() {
        assert_eq!(
            classify_update("1.2.3", "1.2.4", Some("release")).unwrap(),
            UpdateRisk::Safe
        );
    }

    #[test]
    fn major_update_is_risky() {
        assert_eq!(
            classify_update("1.2.3", "2.0.0", Some("release")).unwrap(),
            UpdateRisk::Risky
        );
    }

    #[test]
    fn beta_channel_is_risky() {
        assert_eq!(
            classify_update("1.2.3", "1.2.4", Some("beta")).unwrap(),
            UpdateRisk::Risky
        );
    }

    #[test]
    fn no_v_prefix_parses() {
        assert_eq!(
            classify_update("v1.2.3", "1.2.4", Some("release")).unwrap(),
            UpdateRisk::Safe
        );
    }
}
