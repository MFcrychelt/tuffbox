use crate::manifest::{DependencyKind, LoaderKind, ProjectManifest, Side};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
                graph.edges.push(GraphEdge {
                    from: mod_id.clone(),
                    to: NodeId::module(&dep.target),
                    kind: dependency_kind_to_edge_kind(dep.kind),
                    constraint: dep.version_constraint.clone(),
                    reason: dep.reason.clone(),
                });
            }
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
