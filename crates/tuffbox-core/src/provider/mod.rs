pub mod local;
pub mod modrinth;

pub use local::LocalJarProvider;
pub use modrinth::ModrinthProvider;

use crate::manifest::{DependencyKind, ModDependencySpec};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),
    #[error("failed to parse response: {0}")]
    Parse(#[source] serde_json::Error),
    #[error("provider returned status {status}: {message}")]
    Api { status: u16, message: String },
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[error("version not found: {0}")]
    VersionNotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported operation for provider")]
    Unsupported,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSearchQuery {
    pub query: Option<String>,
    pub minecraft_version: Option<String>,
    pub loader: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub project_type: String,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub files: Vec<ProviderFileInfo>,
    pub dependencies: Vec<ProviderDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderFileInfo {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub hashes: ProviderFileHashes,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderFileHashes {
    pub sha1: Option<String>,
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub dependency_type: String,
}

pub trait ContentProvider {
    fn search(&self, query: &ProviderSearchQuery) -> Result<Vec<ProjectInfo>, ProviderError>;
    fn get_project(&self, id: &str) -> Result<ProjectInfo, ProviderError>;
    fn get_versions(
        &self,
        id: &str,
        query: &ProviderSearchQuery,
    ) -> Result<Vec<VersionInfo>, ProviderError>;
    fn get_file(&self, version_id: &str, filename: &str)
        -> Result<ProviderFileInfo, ProviderError>;
    fn resolve_dependencies(
        &self,
        version_id: &str,
    ) -> Result<Vec<ModDependencySpec>, ProviderError>;
}

impl ProviderFileInfo {
    pub fn primary_file(version: &VersionInfo) -> Option<&ProviderFileInfo> {
        version
            .files
            .iter()
            .find(|f| f.primary)
            .or(version.files.first())
    }
}

pub fn provider_dependency_to_spec(dep: ProviderDependency) -> Option<ModDependencySpec> {
    let kind = match dep.dependency_type.as_str() {
        "required" => DependencyKind::Requires,
        "optional" => DependencyKind::Optional,
        "incompatible" => DependencyKind::Conflicts,
        _ => return None,
    };
    let target = dep.project_id?;
    Some(ModDependencySpec {
        kind,
        target,
        version_constraint: None,
        reason: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_dependency_to_spec_maps_required() {
        let dep = ProviderDependency {
            project_id: Some("sodium".to_string()),
            version_id: None,
            dependency_type: "required".to_string(),
        };
        let spec = provider_dependency_to_spec(dep).unwrap();
        assert_eq!(spec.kind, DependencyKind::Requires);
        assert_eq!(spec.target, "sodium");
    }

    #[test]
    fn provider_dependency_to_spec_ignores_unknown() {
        let dep = ProviderDependency {
            project_id: Some("foo".to_string()),
            version_id: None,
            dependency_type: "embedded".to_string(),
        };
        assert!(provider_dependency_to_spec(dep).is_none());
    }

    #[test]
    fn provider_dependency_to_spec_requires_project_id() {
        let dep = ProviderDependency {
            project_id: None,
            version_id: Some("v1".to_string()),
            dependency_type: "required".to_string(),
        };
        assert!(provider_dependency_to_spec(dep).is_none());
    }
}
