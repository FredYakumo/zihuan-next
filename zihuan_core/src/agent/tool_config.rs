use serde::{Deserialize, Serialize};

use crate::graph::function_graph::FunctionPortDef;
use crate::graph::graph_io::NodeGraphDefinition;
use crate::graph::tool_spec::{ScriptToolConfig, ToolParamDef};
use crate::tool_runtime::ToolRunDuration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub run_duration: ToolRunDuration,
    pub tool_type: AgentToolType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentToolType {
    NodeGraph(NodeGraphToolConfig),
    SubAgent(SubAgentToolConfig),
    Script(ScriptAgentToolConfig),
    Plugin(PluginToolConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentToolConfig {
    pub sub_agent_id: String,
}

/// A tool supplied by an installed `brain_tool` plugin.
///
/// The plugin owns the LLM-facing signature; this entry only selects which plugin the service
/// exposes, so the same plugin directory can be shared by several services.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginToolConfig {
    /// The installed plugin's `name`, not its tool name — one plugin declares one tool.
    pub plugin: String,
}

/// An agent tool implemented by a script rather than by a graph.
///
/// The script signature the LLM sees is declared here (`parameters` / `outputs`), so it stays
/// editable without reading the script; a script that exports a manifest can fill both in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptAgentToolConfig {
    #[serde(flatten)]
    pub script: ScriptToolConfig,
    #[serde(default)]
    pub parameters: Vec<ToolParamDef>,
    #[serde(default)]
    pub outputs: Vec<FunctionPortDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "target_type", rename_all = "snake_case")]
pub enum NodeGraphToolConfig {
    FilePath {
        path: String,
        #[serde(default)]
        parameters: Vec<ToolParamDef>,
        #[serde(default)]
        outputs: Vec<FunctionPortDef>,
    },
    WorkflowSet {
        name: String,
        #[serde(default)]
        parameters: Vec<ToolParamDef>,
        #[serde(default)]
        outputs: Vec<FunctionPortDef>,
    },
    InlineGraph {
        graph: NodeGraphDefinition,
        #[serde(default)]
        parameters: Vec<ToolParamDef>,
        #[serde(default)]
        outputs: Vec<FunctionPortDef>,
    },
}
