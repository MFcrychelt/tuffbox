//! `tuffbox://` deep links.
//!
//! Two verbs, both funnelled through one park-and-drain channel:
//! - `tuffbox://install?repo=owner/repo` — a GitHub pack (also `gh:owner/repo:ref`).
//! - `tuffbox://launch?project=<path>` — a local project to open.
//!
//! The URL arrives either as the launch argument that started the process (cold
//! start) or through a second instance's forwarded argv while the app is already
//! running. Both paths land in [`handle_deep_link_url`], which parks a verdict
//! the frontend drains via [`take_pending_deep_link`] and emits
//! `tuffbox:deep-link` so an already-mounted UI reacts at once.
//!
//! Validation is strict *because* the URL comes from outside the app: a foreign
//! scheme is ignored rather than reported, an unknown verb is parked as
//! [`PendingDeepLink::Unknown`] so the user learns the link was wrong, and every
//! parameter is checked against the same resolver the manual flows use before it
//! is parked. Neither verb acts on its own — the frontend confirms, so a link
//! from a web page cannot start anything by itself.

use serde::Serialize;
use std::path::Path;
use std::sync::LazyLock;
use std::sync::Mutex;
use tuffbox_core::github_pack::parse_github_source;
use tuffbox_core::ProjectManifest;

/// Verdict for one deep link, drained exactly once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PendingDeepLink {
    Install { verdict: InstallVerdict },
    Launch { verdict: LaunchVerdict },
    /// A `tuffbox://` link with a verb we do not know: surfaced as a toast,
    /// never acted on.
    Unknown { raw: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum InstallVerdict {
    /// Validated and normalized to what `github_pack_inspect_source` accepts.
    Valid { repo: String },
    /// Not a GitHub repo — surfaced as a toast by the frontend, never installed.
    Invalid { raw: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum LaunchVerdict {
    /// Absolute manifest path plus the project's display name.
    Valid { project: String, name: String },
    Invalid { raw: String, reason: String },
}

/// Parked link that arrived before the UI was ready — or one waiting for the
/// user to confirm it. Drained on mount (see [`take_pending_deep_link`]).
static PENDING_DEEP_LINK: LazyLock<Mutex<Option<PendingDeepLink>>> =
    LazyLock::new(|| Mutex::new(None));

/// Splits `tuffbox://<verb>?<params>` into the verb and its query pairs.
///
/// `None` means "not our scheme" — the caller ignores such links entirely.
/// Windows fills the host slot with the path (`tuffbox://install?…` parses as
/// host `install`), while other writers spell it `tuffbox:///install?…`; both
/// are accepted.
fn scheme_parts(url: &str) -> Option<(String, Vec<(String, String)>)> {
    let parsed = url::Url::parse(url).ok()?;
    if parsed.scheme() != "tuffbox" {
        return None;
    }
    let path = format!("{}{}", parsed.host_str().unwrap_or_default(), parsed.path());
    let verb = path.trim_matches('/').to_ascii_lowercase();
    let params = parsed
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    Some((verb, params))
}

/// First non-empty value of `key`, percent-decoded by the URL parser.
fn param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
}

/// Classifies the `repo` parameter against the same parser the manual import
/// flow uses.
fn install_verdict(params: &[(String, String)]) -> InstallVerdict {
    let Some(raw) = param(params, "repo") else {
        return InstallVerdict::Invalid {
            raw: String::new(),
        };
    };
    match parse_github_source(raw) {
        Ok(source) => InstallVerdict::Valid {
            // Keep a pinned ref when the link carries one; `gh:` is the
            // spelled-out form both parse and inspect accept.
            repo: match source.git_ref {
                Some(git_ref) => format!("gh:{}/{}:{}", source.owner, source.repo, git_ref),
                None => format!("{}/{}", source.owner, source.repo),
            },
        },
        Err(_) => InstallVerdict::Invalid {
            raw: raw.to_string(),
        },
    }
}

/// Validates the `project` parameter the same way `--launch` does: resolve it to
/// a manifest, then prove the manifest parses. Anything else is reported, not
/// parked as a launch request.
fn launch_verdict(params: &[(String, String)]) -> LaunchVerdict {
    let Some(raw) = param(params, "project") else {
        return LaunchVerdict::Invalid {
            raw: String::new(),
            reason: "missing project parameter".to_string(),
        };
    };
    if !Path::new(raw).is_absolute() {
        return LaunchVerdict::Invalid {
            raw: raw.to_string(),
            reason: "project path must be absolute".to_string(),
        };
    }
    let manifest = match crate::resolve_manifest_path(raw) {
        Ok(manifest) => manifest,
        Err(reason) => {
            return LaunchVerdict::Invalid {
                raw: raw.to_string(),
                reason,
            }
        }
    };
    match ProjectManifest::load_from_path(&manifest) {
        Ok(found) => LaunchVerdict::Valid {
            project: manifest.to_string_lossy().into_owned(),
            name: found.project.name,
        },
        Err(error) => LaunchVerdict::Invalid {
            raw: raw.to_string(),
            reason: format!("manifest unreadable: {error}"),
        },
    }
}

/// Classifies one `tuffbox://` URL. `None` = not our scheme (caller ignores it).
pub fn parse_deep_link(url: &str) -> Option<PendingDeepLink> {
    let (verb, params) = scheme_parts(url)?;
    Some(match verb.as_str() {
        "install" => PendingDeepLink::Install {
            verdict: install_verdict(&params),
        },
        "launch" => PendingDeepLink::Launch {
            verdict: launch_verdict(&params),
        },
        _ => PendingDeepLink::Unknown {
            raw: url.to_string(),
        },
    })
}

/// Validates and parks one incoming link. Called for cold start (launch args),
/// warm start (second-instance argv) and runtime open-url events — always
/// exactly once per physical link.
pub fn handle_deep_link_url(app: &tauri::AppHandle, url: &str) {
    use tauri::{Emitter, Manager};

    let Some(link) = parse_deep_link(url) else {
        return;
    };
    if let Ok(mut slot) = PENDING_DEEP_LINK.lock() {
        *slot = Some(link.clone());
    }
    let _ = app.emit("tuffbox:deep-link", link);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Drain one parked link (frontend mount / event follow-up).
#[tauri::command(rename_all = "camelCase")]
pub fn take_pending_deep_link() -> Option<PendingDeepLink> {
    PENDING_DEEP_LINK.lock().ok().and_then(|mut slot| slot.take())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn install(url: &str) -> Option<PendingDeepLink> {
        parse_deep_link(url)
    }

    #[test]
    fn ignores_foreign_schemes_and_accepts_both_slash_spellings() {
        assert_eq!(install("https://github.com/acme/cool-pack"), None);
        assert_eq!(install("not a url"), None);
        assert!(matches!(
            install("tuffbox://install/?repo=acme/cool-pack"),
            Some(PendingDeepLink::Install { .. })
        ));
    }

    #[test]
    fn classifies_install_links() {
        assert_eq!(
            install("tuffbox://install?repo=acme/cool-pack"),
            Some(PendingDeepLink::Install {
                verdict: InstallVerdict::Valid {
                    repo: "acme/cool-pack".into()
                }
            })
        );
        // Percent-encoded owner/repo and a full GitHub URL keeping its ref.
        assert_eq!(
            install("tuffbox://install?repo=acme%2Fcool-pack"),
            Some(PendingDeepLink::Install {
                verdict: InstallVerdict::Valid {
                    repo: "acme/cool-pack".into()
                }
            })
        );
        assert_eq!(
            install(
                "tuffbox://install?repo=https%3A%2F%2Fgithub.com%2Facme%2Fcool-pack%2Freleases%2Ftag%2Fv1.2.3"
            ),
            Some(PendingDeepLink::Install {
                verdict: InstallVerdict::Valid {
                    repo: "gh:acme/cool-pack:v1.2.3".into()
                }
            })
        );
        assert_eq!(
            install("tuffbox://install?repo=not+a+repo"),
            Some(PendingDeepLink::Install {
                verdict: InstallVerdict::Invalid {
                    raw: "not a repo".into()
                }
            })
        );
        // Our verb, missing parameter: reported as a broken install link rather
        // than silently ignored.
        assert_eq!(
            install("tuffbox://install"),
            Some(PendingDeepLink::Install {
                verdict: InstallVerdict::Invalid { raw: String::new() }
            })
        );
    }

    #[test]
    fn reports_unknown_verbs_instead_of_acting_on_them() {
        assert_eq!(
            install("tuffbox://join-group?id=42"),
            Some(PendingDeepLink::Unknown {
                raw: "tuffbox://join-group?id=42".into()
            })
        );
        assert!(matches!(
            install("tuffbox://"),
            Some(PendingDeepLink::Unknown { .. })
        ));
    }

    #[test]
    fn launch_requires_an_absolute_path_to_a_real_project() {
        let relative = launch_verdict(&params(&[("project", "packs/demo")]));
        assert!(matches!(
            relative,
            LaunchVerdict::Invalid { ref reason, .. } if reason.contains("absolute")
        ));

        let missing = launch_verdict(&params(&[("project", "Z:/definitely/not/here")]));
        assert!(matches!(missing, LaunchVerdict::Invalid { .. }));

        let absent = launch_verdict(&params(&[]));
        assert!(matches!(
            absent,
            LaunchVerdict::Invalid { ref reason, .. } if reason.contains("missing")
        ));
    }

    #[test]
    fn launch_accepts_a_project_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("project.tuffbox.json"),
            r#"{
              "schemaVersion": "0.1.0",
              "project": { "id": "demo", "name": "Demo Pack", "version": "0.1.0" },
              "minecraft": { "version": "1.20.1" },
              "loader": { "type": "fabric", "version": "0.15.11" },
              "mods": []
            }"#,
        )
        .expect("write manifest");

        let verdict = launch_verdict(&params(&[("project", &dir.path().to_string_lossy())]));
        match verdict {
            LaunchVerdict::Valid { project, name } => {
                assert_eq!(name, "Demo Pack");
                assert!(project.ends_with("project.tuffbox.json"), "got {project}");
            }
            other => panic!("expected a valid launch verdict, got {other:?}"),
        }
    }

    #[test]
    fn launch_rejects_a_directory_without_a_manifest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let verdict = launch_verdict(&params(&[("project", &dir.path().to_string_lossy())]));
        assert!(matches!(
            verdict,
            LaunchVerdict::Invalid { ref reason, .. } if reason.contains("not found")
        ));
    }

    #[test]
    fn launch_rejects_a_manifest_that_does_not_parse() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("project.tuffbox.json"), b"{ not json").expect("write");
        let verdict = launch_verdict(&params(&[("project", &dir.path().to_string_lossy())]));
        assert!(matches!(
            verdict,
            LaunchVerdict::Invalid { ref reason, .. } if reason.contains("unreadable")
        ));
    }

    #[test]
    fn launch_links_parse_end_to_end() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("project.tuffbox.json"),
            r#"{
              "schemaVersion": "0.1.0",
              "project": { "id": "demo", "name": "Demo Pack", "version": "0.1.0" },
              "minecraft": { "version": "1.20.1" },
              "loader": { "type": "fabric", "version": "0.15.11" },
              "mods": []
            }"#,
        )
        .expect("write manifest");

        let url = format!(
            "tuffbox://launch?project={}",
            url::form_urlencoded::byte_serialize(dir.path().to_string_lossy().as_bytes()).collect::<String>()
        );
        assert!(matches!(
            parse_deep_link(&url),
            Some(PendingDeepLink::Launch {
                verdict: LaunchVerdict::Valid { .. }
            })
        ));
    }
}
