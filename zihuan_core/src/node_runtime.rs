use serde::{Deserialize, Serialize};

/// Selects the Node.js executable used by the dynamic script runtime.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeRuntimeKind {
    /// Resolve `node` from PATH and run the checked-in `dynamic_script_engine` project.
    #[default]
    ProjectNode,
    CustomExecutable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeRuntimeConfig {
    #[serde(default)]
    pub kind: NodeRuntimeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
}

impl Default for NodeRuntimeConfig {
    fn default() -> Self {
        Self { kind: NodeRuntimeKind::ProjectNode, executable_path: None }
    }
}

impl From<NodeRuntimeKind> for NodeRuntimeConfig {
    fn from(kind: NodeRuntimeKind) -> Self {
        Self { kind, executable_path: None }
    }
}
