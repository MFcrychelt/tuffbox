use super::{
    provider_dependency_to_spec, ContentProvider, ModDependencySpec, ProjectInfo,
    ProviderDependency, ProviderError, ProviderFileHashes, ProviderFileInfo, ProviderSearchQuery,
    VersionInfo,
};
use serde::{Deserialize, Deserializer};

const BASE_URL: &str = "https://api.modrinth.com/v2";

/// Deserializes a field that can be either a string or an object.
/// Modrinth returns `license` as either a string ID or an object
/// `{"id": "MIT", "name": "MIT License", "url": "..."}`, and
/// `client_side`/`server_side` as either a string or an object
/// `{"client": "optional", "server": "required"}`.
fn string_or_object<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde_json::Value;
    let value = Value::deserialize(deserializer)?;
    match value {
        Value::Null => Ok(None),
        Value::String(s) => Ok(Some(s)),
        Value::Object(map) => {
            if let Some(id) = map.get("id").and_then(|v| v.as_str()) {
                Ok(Some(id.to_string()))
            } else if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                Ok(Some(name.to_string()))
            } else if let Some(client) = map.get("client").and_then(|v| v.as_str()) {
                Ok(Some(client.to_string()))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}

pub struct ModrinthProvider;

impl ModrinthProvider {
    pub fn new() -> Self {
        Self
    }


    fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, ProviderError> {
        let url = format!("{BASE_URL}{path}");
        crate::http::get_json_with_context(&url).map_err(ProviderError::NetworkContext)
    }

    /// Looks up the Modrinth version that produced a given file, by SHA1 hash.
    ///
    /// This lets TuffBox recognize `.jar` files that were dropped into the
    /// `mods/` folder manually (outside the IDE) and turn them into proper
    /// tracked Modrinth-sourced entries instead of leaving them as opaque
    /// "local" mods forever.
    pub fn get_version_by_hash(&self, sha1: &str) -> Result<Option<VersionInfo>, ProviderError> {
        let url = format!("{BASE_URL}/version_file/{sha1}?algorithm=sha1");
        let version: Option<ModrinthVersion> = crate::http::get_json_optional(&url)?;
        Ok(version.map(Into::into))
    }

    /// Resolves the parent project for a version obtained through
    /// [`Self::get_version_by_hash`].
    pub fn identify_local_jar(&self, sha1: &str) -> Result<Option<(ProjectInfo, VersionInfo)>, ProviderError> {
        let Some(version) = self.get_version_by_hash(sha1)? else {
            return Ok(None);
        };
        let project = self.get_project(&version.project_id)?;
        Ok(Some((project, version)))
    }

    /// Batch-resolves the latest compatible version for a set of file hashes
    /// using Modrinth's `POST /v2/version_files/update` endpoint.
    ///
    /// Returns a map of `sha1 -> latest VersionInfo` for every hash that has
    /// an update available.
    pub fn get_latest_versions(
        &self,
        hashes: &[String],
        loaders: &[String],
        game_versions: &[String],
    ) -> Result<std::collections::HashMap<String, VersionInfo>, ProviderError> {
        if hashes.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let url = format!("{BASE_URL}/version_files/update");
        let body = serde_json::json!({
            "hashes": hashes,
            "algorithm": "sha1",
            "loaders": loaders,
            "game_versions": game_versions,
        });
        let raw: std::collections::HashMap<String, ModrinthVersion> =
            crate::http::post_json(&url, &body)?;
        Ok(raw.into_iter().map(|(k, v)| (k, v.into())).collect())
    }
}

impl ContentProvider for ModrinthProvider {
    fn search(&self, query: &ProviderSearchQuery) -> Result<Vec<ProjectInfo>, ProviderError> {
        let index = query.sort.as_deref().unwrap_or("relevance");
        let limit = query.limit.unwrap_or(24).clamp(1, 100);
        let mut path = format!("/search?index={}&limit={}", urlencode(index), limit);
        if let Some(q) = &query.query {
            if !q.trim().is_empty() {
                path.push_str(&format!("&query={}", urlencode(q.trim())));
            }
        }
        let facets = build_facets(query);
        if !facets.is_empty() {
            path.push_str(&format!("&facets={}", urlencode(&facets)));
        }

        let response: ModrinthSearchResponse = self.get_json(&path)?;
        Ok(response.hits.into_iter().map(Into::into).collect())
    }

    fn get_project(&self, id: &str) -> Result<ProjectInfo, ProviderError> {
        let project: ModrinthProject = self.get_json(&format!("/project/{id}"))?;
        Ok(project.into())
    }

    fn get_version(&self, version_id: &str) -> Result<VersionInfo, ProviderError> {
        let version: ModrinthVersion = self.get_json(&format!("/version/{version_id}"))?;
        Ok(version.into())
    }

    fn get_versions(
        &self,
        id: &str,
        query: &ProviderSearchQuery,
    ) -> Result<Vec<VersionInfo>, ProviderError> {
        let mut path = format!("/project/{id}/version");
        let mut params = Vec::new();
        if let Some(loader) = &query.loader {
            params.push(format!(
                "loaders={}",
                urlencode(&serde_json::to_string(&[loader]).unwrap())
            ));
        }
        if let Some(mc) = &query.minecraft_version {
            params.push(format!(
                "game_versions={}",
                urlencode(&serde_json::to_string(&[mc]).unwrap())
            ));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }

        let versions: Vec<ModrinthVersion> = self.get_json(&path)?;
        Ok(versions.into_iter().map(Into::into).collect())
    }

    fn get_file(
        &self,
        version_id: &str,
        filename: &str,
    ) -> Result<ProviderFileInfo, ProviderError> {
        let version: ModrinthVersion = self.get_json(&format!("/version/{version_id}"))?;
        version
            .files
            .into_iter()
            .find(|f| f.filename == filename)
            .map(Into::into)
            .ok_or_else(|| ProviderError::VersionNotFound(filename.to_string()))
    }

    fn resolve_dependencies(
        &self,
        version_id: &str,
    ) -> Result<Vec<ModDependencySpec>, ProviderError> {
        let version: ModrinthVersion = self.get_json(&format!("/version/{version_id}"))?;
        let mut dependencies: Vec<ModDependencySpec> = version
            .dependencies
            .into_iter()
            .map(ProviderDependency::from)
            .filter_map(provider_dependency_to_spec)
            .collect();

        // Modrinth dependency payloads use immutable project IDs, while TuffBox
        // mod nodes use stable human-readable slugs (`mod:sodium`, `mod:fabric-api`).
        // Normalizing here keeps missing-dependency diagnostics consistent across
        // CLI, desktop UI and imported manifests.
        for dependency in &mut dependencies {
            if let Ok(project) = self.get_project(&dependency.target) {
                dependency.target = project.slug;
            }
        }

        Ok(dependencies)
    }
}

fn build_facets(query: &ProviderSearchQuery) -> String {
    let mut facets: Vec<Vec<String>> = Vec::new();
    let project_type = query.project_type.as_deref().unwrap_or("mod");
    facets.push(vec![format!("project_type:{project_type}")]);

    if let Some(mc) = &query.minecraft_version {
        facets.push(vec![format!("versions:{mc}")]);
    }
    // The loader facet only makes sense for loader-bound content (mods,
    // modpacks, plugins). Resourcepacks/datapacks/shaders aren't tied to a
    // mod loader on Modrinth, so applying it there would silently zero out
    // every result.
    if matches!(project_type, "mod" | "modpack" | "plugin") {
        if let Some(loader) = &query.loader {
            if !loader.trim().is_empty() {
                facets.push(vec![format!("categories:{}", loader.trim().to_lowercase())]);
            }
        }
    }
    if let Some(category) = &query.category {
        if !category.trim().is_empty() {
            facets.push(vec![format!("categories:{}", category.trim().to_lowercase().replace(' ', "-"))]);
        }
    }
    if let Some(environment) = &query.environment {
        if !environment.trim().is_empty() {
            facets.push(vec![format!("{}_side:required", environment.trim().to_lowercase())]);
        }
    }
    if query.license.as_deref() == Some("open-source") {
        facets.push(vec!["open_source:true".to_string()]);
    }
    if facets.is_empty() {
        return String::new();
    }
    serde_json::to_string(&facets).unwrap_or_default()
}

fn urlencode(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('[', "%5B")
        .replace(']', "%5D")
        .replace('"', "%22")
        .replace(':', "%3A")
        .replace(',', "%2C")
        .replace('&', "%26")
        .replace('+', "%2B")
        .replace('#', "%23")
        .replace('=', "%3D")
        .replace('?', "%3F")
        .replace('/', "%2F")
        .replace('@', "%40")
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthSearchHit>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ModrinthSearchHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    project_type: String,
    icon_url: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    follows: Option<u64>,
    #[serde(default)]
    date_modified: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default, deserialize_with = "string_or_object")]
    license: Option<String>,
    #[serde(default, deserialize_with = "string_or_object")]
    client_side: Option<String>,
    #[serde(default, deserialize_with = "string_or_object")]
    server_side: Option<String>,
}

impl From<ModrinthSearchHit> for ProjectInfo {
    fn from(hit: ModrinthSearchHit) -> Self {
        Self {
            id: hit.project_id,
            slug: hit.slug,
            name: hit.title,
            description: hit.description,
            project_type: hit.project_type,
            icon_url: hit.icon_url,
            author: hit.author,
            downloads: hit.downloads,
            follows: hit.follows,
            date_modified: hit.date_modified,
            categories: hit.categories,
            license: hit.license,
            client_side: hit.client_side,
            server_side: hit.server_side,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ModrinthProject {
    id: String,
    slug: String,
    title: String,
    description: String,
    project_type: String,
    icon_url: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    follows: Option<u64>,
    #[serde(default)]
    date_modified: Option<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default, deserialize_with = "string_or_object")]
    license: Option<String>,
    #[serde(default, deserialize_with = "string_or_object")]
    client_side: Option<String>,
    #[serde(default, deserialize_with = "string_or_object")]
    server_side: Option<String>,
}

impl From<ModrinthProject> for ProjectInfo {
    fn from(project: ModrinthProject) -> Self {
        Self {
            id: project.id,
            slug: project.slug,
            name: project.title,
            description: project.description,
            project_type: project.project_type,
            icon_url: project.icon_url,
            author: project.author,
            downloads: project.downloads,
            follows: project.follows,
            date_modified: project.date_modified,
            categories: project.categories,
            license: project.license,
            client_side: project.client_side,
            server_side: project.server_side,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ModrinthVersion {
    id: String,
    project_id: String,
    version_number: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    files: Vec<ModrinthFile>,
    dependencies: Vec<ModrinthDependency>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    changelog: Option<String>,
    #[serde(default)]
    date_published: Option<String>,
}

impl From<ModrinthVersion> for VersionInfo {
    fn from(version: ModrinthVersion) -> Self {
        Self {
            id: version.id,
            project_id: version.project_id,
            version_number: version.version_number,
            game_versions: version.game_versions,
            loaders: version.loaders,
            files: version.files.into_iter().map(Into::into).collect(),
            dependencies: version
                .dependencies
                .into_iter()
                .map(ProviderDependency::from)
                .collect(),
            name: version.name,
            changelog: version.changelog,
            date_published: version.date_published,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ModrinthFile {
    url: String,
    filename: String,
    primary: bool,
    hashes: ModrinthFileHashes,
}

impl From<ModrinthFile> for ProviderFileInfo {
    fn from(file: ModrinthFile) -> Self {
        Self {
            url: file.url,
            filename: file.filename,
            primary: file.primary,
            hashes: ProviderFileHashes {
                sha1: file.hashes.sha1,
                sha512: file.hashes.sha512,
            },
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ModrinthFileHashes {
    sha1: Option<String>,
    sha512: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ModrinthDependency {
    project_id: Option<String>,
    version_id: Option<String>,
    dependency_type: String,
}

impl From<ModrinthDependency> for ProviderDependency {
    fn from(dep: ModrinthDependency) -> Self {
        Self {
            project_id: dep.project_id,
            version_id: dep.version_id,
            dependency_type: dep.dependency_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires network access to Modrinth"]
    fn searches_sodium_for_fabric_1_20_1() {
        let provider = ModrinthProvider::new();
        let results = provider
            .search(&ProviderSearchQuery {
                query: Some("sodium".to_string()),
                minecraft_version: Some("1.20.1".to_string()),
                loader: Some("fabric".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|p| p.slug == "sodium"));
    }
}
