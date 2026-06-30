use crate::{
    change_plan::{ChangeAction, ChangePlan, ChangeRisk},
    diagnostics::{Diagnostic, DiagnosticSeverity},
    graph::{DependencyGraph, EdgeKind, NodeId, NodeKind},
    manifest::{ProjectManifest, Side},
};
use std::collections::{HashMap, HashSet};

pub struct Resolver;

impl Resolver {
    pub fn analyze(graph: &DependencyGraph) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        diagnostics.extend(Self::find_missing_dependencies(graph));
        diagnostics.extend(Self::find_conflicts(graph));
        diagnostics.extend(Self::find_duplicate_mod_ids(graph));
        diagnostics.extend(Self::find_profile_includes_unknown_mod(graph));
        diagnostics
    }

    pub fn analyze_project(manifest: &ProjectManifest, graph: &DependencyGraph) -> Vec<Diagnostic> {
        let mut diagnostics = Self::analyze(graph);
        diagnostics.extend(Self::find_wrong_side_in_profile(manifest));
        diagnostics.extend(Self::find_unknown_sides(graph));
        diagnostics
    }

    pub fn create_fix_plan(
        graph: &DependencyGraph,
        diagnostics: &[Diagnostic],
    ) -> Option<ChangePlan> {
        let error = diagnostics
            .iter()
            .find(|d| d.severity == DiagnosticSeverity::Error)?;

        match error.code.as_str() {
            "MISSING_DEPENDENCY" => {
                let missing = error
                    .related_nodes
                    .last()?
                    .0
                    .strip_prefix("mod:")?
                    .to_string();
                Some(ChangePlan {
                    summary: format!("Install missing dependency: {missing}"),
                    risk: ChangeRisk::Low,
                    actions: vec![ChangeAction::InstallMod {
                        project_id: missing,
                        version: None,
                    }],
                    requires_snapshot: true,
                })
            }
            "MOD_CONFLICT" => {
                let removable = error.related_nodes.last()?.clone();
                let label = graph
                    .node(&removable)
                    .map(|n| n.label.clone())
                    .unwrap_or(removable.0.clone());
                Some(ChangePlan {
                    summary: format!("Disable conflicting mod: {label}"),
                    risk: ChangeRisk::Medium,
                    actions: vec![ChangeAction::DisableMod { node_id: removable }],
                    requires_snapshot: true,
                })
            }
            _ => Some(ChangePlan {
                summary: format!("Review diagnostic: {}", error.code),
                risk: ChangeRisk::Medium,
                actions: vec![],
                requires_snapshot: true,
            }),
        }
    }

    fn find_missing_dependencies(graph: &DependencyGraph) -> Vec<Diagnostic> {
        graph
            .edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Requires)
            .filter(|edge| !graph.has_node(&edge.to))
            .map(|edge| {
                let from = graph
                    .node(&edge.from)
                    .map(|n| n.label.clone())
                    .unwrap_or(edge.from.0.clone());
                Diagnostic::error(
                    "MISSING_DEPENDENCY",
                    format!("{from} requires missing dependency {}", edge.to.0),
                    vec![edge.from.clone(), edge.to.clone()],
                )
            })
            .collect()
    }

    fn find_conflicts(graph: &DependencyGraph) -> Vec<Diagnostic> {
        graph
            .edges
            .iter()
            .filter(|edge| matches!(edge.kind, EdgeKind::Conflicts | EdgeKind::BreaksWith))
            .filter(|edge| graph.has_node(&edge.from) && graph.has_node(&edge.to))
            .map(|edge| {
                let from = graph
                    .node(&edge.from)
                    .map(|n| n.label.clone())
                    .unwrap_or(edge.from.0.clone());
                let to = graph
                    .node(&edge.to)
                    .map(|n| n.label.clone())
                    .unwrap_or(edge.to.0.clone());
                Diagnostic::error(
                    "MOD_CONFLICT",
                    edge.reason
                        .clone()
                        .unwrap_or_else(|| format!("{from} conflicts with {to}")),
                    vec![edge.from.clone(), edge.to.clone()],
                )
            })
            .collect()
    }

    fn find_duplicate_mod_ids(graph: &DependencyGraph) -> Vec<Diagnostic> {
        let mut seen: HashMap<&str, Vec<NodeId>> = HashMap::new();
        for node in &graph.nodes {
            if node.kind == NodeKind::Mod {
                seen.entry(node.id.0.as_str())
                    .or_default()
                    .push(node.id.clone());
            }
        }

        seen.into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(id, ids)| {
                Diagnostic::error("DUPLICATE_MOD", format!("Duplicate mod node: {id}"), ids)
            })
            .collect()
    }

    fn find_profile_includes_unknown_mod(graph: &DependencyGraph) -> Vec<Diagnostic> {
        let mod_nodes: HashSet<_> = graph
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Mod)
            .map(|node| node.id.clone())
            .collect();

        graph
            .edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::IncludedInProfile)
            .filter(|edge| !mod_nodes.contains(&edge.to))
            .map(|edge| {
                Diagnostic::warning(
                    "PROFILE_INCLUDES_UNKNOWN_MOD",
                    format!("Profile edge points to unknown mod {}", edge.to.0),
                    vec![edge.from.clone(), edge.to.clone()],
                )
            })
            .collect()
    }

    fn find_wrong_side_in_profile(manifest: &ProjectManifest) -> Vec<Diagnostic> {
        let mod_map: HashMap<String, Side> = manifest
            .mods
            .iter()
            .map(|module| (module.id.clone(), module.side))
            .collect();

        let mut diagnostics = Vec::new();
        for profile in &manifest.profiles {
            for mod_id in &profile.include_mods {
                let Some(&module_side) = mod_map.get(mod_id) else {
                    continue;
                };
                if module_side.is_compatible_with_profile(profile.side) {
                    continue;
                }
                diagnostics.push(Diagnostic::error(
                    "WRONG_SIDE_IN_PROFILE",
                    format!(
                        "Profile {} includes mod {} with incompatible side {:?}",
                        profile.name, mod_id, module_side
                    ),
                    vec![NodeId::profile(&profile.id), NodeId::module(mod_id)],
                ));
            }
        }
        diagnostics
    }

    fn find_unknown_sides(graph: &DependencyGraph) -> Vec<Diagnostic> {
        graph
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Mod && node.side == Side::Unknown)
            .map(|node| {
                Diagnostic::warning(
                    "UNKNOWN_SIDE",
                    format!(
                        "Mod {} has unknown side; verify profile compatibility",
                        node.label
                    ),
                    vec![node.id.clone()],
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{graph::DependencyGraph, manifest::ProjectManifest};

    #[test]
    fn sample_manifest_builds_graph_without_errors() {
        let raw = include_str!("../../../examples/sample-project.tuffbox.json");
        let manifest: ProjectManifest = serde_json::from_str(raw).unwrap();
        let graph = DependencyGraph::from_manifest(&manifest);
        let diagnostics = Resolver::analyze(&graph);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics, got {diagnostics:#?}"
        );
    }
}
