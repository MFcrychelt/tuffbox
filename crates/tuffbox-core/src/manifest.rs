use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("failed to read manifest {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse manifest {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("project has no profiles")]
    NoProfiles,
    #[error("project has no client or both profile")]
    NoClientProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectManifest {
    pub schema_version: String,
    pub project: ProjectMetadata,
    pub minecraft: MinecraftSpec,
    pub loader: LoaderSpec,
    #[serde(default)]
    pub java: Option<JavaSpec>,
    #[serde(default)]
    pub profiles: Vec<ProfileSpec>,
    #[serde(default)]
    pub mods: Vec<ModSpec>,
    #[serde(default)]
    pub overrides: Option<OverridesSpec>,
}

impl ProjectManifest {
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, ManifestError> {
        let path_ref = path.as_ref();
        let path_string = path_ref.display().to_string();
        let raw = fs::read_to_string(path_ref).map_err(|source| ManifestError::Read {
            path: path_string.clone(),
            source,
        })?;
        let manifest = serde_json::from_str(&raw).map_err(|source| ManifestError::Parse {
            path: path_string,
            source,
        })?;
        Ok(manifest)
    }

    pub fn validate_basic(&self) -> Result<(), ManifestError> {
        if self.profiles.is_empty() {
            return Err(ManifestError::NoProfiles);
        }

        let has_client = self
            .profiles
            .iter()
            .any(|profile| matches!(profile.side, Side::Client | Side::Both));

        if !has_client {
            return Err(ManifestError::NoClientProfile);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftSpec {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderSpec {
    #[serde(rename = "type")]
    pub kind: LoaderKind,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Vanilla,
    Fabric,
    Forge,
    Neoforge,
    Quilt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaSpec {
    #[serde(default)]
    pub major: Option<u16>,
    #[serde(default)]
    pub distribution: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSpec {
    pub id: String,
    pub name: String,
    pub side: Side,
    #[serde(default)]
    pub include_optional_mods: bool,
    #[serde(default)]
    pub include_shaders: bool,
    #[serde(default)]
    pub memory_mb: Option<u32>,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub include_mods: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Client,
    Server,
    Both,
    Optional,
    Unknown,
}

impl Side {
    pub fn is_compatible_with_profile(self, profile_side: Side) -> bool {
        match (self, profile_side) {
            (Side::Both, _) => true,
            (_, Side::Both) => true,
            (Side::Client, Side::Client) => true,
            (Side::Server, Side::Server) => true,
            (Side::Optional, _) => true,
            (Side::Unknown, _) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSpec {
    pub id: String,
    pub name: String,
    pub source: ModSource,
    pub version: String,
    #[serde(default)]
    pub file_name: Option<String>,
    #[serde(default)]
    pub hashes: Option<FileHashes>,
    pub side: Side,
    #[serde(default)]
    pub dependencies: Vec<ModDependencySpec>,
    #[serde(default)]
    pub status: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModSource {
    #[serde(rename = "type")]
    pub kind: SourceKind,
    #[serde(default, rename = "projectId")]
    pub project_id: Option<String>,
    #[serde(default, rename = "fileId")]
    pub file_id: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Modrinth,
    Curseforge,
    Github,
    Local,
    Direct,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHashes {
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDependencySpec {
    #[serde(rename = "type")]
    pub kind: DependencyKind,
    pub target: String,
    #[serde(default, rename = "versionConstraint")]
    pub version_constraint: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    Requires,
    Optional,
    Conflicts,
    BreaksWith,
    Replaces,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverridesSpec {
    #[serde(default)]
    pub config: Option<String>,
    #[serde(default)]
    pub kubejs: Option<String>,
    #[serde(default)]
    pub resourcepacks: Option<String>,
    #[serde(default)]
    pub shaderpacks: Option<String>,
}
