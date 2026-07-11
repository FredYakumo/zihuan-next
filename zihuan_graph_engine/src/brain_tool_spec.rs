use serde::{Deserialize, Serialize};
pub use zihuan_core::tool_runtime::ToolRunDuration;

use crate::function_graph::{default_function_subgraph, FunctionPortDef, FUNCTION_OUTPUTS_NODE_ID};
use crate::graph_io::NodeGraphDefinition;
use crate::DataType;

pub const BRAIN_TOOLS_CONFIG_PORT: &str = "tools_config";
pub const BRAIN_SHARED_INPUTS_PORT: &str = "shared_inputs";
pub const BRAIN_TOOL_FIXED_CONTENT_INPUT: &str = "content";
pub const QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT: &str = "message_event";
pub const QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT: &str = "qq_ims_bot_adapter";
pub const QQ_AGENT_TOOL_OWNER_TYPE: &str = "qq_chat";
pub const QQ_AGENT_TOOL_OUTPUT_NAME: &str = "result";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BrainToolImplementation {
    #[default]
    NodeGraph,
    BuiltIn,
    PythonScript,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuiltInBrainToolKind {
    ImageUnderstand,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PythonToolMode {
    #[default]
    UvProject,
    VenvPython,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PythonScriptToolConfig {
    pub script_path: String,
    #[serde(default = "default_python_module_entry")]
    pub module_entry: String,
    #[serde(default)]
    pub python_mode: PythonToolMode,
    #[serde(default = "default_python_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_python_module_entry() -> String {
    "run_tool".to_string()
}

fn default_python_timeout_secs() -> u64 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolParamDef {
    pub name: String,
    pub data_type: DataType,
    #[serde(default)]
    pub desc: String,
    #[serde(default = "default_tool_param_required")]
    pub required: bool,
}

fn default_tool_param_required() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BrainToolDefinition {
    #[serde(default = "default_tool_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub run_duration: ToolRunDuration,
    #[serde(default)]
    pub implementation: BrainToolImplementation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_in_kind: Option<BuiltInBrainToolKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub python_config: Option<PythonScriptToolConfig>,
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
        if self.uses_subgraph() && self.subgraph.nodes.is_empty() {
            self.subgraph = default_function_subgraph();
        }
    }

    pub fn uses_subgraph(&self) -> bool {
        matches!(self.implementation, BrainToolImplementation::NodeGraph)
    }

    pub fn builtin_kind(&self) -> Option<BuiltInBrainToolKind> {
        self.built_in_kind
    }

    pub fn python_config(&self) -> Option<&PythonScriptToolConfig> {
        self.python_config.as_ref()
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
                description: param.desc.clone(),
                required: param.required,
            })
            .collect()
    }
}

pub fn brain_shared_inputs_from_value(value: &serde_json::Value) -> Option<Vec<FunctionPortDef>> {
    serde_json::from_value::<Vec<FunctionPortDef>>(value.clone()).ok()
}

pub fn fixed_tool_runtime_inputs(owner_node_type: &str) -> Vec<FunctionPortDef> {
    match owner_node_type {
        QQ_AGENT_TOOL_OWNER_TYPE => vec![
            FunctionPortDef {
                name: BRAIN_TOOL_FIXED_CONTENT_INPUT.to_string(),
                data_type: DataType::String,
                description: "触发此次工具调用的上下文文本内容".to_string(),
                required: true,
            },
            FunctionPortDef {
                name: QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT.to_string(),
                data_type: DataType::MessageEvent,
                description: "当前触发此次工具调用的消息事件".to_string(),
                required: true,
            },
            FunctionPortDef {
                name: QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT.to_string(),
                data_type: DataType::BotAdapterRef,
                description: "当前消息事件对应的 Bot Adapter 连接引用".to_string(),
                required: true,
            },
        ],
        _ => vec![FunctionPortDef {
            name: BRAIN_TOOL_FIXED_CONTENT_INPUT.to_string(),
            data_type: DataType::String,
            description: "触发此次工具调用的上下文文本内容".to_string(),
            required: true,
        }],
    }
}

pub fn brain_tool_input_signature(
    owner_node_type: &str,
    shared_inputs: &[FunctionPortDef],
    tool: &BrainToolDefinition,
) -> Vec<FunctionPortDef> {
    let mut signature = shared_inputs.to_vec();
    signature.extend(fixed_tool_runtime_inputs(owner_node_type));
    signature.extend(tool.input_signature());
    signature
}

pub fn tool_subgraph_owner_uses_brain_outputs(node_type: &str) -> bool {
    node_type == "brain"
}

pub fn tool_subgraph_owner_types() -> [&'static str; 2] {
    ["brain", QQ_AGENT_TOOL_OWNER_TYPE]
}

pub fn is_tool_subgraph_owner(node_type: &str) -> bool {
    tool_subgraph_owner_types().contains(&node_type)
}

pub fn normalized_tool_outputs_for_owner(node_type: &str, tool: &BrainToolDefinition) -> Vec<FunctionPortDef> {
    if tool_subgraph_owner_uses_brain_outputs(node_type) {
        tool.outputs.clone()
    } else {
        vec![FunctionPortDef {
            name: QQ_AGENT_TOOL_OUTPUT_NAME.to_string(),
            data_type: crate::DataType::String,
            description: "工具返回给 Agent 的文本结果".to_string(),
            required: true,
        }]
    }
}
