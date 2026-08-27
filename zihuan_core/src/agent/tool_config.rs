use serde::{Deserialize, Serialize};

use crate::graph::function_graph::FunctionPortDef;
use crate::graph::graph_io::NodeGraphDefinition;
use crate::graph::tool_spec::{PythonScriptToolConfig, ToolParamDef};
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
    PythonScript(PythonScriptAgentToolConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonScriptAgentToolConfig {
    pub script_path: String,
    #[serde(default)]
    pub module_entry: Option<String>,
    #[serde(default)]
    pub python_mode: Option<crate::graph::tool_spec::PythonToolMode>,
    #[serde(default)]
    pub python_runtime: Option<dynamic_script_engine::PythonRuntimeConfig>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub parameters: Vec<ToolParamDef>,
    #[serde(default)]
    pub outputs: Vec<FunctionPortDef>,
}

impl PythonScriptAgentToolConfig {
    pub fn to_runtime_config(&self) -> PythonScriptToolConfig {
        PythonScriptToolConfig {
            script_path: self.script_path.clone(),
            module_entry: self
                .module_entry
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "run_tool".to_string()),
            python_runtime: self.python_runtime.clone(),
            python_mode: self.python_mode,
            timeout_secs: self.timeout_secs.unwrap_or(60),
        }
    }
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
