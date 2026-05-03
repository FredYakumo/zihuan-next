use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::agent::brain::{Brain, BrainStopReason, BrainTool, MAX_TOOL_ITERATIONS};
use crate::brain_tool::{
    brain_shared_inputs_from_value, BrainToolDefinition, BRAIN_SHARED_INPUTS_PORT,
    BRAIN_TOOLS_CONFIG_PORT,
};
use crate::tool_subgraph::{
    shared_inputs_ports, validate_shared_inputs, validate_tool_definitions, SubgraphFunctionTool,
    ToolResultMode, ToolSubgraphRunner,
};
use zihuan_core::error::{Error, Result};
use zihuan_llm_types::tooling::FunctionTool;
use zihuan_llm_types::OpenAIMessage;
use zihuan_node::function_graph::FunctionPortDef;
use zihuan_node::{DataType, DataValue, Node, Port};

struct SubgraphBrainTool {
    runner: ToolSubgraphRunner,
}

impl BrainTool for SubgraphBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.runner.spec()
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.runner.execute_to_string(call_content, arguments)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// BrainNode
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BrainNode {
    id: String,
    name: String,
    shared_inputs: Vec<FunctionPortDef>,
    tool_definitions: Vec<BrainToolDefinition>,
}

impl BrainNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            shared_inputs: Vec::new(),
            tool_definitions: Vec::new(),
        }
    }

    fn set_shared_inputs(&mut self, shared_inputs: Vec<FunctionPortDef>) -> Result<()> {
        self.shared_inputs = validate_shared_inputs(&shared_inputs, "Brain")?;
        self.tool_definitions = validate_tool_definitions(
            &self.tool_definitions,
            &self.shared_inputs,
            ToolResultMode::JsonObject,
            "brain",
            "Brain",
        )?;
        Ok(())
    }

    fn set_tool_definitions(&mut self, tool_definitions: Vec<BrainToolDefinition>) -> Result<()> {
        self.tool_definitions = validate_tool_definitions(
            &tool_definitions,
            &self.shared_inputs,
            ToolResultMode::JsonObject,
            "brain",
            "Brain",
        )?;
        Ok(())
    }

    fn output_ports_static() -> Vec<Port> {
        vec![
            Port::new("output", DataType::Vec(Box::new(DataType::OpenAIMessage)))
                .with_description("本次 Brain 运行新增的 assistant/tool 消息轨迹"),
        ]
    }

    fn wrap_error(&self, message: impl Into<String>) -> Error {
        Error::ValidationError(format!("[NODE_ERROR:{}] {}", self.id, message.into()))
    }

    pub fn tool_specs(&self) -> Vec<Arc<dyn FunctionTool>> {
        self.tool_definitions
            .iter()
            .cloned()
            .map(|definition| {
                Arc::new(SubgraphFunctionTool::new(definition)) as Arc<dyn FunctionTool>
            })
            .collect()
    }

    fn parse_messages_input(inputs: &HashMap<String, DataValue>) -> Result<Vec<OpenAIMessage>> {
        match inputs.get("messages") {
            Some(DataValue::Vec(_, items)) => Ok(items
                .iter()
                .filter_map(|item| {
                    if let DataValue::OpenAIMessage(message) = item {
                        Some(message.clone())
                    } else {
                        None
                    }
                })
                .collect()),
            _ => Err(Error::ValidationError(
                "Missing required input: messages".to_string(),
            )),
        }
    }

    fn parse_shared_inputs_input(
        &self,
        inputs: &HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let mut values = HashMap::new();
        for port in &self.shared_inputs {
            let value = inputs
                .get(&port.name)
                .ok_or_else(|| self.wrap_error(format!("缺少必填共享输入 {}", port.name)))?;
            values.insert(port.name.clone(), value.clone());
        }
        Ok(values)
    }
}

impl Node for BrainNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用 LLModel 和内置 Tool Loop 执行多轮工具调用推理")
    }

    fn input_ports(&self) -> Vec<Port> {
        let mut ports = vec![
            Port::new("llm_model", DataType::LLModel)
                .with_description("LLM 模型引用，由 LLM API 节点提供"),
            Port::new("messages", DataType::Vec(Box::new(DataType::OpenAIMessage)))
                .with_description("消息列表（包含 system/user/assistant/tool 等角色）"),
            // Hidden ports: managed via "管理工具" button dialog
            Port::new(BRAIN_TOOLS_CONFIG_PORT, DataType::Json)
                .with_description("Tools 配置，由工具编辑器维护")
                .optional()
                .hidden(),
            Port::new(BRAIN_SHARED_INPUTS_PORT, DataType::Json)
                .with_description("Brain 共享输入签名，由工具编辑器维护")
                .optional()
                .hidden(),
        ];
        ports.extend(shared_inputs_ports(&self.shared_inputs, "Brain"));
        ports
    }

    fn output_ports(&self) -> Vec<Port> {
        Self::output_ports_static()
    }

    fn has_dynamic_input_ports(&self) -> bool {
        true
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        match inline_values.get(BRAIN_SHARED_INPUTS_PORT) {
            Some(DataValue::Json(value)) => {
                if value.is_null() {
                    self.set_shared_inputs(Vec::new())?;
                } else {
                    let shared_inputs = brain_shared_inputs_from_value(value).ok_or_else(|| {
                        Error::ValidationError("Invalid shared_inputs".to_string())
                    })?;
                    self.set_shared_inputs(shared_inputs)?;
                }
            }
            Some(other) => {
                return Err(Error::ValidationError(format!(
                    "shared_inputs expects Json, got {}",
                    other.data_type()
                )));
            }
            None => {
                self.set_shared_inputs(Vec::new())?;
            }
        }

        match inline_values.get(BRAIN_TOOLS_CONFIG_PORT) {
            Some(DataValue::Json(value)) => {
                if value.is_null() {
                    self.tool_definitions.clear();
                    return Ok(());
                }
                let parsed = serde_json::from_value::<Vec<BrainToolDefinition>>(value.clone())
                    .map_err(|e| Error::ValidationError(format!("Invalid tools_config: {e}")))?;
                self.set_tool_definitions(parsed)
            }
            Some(other) => Err(Error::ValidationError(format!(
                "tools_config expects Json, got {}",
                other.data_type()
            ))),
            None => {
                self.tool_definitions.clear();
                Ok(())
            }
        }
    }

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        if let Some(DataValue::Json(value)) = inputs.get(BRAIN_SHARED_INPUTS_PORT) {
            let shared_inputs = brain_shared_inputs_from_value(value)
                .ok_or_else(|| Error::ValidationError("Invalid shared_inputs".to_string()))?;
            self.set_shared_inputs(shared_inputs)?;
        }

        if let Some(DataValue::Json(value)) = inputs.get(BRAIN_TOOLS_CONFIG_PORT) {
            let parsed = serde_json::from_value::<Vec<BrainToolDefinition>>(value.clone())
                .map_err(|e| Error::ValidationError(format!("Invalid tools_config: {e}")))?;
            self.set_tool_definitions(parsed)?;
        }

        let model = match inputs.get("llm_model") {
            Some(DataValue::LLModel(model)) => model.clone(),
            _ => return Err(self.wrap_error("缺少必填输入 llm_model")),
        };

        let messages = Self::parse_messages_input(&inputs)?;
        let shared_runtime_values = self.parse_shared_inputs_input(&inputs)?;

        let mut brain = Brain::new(model);
        for tool_def in &self.tool_definitions {
            brain.add_tool(SubgraphBrainTool {
                runner: ToolSubgraphRunner {
                    node_id: self.id.clone(),
                    owner_node_type: "brain".to_string(),
                    shared_inputs: self.shared_inputs.clone(),
                    definition: tool_def.clone(),
                    shared_runtime_values: shared_runtime_values.clone(),
                    result_mode: ToolResultMode::JsonObject,
                },
            });
        }

        let (output_messages, stop_reason) = brain.run(messages);

        match stop_reason {
            BrainStopReason::TransportError(content) => {
                return Err(self.wrap_error(format!("LLM request failed: {content}")));
            }
            BrainStopReason::MaxIterationsReached => {
                return Err(self.wrap_error(format!(
                    "Brain tool loop exceeded max iterations ({MAX_TOOL_ITERATIONS})"
                )));
            }
            BrainStopReason::Done => {}
        }

        let mut outputs = HashMap::new();
        outputs.insert(
            "output".to_string(),
            DataValue::Vec(
                Box::new(DataType::OpenAIMessage),
                output_messages
                    .into_iter()
                    .map(DataValue::OpenAIMessage)
                    .collect(),
            ),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
