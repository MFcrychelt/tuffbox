use crate::graph::NodeId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeAction {
    InstallMod {
        project_id: String,
        version: Option<String>,
    },
    RemoveMod {
        node_id: NodeId,
    },
    DisableMod {
        node_id: NodeId,
    },
    UpdateMod {
        node_id: NodeId,
        target_version: String,
    },
    EditConfig {
        path: String,
        patch: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeRisk {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePlan {
    pub summary: String,
    pub risk: ChangeRisk,
    pub actions: Vec<ChangeAction>,
    pub requires_snapshot: bool,
}
