use serde::{Deserialize, Serialize};

use crate::function_graph::{default_function_subgraph, FunctionPortDef, FUNCTION_OUTPUTS_NODE_ID};
use crate::graph_io::NodeGraphDefinition;
use crate::DataType;

pub const BRAIN_TOOLS_CONFIG_PORT: &str = "tools_config";
pub const BRAIN_SHARED_INPUTS_PORT: &str = "shared_inputs";
pub const BRAIN_TOOL_FIXED_CONTENT_INPUT: &str = "content";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolParamDef {
    pub name: String,
    pub data_type: DataType,
    #[serde(default)]
    pub desc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrainToolDefinition {
    #[serde(default = "default_tool_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<ToolParamDef>,
    #[serde(default)]
    pub outputs: Vec<FunctionPortDef>,
    #[serde(default = "default_function_subgraph")]
    pub subgraph: NodeGraphDefinition,
}

fn default_tool_id() -> String {
    "tool".to_string()
}

impl BrainToolDefinition {
    pub fn ensure_defaults(&mut self, fallback_index: usize) {
        if self.id.trim().is_empty() {
            self.id = format!("tool_{fallback_index}");
        }
        if self.subgraph.nodes.is_empty() {
            self.subgraph = default_function_subgraph();
        }
    }

    pub fn output_boundary_node_id() -> &'static str {
        FUNCTION_OUTPUTS_NODE_ID
    }

    pub fn input_signature(&self) -> Vec<FunctionPortDef> {
        self.parameters
            .iter()
            .map(|param| FunctionPortDef {
                name: param.name.clone(),
                data_type: param.data_type.clone(),
            })
            .collect()
    }
}

pub fn brain_shared_inputs_from_value(value: &serde_json::Value) -> Option<Vec<FunctionPortDef>> {
    serde_json::from_value::<Vec<FunctionPortDef>>(value.clone()).ok()
}

pub fn brain_tool_input_signature(
    shared_inputs: &[FunctionPortDef],
    tool: &BrainToolDefinition,
) -> Vec<FunctionPortDef> {
    let mut signature = shared_inputs.to_vec();
    signature.push(FunctionPortDef {
        name: BRAIN_TOOL_FIXED_CONTENT_INPUT.to_string(),
        data_type: DataType::String,
    });
    signature.extend(tool.input_signature());
    signature
}
