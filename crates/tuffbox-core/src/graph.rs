use crate::manifest::{DependencyKind, LoaderKind, ProjectManifest, Side};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn minecraft(version: &str) -> Self {
        Self(format!("minecraft:{version}"))
    }

    pub fn loader(kind: &LoaderKind, version: &str) -> Self {
        Self(format!("loader:{}:{version}", loader_kind_slug(kind)))
    }

    pub fn java(major: u16) -> Self {
        Self(format!("java:{major}"))
    }

    pub fn profile(id: &str) -> Self {
        Self(format!("profile:{id}"))
    }

    pub fn module(id: &str) -> Self {
        Self(format!("mod:{id}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    MinecraftVersion,
    Loader,
    JavaRuntime,
    Mod,
    Missing,
    Library,
    ConfigFile,
    ScriptFile,
    ResourcePack,
    ShaderPack,
    Profile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    Requires,
    Optional,
    Conflicts,
    BreaksWith,
    Replaces,
    RequiresLoader,
    RequiresMinecraft,
    RequiresJava,
    IncludedInProfile,
}

impl EdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Requires => "Requires",
            EdgeKind::Optional => "Optional",
            EdgeKind::Conflicts => "Conflicts",
            EdgeKind::BreaksWith => "BreaksWith",
            EdgeKind::Replaces => "Replaces",
            EdgeKind::RequiresLoader => "RequiresLoader",
            EdgeKind::RequiresMinecraft => "RequiresMinecraft",
            EdgeKind::RequiresJava => "RequiresJava",
            EdgeKind::IncludedInProfile => "IncludedInProfile",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: String,
    pub version: Option<String>,
    pub side: Side,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    pub constraint: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl DependencyGraph {
    pub fn from_manifest(manifest: &ProjectManifest) -> Self {
        let mut graph = Self::default();

        // Canonicalize dependency targets onto installed mod ids.
        // Modrinth deps may use either project ids (AANobbMI) or slugs (sodium);
        // installed mods use slug as `id` and store the project id on source.
        let mut target_aliases: HashMap<String, String> = HashMap::new();
        for module in &manifest.mods {
            target_aliases.insert(module.id.clone(), module.id.clone());
            if let Some(pid) = &module.source.project_id {
                target_aliases.insert(pid.clone(), module.id.clone());
            }
        }

        let minecraft_id = NodeId::minecraft(&manifest.minecraft.version);
        graph.nodes.push(GraphNode {
            id: minecraft_id.clone(),
            kind: NodeKind::MinecraftVersion,
            label: format!("Minecraft {}", manifest.minecraft.version),
            version: Some(manifest.minecraft.version.clone()),
            side: Side::Both,
            metadata: HashMap::new(),
        });

        let loader_id = NodeId::loader(&manifest.loader.kind, &manifest.loader.version);
        graph.nodes.push(GraphNode {
            id: loader_id.clone(),
            kind: NodeKind::Loader,
            label: format!(
                "{} {}",
                loader_kind_label(&manifest.loader.kind),
                manifest.loader.version
            ),
            version: Some(manifest.loader.version.clone()),
            side: Side::Both,
            metadata: HashMap::new(),
        });

        graph.edges.push(GraphEdge {
            from: loader_id.clone(),
            to: minecraft_id.clone(),
            kind: EdgeKind::RequiresMinecraft,
            constraint: Some(manifest.minecraft.version.clone()),
            reason: Some("Selected loader is installed for project Minecraft version".to_string()),
        });

        if let Some(java) = &manifest.java {
            if let Some(major) = java.major {
                let java_id = NodeId::java(major);
                graph.nodes.push(GraphNode {
                    id: java_id.clone(),
                    kind: NodeKind::JavaRuntime,
                    label: format!("Java {major}"),
                    version: Some(major.to_string()),
                    side: Side::Both,
                    metadata: HashMap::new(),
                });
                graph.edges.push(GraphEdge {
                    from: loader_id.clone(),
                    to: java_id,
                    kind: EdgeKind::RequiresJava,
                    constraint: Some(major.to_string()),
                    reason: Some("Project selected Java runtime".to_string()),
                });
            }
        }

        for profile in &manifest.profiles {
            graph.nodes.push(GraphNode {
                id: NodeId::profile(&profile.id),
                kind: NodeKind::Profile,
                label: profile.name.clone(),
                version: None,
                side: profile.side,
                metadata: HashMap::new(),
            });
        }

        for module in &manifest.mods {
            let mut metadata = HashMap::new();
            metadata.insert(
                "source".to_string(),
                format!("{:?}", module.source.kind).to_lowercase(),
            );
            if let Some(project_id) = &module.source.project_id {
                metadata.insert("project_id".to_string(), project_id.clone());
            }
            if let Some(file_id) = &module.source.file_id {
                metadata.insert("file_id".to_string(), file_id.clone());
            }
            if let Some(icon_url) = &module.source.icon_url {
                metadata.insert("icon_url".to_string(), icon_url.clone());
            }

            let mod_id = NodeId::module(&module.id);
            graph.nodes.push(GraphNode {
                id: mod_id.clone(),
                kind: NodeKind::Mod,
                label: module.name.clone(),
                version: Some(module.version.clone()),
                side: module.side,
                metadata,
            });

            graph.edges.push(GraphEdge {
                from: mod_id.clone(),
                to: loader_id.clone(),
                kind: EdgeKind::RequiresLoader,
                constraint: Some(format!(
                    "{} {}",
                    loader_kind_slug(&manifest.loader.kind),
                    manifest.loader.version
                )),
                reason: Some("Mod is part of selected loader project".to_string()),
            });

            graph.edges.push(GraphEdge {
                from: mod_id.clone(),
                to: minecraft_id.clone(),
                kind: EdgeKind::RequiresMinecraft,
                constraint: Some(manifest.minecraft.version.clone()),
                reason: Some("Mod is part of selected Minecraft project".to_string()),
            });

            for profile in &manifest.profiles {
                // Если include_mods пуст — все совместимые моды включаются (режим "все").
                // Если include_mods не пуст — только перечисленные моды (режим "белый список").
                let explicitly_included = !profile.include_mods.is_empty()
                    && profile.include_mods.iter().any(|id| id == &module.id);
                let implicitly_included = profile.include_mods.is_empty();
                let included = explicitly_included || implicitly_included;
                if included && module.side.is_compatible_with_profile(profile.side) {
                    graph.edges.push(GraphEdge {
                        from: NodeId::profile(&profile.id),
                        to: mod_id.clone(),
                        kind: EdgeKind::IncludedInProfile,
                        constraint: None,
                        reason: Some(format!("Mod is compatible with {} profile", profile.name)),
                    });
                }
            }

            for dep in &module.dependencies {
                let resolved_target = resolve_dependency_target(&dep.target, &target_aliases);
                // Skip self-edges (a mod declaring itself as a dependency).
                if resolved_target == module.id {
                    continue;
                }
                graph.edges.push(GraphEdge {
                    from: mod_id.clone(),
                    to: NodeId::module(&resolved_target),
                    kind: dependency_kind_to_edge_kind(dep.kind),
                    constraint: dep.version_constraint.clone(),
                    reason: dep.reason.clone(),
                });
            }
        }

        // Missing dependencies are real graph nodes rather than a UI-only
        // invention. This keeps the resolver, CLI, DOT export and desktop
        // view consistent and guarantees every edge endpoint exists.
        let existing: HashSet<NodeId> = graph.nodes.iter().map(|node| node.id.clone()).collect();
        let missing: HashSet<NodeId> = graph
            .edges
            .iter()
            .filter(|edge| edge.to.0.starts_with("mod:") && !existing.contains(&edge.to))
            .map(|edge| edge.to.clone())
            .collect();
        for id in missing {
            graph.nodes.push(GraphNode {
                label: id.0.strip_prefix("mod:").unwrap_or(&id.0).to_string(),
                id,
                kind: NodeKind::Missing,
                version: None,
                side: Side::Unknown,
                metadata: HashMap::new(),
            });
        }

        graph
    }

    pub fn has_node(&self, id: &NodeId) -> bool {
        self.nodes.iter().any(|node| &node.id == id)
    }

    pub fn node(&self, id: &NodeId) -> Option<&GraphNode> {
        self.nodes.iter().find(|node| &node.id == id)
    }
}

/// Map a Modrinth project id or slug onto the installed mod id when possible.
fn resolve_dependency_target(target: &str, aliases: &HashMap<String, String>) -> String {
    aliases
        .get(target)
        .cloned()
        .unwrap_or_else(|| target.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::ProjectManifest;

    #[test]
    fn resolves_project_id_dep_onto_installed_slug_node() {
        let raw = r#"{
          "schemaVersion": "0.1.0",
          "project": { "id": "test", "name": "Test", "version": "1.0.0" },
          "minecraft": { "version": "1.20.1" },
          "loader": { "type": "fabric", "version": "0.15.11" },
          "profiles": [{ "id": "client", "name": "Client", "side": "client" }],
          "mods": [
            {
              "id": "iris",
              "name": "Iris",
              "source": { "type": "modrinth", "projectId": "YL57xq9U" },
              "version": "1.0.0",
              "side": "client",
              "dependencies": [{ "type": "requires", "target": "AANobbMI" }]
            },
            {
              "id": "sodium",
              "name": "Sodium",
              "source": { "type": "modrinth", "projectId": "AANobbMI" },
              "version": "0.5.0",
              "side": "client",
              "dependencies": []
            }
          ]
        }"#;
        let manifest: ProjectManifest = serde_json::from_str(raw).unwrap();
        let graph = DependencyGraph::from_manifest(&manifest);
        let edge = graph
            .edges
            .iter()
            .find(|e| e.from.0 == "mod:iris" && e.kind == EdgeKind::Requires)
            .expect("iris requires edge");
        assert_eq!(edge.to.0, "mod:sodium");
        assert!(graph.has_node(&NodeId::module("sodium")));
    }
}

fn dependency_kind_to_edge_kind(kind: DependencyKind) -> EdgeKind {
    match kind {
        DependencyKind::Requires => EdgeKind::Requires,
        DependencyKind::Optional => EdgeKind::Optional,
        DependencyKind::Conflicts => EdgeKind::Conflicts,
        DependencyKind::BreaksWith => EdgeKind::BreaksWith,
        DependencyKind::Replaces => EdgeKind::Replaces,
    }
}

pub fn loader_kind_slug(kind: &LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Vanilla => "vanilla",
        LoaderKind::Fabric => "fabric",
        LoaderKind::Forge => "forge",
        LoaderKind::Neoforge => "neoforge",
        LoaderKind::Quilt => "quilt",
    }
}

fn loader_kind_label(kind: &LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Vanilla => "Vanilla",
        LoaderKind::Fabric => "Fabric",
        LoaderKind::Forge => "Forge",
        LoaderKind::Neoforge => "NeoForge",
        LoaderKind::Quilt => "Quilt",
    }
}
