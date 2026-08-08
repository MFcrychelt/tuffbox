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
        // Local jars may still carry filename ids (meteor-client-0.5.8) until
        // jar enrichment rewrites them — versioned-slug matching covers that.
        let mut target_aliases: HashMap<String, String> = HashMap::new();
        let installed_ids: Vec<String> = manifest.mods.iter().map(|m| m.id.clone()).collect();
        for module in &manifest.mods {
            target_aliases.insert(module.id.clone(), module.id.clone());
            target_aliases
                .entry(normalize_mod_key(&module.id))
                .or_insert_with(|| module.id.clone());
            if let Some(pid) = &module.source.project_id {
                target_aliases.insert(pid.clone(), module.id.clone());
            }
        }
        // fabric.mod.json often depends on `fabric`; Fabric API's id is `fabric-api`
        // (or a versioned local filename like fabric-api-0.100.0+1.21.jar).
        let fabric_api_id = installed_ids.iter().find(|id| {
            normalize_mod_key(id) == "fabric-api" || looks_like_versioned_slug(id, "fabric-api")
        });
        if let Some(id) = fabric_api_id {
            target_aliases
                .entry("fabric-api".into())
                .or_insert_with(|| id.clone());
            target_aliases
                .entry("fabric".into())
                .or_insert_with(|| id.clone());
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
                module.source.kind.as_str().to_string(),
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
            // Provider categories (Modrinth / normalized CF tags) drive graph
            // clustering. Pipe-separated so CF names with commas stay intact
            // if ever stored raw; Modrinth slugs never contain `|` or `,`.
            if !module.source.categories.is_empty() {
                metadata.insert(
                    "categories".to_string(),
                    module.source.categories.join("|"),
                );
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
                if included && module.included_in_profile(profile) {
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
                let resolved_target =
                    resolve_dependency_target(&dep.target, &target_aliases, &installed_ids);
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

        // Conflicts / BreaksWith only apply when the other mod is actually
        // installed. LambDynamicLights declaring "incompatible with ryoamiclights"
        // must not surface as a conflict when ryoamiclights isn't in the pack.
        {
            let installed_mods: HashSet<NodeId> = graph
                .nodes
                .iter()
                .filter(|n| n.kind == NodeKind::Mod)
                .map(|n| n.id.clone())
                .collect();
            graph.edges.retain(|edge| {
                if !matches!(edge.kind, EdgeKind::Conflicts | EdgeKind::BreaksWith) {
                    return true;
                }
                installed_mods.contains(&edge.from) && installed_mods.contains(&edge.to)
            });
        }

        // Missing dependencies are real graph nodes rather than a UI-only
        // invention. Only *required* unresolved deps become Missing nodes —
        // optional integrations must not appear as install prompts.
        let existing: HashSet<NodeId> = graph.nodes.iter().map(|node| node.id.clone()).collect();
        let missing: HashSet<NodeId> = graph
            .edges
            .iter()
            .filter(|edge| {
                edge.kind == EdgeKind::Requires
                    && edge.to.0.starts_with("mod:")
                    && !existing.contains(&edge.to)
            })
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

        // Inject built-in known conflicts between installed mods so the graph
        // surfaces Sodium↔OptiFine style problems even when Modrinth metadata
        // didn't declare an incompatible edge.
        {
            use crate::knowledge::builtin::check_known_conflict;
            let mod_slugs: Vec<String> = graph
                .nodes
                .iter()
                .filter(|n| n.kind == NodeKind::Mod)
                .map(|n| {
                    n.id.0
                        .strip_prefix("mod:")
                        .unwrap_or(&n.id.0)
                        .to_string()
                })
                .collect();
            let mut existing_pairs: HashSet<(String, String)> = graph
                .edges
                .iter()
                .filter(|e| matches!(e.kind, EdgeKind::Conflicts | EdgeKind::BreaksWith))
                .map(|e| {
                    let a = e.from.0.clone();
                    let b = e.to.0.clone();
                    if a <= b {
                        (a, b)
                    } else {
                        (b, a)
                    }
                })
                .collect();
            for i in 0..mod_slugs.len() {
                for j in (i + 1)..mod_slugs.len() {
                    let a = &mod_slugs[i];
                    let b = &mod_slugs[j];
                    let Some(reason) = check_known_conflict(a, b) else {
                        continue;
                    };
                    let from = NodeId::module(a);
                    let to = NodeId::module(b);
                    let key = if from.0 <= to.0 {
                        (from.0.clone(), to.0.clone())
                    } else {
                        (to.0.clone(), from.0.clone())
                    };
                    if !existing_pairs.insert(key) {
                        continue;
                    }
                    graph.edges.push(GraphEdge {
                        from,
                        to,
                        kind: EdgeKind::Conflicts,
                        constraint: None,
                        reason: Some(reason),
                    });
                }
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

fn normalize_mod_key(id: &str) -> String {
    id.trim().to_ascii_lowercase().replace('_', "-")
}

/// True when `id` is `slug` or a versioned filename like `meteor-client-0.5.8`.
/// Rejects soft prefixes (`sodium` ↛ `sodium-extra`).
fn looks_like_versioned_slug(id: &str, slug: &str) -> bool {
    let n = normalize_mod_key(id);
    let s = normalize_mod_key(slug);
    if n == s {
        return true;
    }
    match n.strip_prefix(&format!("{s}-")) {
        Some(rest) => rest.chars().next().is_some_and(|c| c.is_ascii_digit()),
        None => false,
    }
}

/// Map a Modrinth project id or slug onto the installed mod id when possible.
fn resolve_dependency_target(
    target: &str,
    aliases: &HashMap<String, String>,
    installed_ids: &[String],
) -> String {
    if let Some(mapped) = aliases.get(target) {
        return mapped.clone();
    }
    let normalized = normalize_mod_key(target);
    if let Some(mapped) = aliases.get(&normalized) {
        return mapped.clone();
    }
    let matches: Vec<&String> = installed_ids
        .iter()
        .filter(|id| looks_like_versioned_slug(id, target))
        .collect();
    if matches.len() == 1 {
        return matches[0].clone();
    }
    target.to_string()
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

    #[test]
    fn conflict_edges_only_when_both_mods_installed() {
        let raw = r#"{
          "schemaVersion": "0.1.0",
          "project": { "id": "test", "name": "Test", "version": "1.0.0" },
          "minecraft": { "version": "1.21.1" },
          "loader": { "type": "fabric", "version": "0.16.0" },
          "profiles": [{ "id": "client", "name": "Client", "side": "client" }],
          "mods": [
            {
              "id": "lambdynamiclights",
              "name": "LambDynamicLights - Dynamic Lights",
              "source": { "type": "modrinth", "projectId": "yBW8D80W" },
              "version": "4.0.0",
              "side": "client",
              "dependencies": [
                { "type": "conflicts", "target": "ryoamiclights" },
                { "type": "conflicts", "target": "sodium-dynamic-lights" }
              ]
            }
          ]
        }"#;
        let manifest: ProjectManifest = serde_json::from_str(raw).unwrap();
        let graph = DependencyGraph::from_manifest(&manifest);
        let conflicts: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Conflicts | EdgeKind::BreaksWith))
            .collect();
        assert!(
            conflicts.is_empty(),
            "incompatible-with declarations must not become conflicts when the other mod is absent: {conflicts:?}"
        );

        let raw_both = r#"{
          "schemaVersion": "0.1.0",
          "project": { "id": "test", "name": "Test", "version": "1.0.0" },
          "minecraft": { "version": "1.21.1" },
          "loader": { "type": "fabric", "version": "0.16.0" },
          "profiles": [{ "id": "client", "name": "Client", "side": "client" }],
          "mods": [
            {
              "id": "lambdynamiclights",
              "name": "LambDynamicLights",
              "source": { "type": "modrinth", "projectId": "yBW8D80W" },
              "version": "4.0.0",
              "side": "client",
              "dependencies": [
                { "type": "conflicts", "target": "ryoamiclights" }
              ]
            },
            {
              "id": "ryoamiclights",
              "name": "RyoamicLights",
              "source": { "type": "modrinth", "projectId": "svc" },
              "version": "1.0.0",
              "side": "client",
              "dependencies": []
            }
          ]
        }"#;
        let graph_both = DependencyGraph::from_manifest(&serde_json::from_str(raw_both).unwrap());
        let conflicts_both: Vec<_> = graph_both
            .edges
            .iter()
            .filter(|e| matches!(e.kind, EdgeKind::Conflicts | EdgeKind::BreaksWith))
            .collect();
        assert_eq!(
            conflicts_both.len(),
            1,
            "real both-installed conflict should remain: {conflicts_both:?}"
        );
    }

    #[test]
    fn missing_nodes_only_for_required_deps_not_optional() {
        let raw = r#"{
          "schemaVersion": "0.1.0",
          "project": { "id": "test", "name": "Test", "version": "1.0.0" },
          "minecraft": { "version": "1.20.1" },
          "loader": { "type": "fabric", "version": "0.15.11" },
          "profiles": [{ "id": "client", "name": "Client", "side": "client" }],
          "mods": [
            {
              "id": "demo",
              "name": "Demo",
              "source": { "type": "modrinth", "projectId": "demo" },
              "version": "1.0.0",
              "side": "both",
              "dependencies": [
                { "type": "requires", "target": "fabric-api" },
                { "type": "optional", "target": "sodium" },
                { "type": "optional", "target": "iris" }
              ]
            }
          ]
        }"#;
        let manifest: ProjectManifest = serde_json::from_str(raw).unwrap();
        let graph = DependencyGraph::from_manifest(&manifest);
        let missing: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Missing)
            .map(|n| n.id.0.as_str())
            .collect();
        assert!(
            missing.contains(&"mod:fabric-api"),
            "required missing dep should be a Missing node: {missing:?}"
        );
        assert!(
            !missing.iter().any(|id| *id == "mod:sodium" || *id == "mod:iris"),
            "optional deps must not become Missing nodes: {missing:?}"
        );
    }

    #[test]
    fn resolves_versioned_local_filename_onto_dep_slug() {
        let raw = r#"{
          "schemaVersion": "0.1.0",
          "project": { "id": "test", "name": "Test", "version": "1.0.0" },
          "minecraft": { "version": "1.21.1" },
          "loader": { "type": "fabric", "version": "0.16.0" },
          "profiles": [{ "id": "client", "name": "Client", "side": "client" }],
          "mods": [
            {
              "id": "baritone",
              "name": "Baritone",
              "source": { "type": "modrinth", "projectId": "baritone" },
              "version": "1.0.0",
              "side": "client",
              "dependencies": [{ "type": "requires", "target": "meteor-client" }]
            },
            {
              "id": "meteor-client-0.5.8",
              "name": "meteor-client-0.5.8.jar",
              "source": { "type": "local", "path": "mods/meteor-client-0.5.8.jar" },
              "version": "unknown",
              "side": "client",
              "dependencies": []
            }
          ]
        }"#;
        let graph = DependencyGraph::from_manifest(&serde_json::from_str(raw).unwrap());
        let edge = graph
            .edges
            .iter()
            .find(|e| e.from.0 == "mod:baritone" && e.kind == EdgeKind::Requires)
            .expect("requires edge");
        assert_eq!(edge.to.0, "mod:meteor-client-0.5.8");
        assert!(
            !graph.nodes.iter().any(|n| n.kind == NodeKind::Missing),
            "local versioned jar must satisfy meteor-client"
        );
        assert_eq!(
            graph
                .node(&NodeId::module("meteor-client-0.5.8"))
                .and_then(|n| n.metadata.get("source"))
                .map(String::as_str),
            Some("local")
        );
    }

    #[test]
    fn resolves_fabric_dep_onto_fabric_api() {
        let raw = r#"{
          "schemaVersion": "0.1.0",
          "project": { "id": "test", "name": "Test", "version": "1.0.0" },
          "minecraft": { "version": "1.21.1" },
          "loader": { "type": "fabric", "version": "0.16.0" },
          "profiles": [{ "id": "client", "name": "Client", "side": "client" }],
          "mods": [
            {
              "id": "demo",
              "name": "Demo",
              "source": { "type": "modrinth", "projectId": "demo" },
              "version": "1.0.0",
              "side": "both",
              "dependencies": [{ "type": "requires", "target": "fabric" }]
            },
            {
              "id": "fabric-api",
              "name": "Fabric API",
              "source": { "type": "modrinth", "projectId": "P7dR8mSH" },
              "version": "0.100.0",
              "side": "both",
              "dependencies": []
            }
          ]
        }"#;
        let graph = DependencyGraph::from_manifest(&serde_json::from_str(raw).unwrap());
        let edge = graph
            .edges
            .iter()
            .find(|e| e.from.0 == "mod:demo" && e.kind == EdgeKind::Requires)
            .expect("requires edge");
        assert_eq!(edge.to.0, "mod:fabric-api");
        assert!(!graph.nodes.iter().any(|n| n.kind == NodeKind::Missing));
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
