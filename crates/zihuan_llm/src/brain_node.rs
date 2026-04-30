use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use log::info;
use serde_json::{json, Map, Value};

use zihuan_core::error::{Error, Result};
use crate::brain_tool::{
    brain_shared_inputs_from_value, brain_tool_input_signature, BrainToolDefinition, ToolParamDef,
    BRAIN_SHARED_INPUTS_PORT, BRAIN_TOOLS_CONFIG_PORT, BRAIN_TOOL_FIXED_CONTENT_INPUT,
};
use crate::agent::brain::{
    Brain, BrainStopReason, BrainTool, MAX_TOOL_ITERATIONS,
};
use zihuan_llm_types::tooling::FunctionTool;
use zihuan_llm_types::OpenAIMessage;
use zihuan_node::function_graph::{
    sync_function_subgraph_signature, FunctionPortDef, FUNCTION_INPUTS_NODE_ID,
    FUNCTION_OUTPUTS_NODE_ID,
};
use zihuan_node::graph_io::refresh_port_types;
use zihuan_node::registry::{build_node_graph_from_definition, NODE_REGISTRY};
use zihuan_node::util::function::{
    data_value_from_json_with_declared_type, inject_runtime_values_into_function_inputs_node,
};
use zihuan_node::{DataType, DataValue, Node, Port};

#[derive(Debug, Clone)]
struct BrainFunctionTool {
    definition: BrainToolDefinition,
}

impl BrainFunctionTool {
    fn new(definition: BrainToolDefinition) -> Self {
        Self { definition }
    }
}

impl FunctionTool for BrainFunctionTool {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters(&self) -> Value {
        tool_parameters_to_json_schema(&self.definition.parameters)
    }

    fn call(&self, arguments: Value) -> Result<Value> {
        Ok(arguments)
    }
}

fn data_type_to_json_schema_type(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::String | DataType::Password => "string",
        DataType::Integer => "integer",
        DataType::Float => "number",
        DataType::Boolean => "boolean",
        DataType::Binary => "string",
        DataType::Vec(_) => "array",
        DataType::Json
        | DataType::MessageEvent
        | DataType::OpenAIMessage
        | DataType::QQMessage
        | DataType::FunctionTools
        | DataType::BotAdapterRef
        | DataType::RedisRef
        | DataType::MySqlRef
        | DataType::TavilyRef
        | DataType::SessionStateRef
        | DataType::OpenAIMessageSessionCacheRef
        | DataType::LLModel
        | DataType::LoopControlRef
        | DataType::Custom(_)
        | DataType::Any => "object",
    }
}

fn tool_parameters_to_json_schema(parameters: &[ToolParamDef]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();

    for param in parameters {
        let param_name = param.name.trim();
        if param_name.is_empty() {
            continue;
        }
        required.push(Value::String(param_name.to_string()));
        properties.insert(
            param_name.to_string(),
            json!({
                "type": data_type_to_json_schema_type(&param.data_type),
                "description": if param.desc.trim().is_empty() {
                    format!("参数 {}", param_name)
                } else {
                    param.desc.clone()
                },
            }),
        );
    }

    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

fn shared_inputs_ports(shared_inputs: &[FunctionPortDef]) -> Vec<Port> {
    shared_inputs
        .iter()
        .map(|port| port.to_port(format!("Brain 共享输入 '{}'", port.name)))
        .collect()
}

fn validate_shared_inputs(shared_inputs: &[FunctionPortDef]) -> Result<Vec<FunctionPortDef>> {
    let mut seen_names = HashSet::new();
    let mut normalized = Vec::with_capacity(shared_inputs.len());

    for port in shared_inputs.iter().cloned() {
        let name = port.name.trim();
        if name.is_empty() {
            return Err(Error::ValidationError(
                "Brain shared input name cannot be empty".to_string(),
            ));
        }
        if !seen_names.insert(name.to_string()) {
            return Err(Error::ValidationError(format!(
                "Duplicate Brain shared input name: {name}"
            )));
        }
        normalized.push(FunctionPortDef {
            name: name.to_string(),
            data_type: port.data_type,
        });
    }

    Ok(normalized)
}

fn validate_tool_definitions(
    tool_definitions: &[BrainToolDefinition],
    shared_inputs: &[FunctionPortDef],
) -> Result<Vec<BrainToolDefinition>> {
    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();
    let shared_input_names = shared_inputs
        .iter()
        .map(|port| port.name.trim().to_string())
        .collect::<HashSet<_>>();
    let mut normalized = Vec::with_capacity(tool_definitions.len());

    for (index, tool) in tool_definitions.iter().cloned().enumerate() {
        let mut tool = tool;
        tool.ensure_defaults(index + 1);

        let tool_id = tool.id.trim();
        let tool_name = tool.name.trim();
        if tool_id.is_empty() {
            return Err(Error::ValidationError("Tool id cannot be empty".to_string()));
        }
        if tool_name.is_empty() {
            return Err(Error::ValidationError("Tool name cannot be empty".to_string()));
        }
        if !seen_ids.insert(tool_id.to_string()) {
            return Err(Error::ValidationError(format!("Duplicate tool id: {tool_id}")));
        }
        if !seen_names.insert(tool_name.to_string()) {
            return Err(Error::ValidationError(format!("Duplicate tool name: {tool_name}")));
        }

        let mut seen_param_names = HashSet::new();
        for param in &tool.parameters {
            let param_name = param.name.trim();
            if param_name.is_empty() {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' has an empty parameter name",
                    tool_name
                )));
            }
            if shared_input_names.contains(param_name) {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' parameter '{}' conflicts with Brain shared input",
                    tool_name, param_name
                )));
            }
            if param_name == BRAIN_TOOL_FIXED_CONTENT_INPUT {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' parameter '{}' is reserved for Brain tool-call content",
                    tool_name, param_name
                )));
            }
            if !seen_param_names.insert(param_name.to_string()) {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' has duplicate parameter '{}'",
                    tool_name, param_name
                )));
            }
        }

        let input_signature = brain_tool_input_signature(shared_inputs, &tool);
        sync_function_subgraph_signature(&mut tool.subgraph, &input_signature, &tool.outputs);
        normalized.push(tool);
    }

    Ok(normalized)
}

fn build_tool_error_message(message: impl Into<String>) -> String {
    Value::Object(Map::from_iter([(
        "error".to_string(),
        Value::String(message.into()),
    )]))
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// SubgraphBrainTool — wraps a BrainToolDefinition for use with Brain
// ─────────────────────────────────────────────────────────────────────────────

struct SubgraphBrainTool {
    node_id: String,
    shared_inputs: Vec<FunctionPortDef>,
    definition: BrainToolDefinition,
    shared_runtime_values: HashMap<String, DataValue>,
}

impl BrainTool for SubgraphBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(BrainFunctionTool::new(self.definition.clone()))
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        match self.run_subgraph(call_content.to_string(), arguments.clone()) {
            Ok(result) => result,
            Err(e) => build_tool_error_message(e.to_string()),
        }
    }
}

impl SubgraphBrainTool {
    fn wrap_error(&self, msg: impl Into<String>) -> Error {
        Error::ValidationError(format!("[NODE_ERROR:{}] {}", self.node_id, msg.into()))
    }

    fn run_subgraph(&self, tool_call_content: String, arguments: Value) -> Result<String> {
        let tool = &self.definition;
        for node in &tool.subgraph.nodes {
            if NODE_REGISTRY.is_event_producer(&node.node_type) {
                return Err(self.wrap_error(format!(
                    "Tool '{}' 子图内不允许事件源节点：{} ({})",
                    tool.name, node.name, node.node_type
                )));
            }
        }

        let tool_runtime_values = match arguments {
            Value::Object(map) => map,
            Value::Null => Map::new(),
            other => {
                return Err(self.wrap_error(format!(
                    "Tool '{}' 的参数必须是 JSON 对象，实际为 {}",
                    tool.name, other
                )));
            }
        };

        let mut runtime_values = self.shared_runtime_values.clone();
        runtime_values.insert(
            BRAIN_TOOL_FIXED_CONTENT_INPUT.to_string(),
            DataValue::String(tool_call_content),
        );
        for (key, value) in tool_runtime_values {
            if runtime_values.contains_key(&key) {
                return Err(self.wrap_error(format!(
                    "Tool '{}' 参数 '{}' 与 Brain 共享输入重名",
                    tool.name, key
                )));
            }
            let parsed_value = tool
                .parameters
                .iter()
                .find(|param| param.name == key)
                .map(|param| {
                    data_value_from_json_with_declared_type(
                        &FunctionPortDef {
                            name: param.name.clone(),
                            data_type: param.data_type.clone(),
                        },
                        &value,
                    )
                })
                .transpose()?
                .unwrap_or(DataValue::Json(value));
            runtime_values.insert(key, parsed_value);
        }

        let input_signature = brain_tool_input_signature(&self.shared_inputs, tool);
        let mut subgraph = tool.subgraph.clone();
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &tool.outputs);
        refresh_port_types(&mut subgraph);

        let function_inputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_INPUTS_NODE_ID)
            .ok_or_else(|| self.wrap_error(format!("Tool '{}' 缺少 function_inputs 边界节点", tool.name)))?;
        function_inputs_node.inline_values.insert(
            zihuan_node::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&input_signature).unwrap_or(Value::Null),
        );

        let function_outputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| self.wrap_error(format!("Tool '{}' 缺少 function_outputs 边界节点", tool.name)))?;
        function_outputs_node.inline_values.insert(
            zihuan_node::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&tool.outputs).unwrap_or(Value::Null),
        );

        let mut graph = build_node_graph_from_definition(&subgraph)
            .map_err(|e| self.wrap_error(format!("Tool '{}' 子图构建失败: {e}", tool.name)))?;
        inject_runtime_values_into_function_inputs_node(&mut graph, runtime_values).map_err(|e| {
            self.wrap_error(format!("Tool '{}' 注入子图运行时输入失败: {e}", tool.name))
        })?;
        let execution_result = graph.execute_and_capture_results();
        if let Some(error_message) = execution_result.error_message {
            return Err(
                self.wrap_error(format!("Tool '{}' 子图执行失败: {error_message}", tool.name))
            );
        }

        let mut result_payload = Map::new();
        if let Some(result_node_values) = execution_result.node_results.get(FUNCTION_OUTPUTS_NODE_ID) {
            for port in &tool.outputs {
                let value = result_node_values.get(&port.name).ok_or_else(|| {
                    self.wrap_error(format!(
                        "Tool '{}' 输出 '{}' 未在子图中提供",
                        tool.name, port.name
                    ))
                })?;
                if !port.data_type.is_compatible_with(&value.data_type()) {
                    return Err(self.wrap_error(format!(
                        "Tool '{}' 输出 '{}' 类型不匹配：声明为 {}，实际为 {}",
                        tool.name,
                        port.name,
                        port.data_type,
                        value.data_type()
                    )));
                }
                result_payload.insert(port.name.clone(), value.to_json());
            }
        }

        Ok(Value::Object(result_payload).to_string())
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
        self.shared_inputs = validate_shared_inputs(&shared_inputs)?;
        self.tool_definitions = validate_tool_definitions(&self.tool_definitions, &self.shared_inputs)?;
        Ok(())
    }

    fn set_tool_definitions(&mut self, tool_definitions: Vec<BrainToolDefinition>) -> Result<()> {
        self.tool_definitions = validate_tool_definitions(&tool_definitions, &self.shared_inputs)?;
        Ok(())
    }

    fn output_ports_static() -> Vec<Port> {
        vec![Port::new("output", DataType::Vec(Box::new(DataType::OpenAIMessage)))
            .with_description("本次 Brain 运行新增的 assistant/tool 消息轨迹")]
    }

    fn wrap_error(&self, message: impl Into<String>) -> Error {
        Error::ValidationError(format!("[NODE_ERROR:{}] {}", self.id, message.into()))
    }

    pub fn tool_specs(&self) -> Vec<Arc<dyn FunctionTool>> {
        self.tool_definitions
            .iter()
            .cloned()
            .map(|definition| Arc::new(BrainFunctionTool::new(definition)) as Arc<dyn FunctionTool>)
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
        ports.extend(shared_inputs_ports(&self.shared_inputs));
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

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
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
                node_id: self.id.clone(),
                shared_inputs: self.shared_inputs.clone(),
                definition: tool_def.clone(),
                shared_runtime_values: shared_runtime_values.clone(),
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
                output_messages.into_iter().map(DataValue::OpenAIMessage).collect(),
            ),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, Once};

    use serde_json::json;

    use super::BrainNode;
    use crate::brain_tool::{
        BrainToolDefinition, ToolParamDef, BRAIN_SHARED_INPUTS_PORT, BRAIN_TOOLS_CONFIG_PORT,
    };
    use zihuan_llm_types::llm_base::LLMBase;
    use zihuan_llm_types::tooling::{ToolCalls, ToolCallsFuncSpec};
    use zihuan_llm_types::{InferenceParam, MessageRole, OpenAIMessage};
    use zihuan_node::function_graph::{
        default_function_subgraph, sync_function_subgraph_signature, FunctionPortDef,
        FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
    };
    use zihuan_node::graph_io::EdgeDefinition;
    use zihuan_node::{DataType, DataValue, Node};

    #[derive(Debug)]
    struct SequenceLlm {
        responses: Mutex<Vec<OpenAIMessage>>,
        seen_messages: Mutex<Vec<Vec<OpenAIMessage>>>,
    }

    impl SequenceLlm {
        fn new(responses: Vec<OpenAIMessage>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().rev().collect()),
                seen_messages: Mutex::new(Vec::new()),
            }
        }
    }

    impl LLMBase for SequenceLlm {
        fn get_model_name(&self) -> &str {
            "sequence-llm"
        }

        fn inference(&self, param: &InferenceParam) -> OpenAIMessage {
            self.seen_messages
                .lock()
                .unwrap()
                .push(param.messages.clone());
            self.responses
                .lock()
                .unwrap()
                .pop()
                .expect("missing test response")
        }
    }

    fn ensure_registry_initialized() {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            zihuan_node::registry::init_node_registry().expect("registry should initialize");
        });
    }

    fn passthrough_tool_definition(name: &str) -> BrainToolDefinition {
        let mut subgraph = default_function_subgraph();
        let input_signature = vec![FunctionPortDef {
            name: "query".to_string(),
            data_type: DataType::String,
        }];
        let output_signature = vec![FunctionPortDef {
            name: "query".to_string(),
            data_type: DataType::String,
        }];
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &output_signature);
        subgraph.edges.push(EdgeDefinition {
            from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
            from_port: "query".to_string(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: "query".to_string(),
        });

        BrainToolDefinition {
            id: format!("{name}_id"),
            name: name.to_string(),
            description: format!("tool {name}"),
            parameters: vec![ToolParamDef {
                name: "query".to_string(),
                data_type: DataType::String,
                desc: "query".to_string(),
            }],
            outputs: vec![FunctionPortDef {
                name: "query".to_string(),
                data_type: DataType::String,
            }],
            subgraph,
        }
    }

    fn tool_using_shared_input(name: &str) -> BrainToolDefinition {
        let mut subgraph = default_function_subgraph();
        let input_signature = vec![
            FunctionPortDef {
                name: "context".to_string(),
                data_type: DataType::Json,
            },
            FunctionPortDef {
                name: "query".to_string(),
                data_type: DataType::String,
            },
        ];
        let output_signature = vec![
            FunctionPortDef {
                name: "context".to_string(),
                data_type: DataType::Json,
            },
            FunctionPortDef {
                name: "query".to_string(),
                data_type: DataType::String,
            },
        ];
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &output_signature);
        subgraph.edges.push(EdgeDefinition {
            from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
            from_port: "context".to_string(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: "context".to_string(),
        });
        subgraph.edges.push(EdgeDefinition {
            from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
            from_port: "query".to_string(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: "query".to_string(),
        });

        BrainToolDefinition {
            id: format!("{name}_id"),
            name: name.to_string(),
            description: format!("tool {name}"),
            parameters: vec![ToolParamDef {
                name: "query".to_string(),
                data_type: DataType::String,
                desc: "query".to_string(),
            }],
            outputs: output_signature,
            subgraph,
        }
    }

    fn tool_using_shared_llm_input(name: &str) -> BrainToolDefinition {
        let mut subgraph = default_function_subgraph();
        let input_signature = vec![FunctionPortDef {
            name: "llm_ref".to_string(),
            data_type: DataType::LLModel,
        }];
        let output_signature = vec![FunctionPortDef {
            name: "llm_ref".to_string(),
            data_type: DataType::LLModel,
        }];
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &output_signature);
        subgraph.edges.push(EdgeDefinition {
            from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
            from_port: "llm_ref".to_string(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: "llm_ref".to_string(),
        });

        BrainToolDefinition {
            id: format!("{name}_id"),
            name: name.to_string(),
            description: format!("tool {name}"),
            parameters: Vec::new(),
            outputs: output_signature,
            subgraph,
        }
    }

    fn messages_input() -> DataValue {
        DataValue::Vec(
            Box::new(DataType::OpenAIMessage),
            vec![DataValue::OpenAIMessage(OpenAIMessage::user("hello"))],
        )
    }

    #[test]
    fn brain_output_is_static_output_message_list_only() {
        ensure_registry_initialized();
        let node = BrainNode::new("brain_1", "Brain");
        let output_names: Vec<String> = node.output_ports().into_iter().map(|port| port.name).collect();
        assert_eq!(output_names, vec!["output"]);
    }

    #[test]
    fn brain_input_ports_include_shared_inputs() {
        ensure_registry_initialized();
        let mut node = BrainNode::new("brain_1", "Brain");
        node.apply_inline_config(&HashMap::from([(
            BRAIN_SHARED_INPUTS_PORT.to_string(),
            DataValue::Json(json!([
                { "name": "context", "data_type": "Json" },
                { "name": "sender_id", "data_type": "String" }
            ])),
        )]))
        .unwrap();

        let input_names: Vec<String> = node.input_ports().into_iter().map(|port| port.name).collect();
        assert!(input_names.contains(&"llm_model".to_string()));
        assert!(input_names.contains(&"messages".to_string()));
        assert!(input_names.contains(&"tools_config".to_string()));
        assert!(input_names.contains(&"shared_inputs".to_string()));
        assert!(input_names.contains(&"context".to_string()));
        assert!(input_names.contains(&"sender_id".to_string()));
    }

    #[test]
    fn execute_runs_internal_tool_loop_and_returns_output_message_list() {
        ensure_registry_initialized();
        let tool_call_message = OpenAIMessage {
            role: MessageRole::Assistant,
            content: Some("calling tool".to_string()),
            tool_calls: vec![ToolCalls {
                id: "tool_call_1".to_string(),
                type_name: "function".to_string(),
                function: ToolCallsFuncSpec {
                    name: "search".to_string(),
                    arguments: json!({"query": "rust"}),
                },
            }],
            tool_call_id: None,
        };
        let final_message = OpenAIMessage {
            role: MessageRole::Assistant,
            content: Some("done".to_string()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        };
        let llm = Arc::new(SequenceLlm::new(vec![
            tool_call_message.clone(),
            final_message.clone(),
        ]));

        let mut node = BrainNode::new("brain_1", "Brain");
        node.apply_inline_config(&HashMap::from([(
            BRAIN_TOOLS_CONFIG_PORT.to_string(),
            DataValue::Json(json!([passthrough_tool_definition("search")])),
        )]))
        .unwrap();

        let outputs = node
            .execute(HashMap::from([
                ("llm_model".to_string(), DataValue::LLModel(llm.clone())),
                ("messages".to_string(), messages_input()),
            ]))
            .unwrap();

        match outputs.get("output") {
            Some(DataValue::Vec(_, items)) => {
                assert_eq!(items.len(), 3);
                match &items[0] {
                    DataValue::OpenAIMessage(message) => {
                        assert_eq!(message.content.as_deref(), Some("calling tool"));
                        assert_eq!(message.role, MessageRole::Assistant);
                    }
                    other => panic!("unexpected first output item: {other:?}"),
                }
                match &items[1] {
                    DataValue::OpenAIMessage(message) => {
                        assert_eq!(message.role, MessageRole::Tool);
                        assert_eq!(message.content.as_deref(), Some("{\"query\":\"rust\"}"));
                    }
                    other => panic!("unexpected second output item: {other:?}"),
                }
                match &items[2] {
                    DataValue::OpenAIMessage(message) => {
                        assert_eq!(message.content.as_deref(), Some("done"));
                        assert!(message.tool_calls.is_empty());
                    }
                    other => panic!("unexpected third output item: {other:?}"),
                }
            }
            other => panic!("unexpected output: {other:?}"),
        }

        let seen_messages = llm.seen_messages.lock().unwrap();
        assert_eq!(seen_messages.len(), 2);
        assert_eq!(seen_messages[1].len(), 3);
        assert_eq!(seen_messages[1][2].role, MessageRole::Tool);
        assert_eq!(
            seen_messages[1][2].content.as_deref(),
            Some("{\"query\":\"rust\"}")
        );
    }

    #[test]
    fn execute_returns_unknown_tool_as_tool_error_message_and_continues() {
        ensure_registry_initialized();
        let llm = Arc::new(SequenceLlm::new(vec![
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("call missing".to_string()),
                tool_calls: vec![ToolCalls {
                    id: "missing_1".to_string(),
                    type_name: "function".to_string(),
                    function: ToolCallsFuncSpec {
                        name: "missing_tool".to_string(),
                        arguments: json!({}),
                    },
                }],
                tool_call_id: None,
            },
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("recovered".to_string()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        ]));

        let mut node = BrainNode::new("brain_1", "Brain");

        let outputs = node
            .execute(HashMap::from([
                ("llm_model".to_string(), DataValue::LLModel(llm.clone())),
                ("messages".to_string(), messages_input()),
                (BRAIN_TOOLS_CONFIG_PORT.to_string(), DataValue::Json(json!([]))),
            ]))
            .unwrap();

        match outputs.get("output") {
            Some(DataValue::Vec(_, items)) => {
                assert_eq!(items.len(), 3);
                match &items[2] {
                    DataValue::OpenAIMessage(message) => {
                        assert_eq!(message.content.as_deref(), Some("recovered"));
                    }
                    other => panic!("unexpected last output item: {other:?}"),
                }
            }
            other => panic!("unexpected output: {other:?}"),
        }

        let seen_messages = llm.seen_messages.lock().unwrap();
        assert_eq!(seen_messages[1][2].role, MessageRole::Tool);
        assert_eq!(
            seen_messages[1][2].content.as_deref(),
            Some("{\"error\":\"Tool 'missing_tool' not found\"}")
        );
    }

    #[test]
    fn execute_returns_no_tool_call_response_directly() {
        ensure_registry_initialized();
        let llm = Arc::new(SequenceLlm::new(vec![OpenAIMessage {
            role: MessageRole::Assistant,
            content: Some("plain reply".to_string()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }]));

        let mut node = BrainNode::new("brain_1", "Brain");
        let outputs = node
            .execute(HashMap::from([
                ("llm_model".to_string(), DataValue::LLModel(llm)),
                ("messages".to_string(), messages_input()),
            ]))
            .unwrap();

        match outputs.get("output") {
            Some(DataValue::Vec(_, items)) => {
                assert_eq!(items.len(), 1);
                match &items[0] {
                    DataValue::OpenAIMessage(message) => {
                        assert_eq!(message.content.as_deref(), Some("plain reply"));
                    }
                    other => panic!("unexpected output item: {other:?}"),
                }
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn tool_specs_expose_only_tool_parameters_to_llm() {
        ensure_registry_initialized();
        let mut node = BrainNode::new("brain_1", "Brain");
        node.apply_inline_config(&HashMap::from([
            (
                BRAIN_SHARED_INPUTS_PORT.to_string(),
                DataValue::Json(json!([{ "name": "context", "data_type": "Json" }])),
            ),
            (
                BRAIN_TOOLS_CONFIG_PORT.to_string(),
                DataValue::Json(json!([tool_using_shared_input("search")])),
            ),
        ]))
        .unwrap();

        let tools = node.tool_specs();
        let params = tools[0].parameters();
        assert!(params["properties"].get("query").is_some());
        assert!(params["properties"].get("context").is_none());
    }

    #[test]
    fn execute_injects_shared_inputs_into_tool_subgraph() {
        ensure_registry_initialized();
        let llm = Arc::new(SequenceLlm::new(vec![
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("calling tool".to_string()),
                tool_calls: vec![ToolCalls {
                    id: "tool_call_1".to_string(),
                    type_name: "function".to_string(),
                    function: ToolCallsFuncSpec {
                        name: "search".to_string(),
                        arguments: json!({"query": "rust"}),
                    },
                }],
                tool_call_id: None,
            },
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("done".to_string()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        ]));

        let mut node = BrainNode::new("brain_1", "Brain");
        node.apply_inline_config(&HashMap::from([
            (
                BRAIN_SHARED_INPUTS_PORT.to_string(),
                DataValue::Json(json!([{ "name": "context", "data_type": "Json" }])),
            ),
            (
                BRAIN_TOOLS_CONFIG_PORT.to_string(),
                DataValue::Json(json!([tool_using_shared_input("search")])),
            ),
        ]))
        .unwrap();

        node.execute(HashMap::from([
            ("llm_model".to_string(), DataValue::LLModel(llm.clone())),
            ("messages".to_string(), messages_input()),
            (
                "context".to_string(),
                DataValue::Json(json!({"scope": "global"})),
            ),
        ]))
        .unwrap();

        let seen_messages = llm.seen_messages.lock().unwrap();
        assert_eq!(
            seen_messages[1][2].content.as_deref(),
            Some("{\"context\":{\"scope\":\"global\"},\"query\":\"rust\"}")
        );
    }

    #[test]
    fn execute_preserves_shared_llm_inputs_into_tool_subgraph() {
        ensure_registry_initialized();
        let agent_llm = Arc::new(SequenceLlm::new(vec![
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("calling tool".to_string()),
                tool_calls: vec![ToolCalls {
                    id: "tool_call_1".to_string(),
                    type_name: "function".to_string(),
                    function: ToolCallsFuncSpec {
                        name: "search".to_string(),
                        arguments: json!({}),
                    },
                }],
                tool_call_id: None,
            },
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("done".to_string()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        ]));
        let shared_llm = Arc::new(SequenceLlm::new(Vec::new()));

        let mut node = BrainNode::new("brain_1", "Brain");
        node.apply_inline_config(&HashMap::from([
            (
                BRAIN_SHARED_INPUTS_PORT.to_string(),
                DataValue::Json(json!([{ "name": "llm_ref", "data_type": "LLModel" }])),
            ),
            (
                BRAIN_TOOLS_CONFIG_PORT.to_string(),
                DataValue::Json(json!([tool_using_shared_llm_input("search")])),
            ),
        ]))
        .unwrap();

        let outputs = node
            .execute(HashMap::from([
                ("llm_model".to_string(), DataValue::LLModel(agent_llm)),
                ("messages".to_string(), messages_input()),
                ("llm_ref".to_string(), DataValue::LLModel(shared_llm.clone())),
            ]))
            .unwrap();

        match outputs.get("output") {
            Some(DataValue::Vec(_, items)) => match &items[1] {
                DataValue::OpenAIMessage(message) => {
                    assert_eq!(message.role, MessageRole::Tool);
                    assert_eq!(
                        message.content.as_deref(),
                        Some("{\"llm_ref\":{\"model_name\":\"sequence-llm\",\"type\":\"LLModel\"}}")
                    );
                }
                other => panic!("unexpected tool result item: {other:?}"),
            },
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn shared_inputs_and_tool_parameters_cannot_conflict() {
        ensure_registry_initialized();
        let mut node = BrainNode::new("brain_1", "Brain");
        let error = node
            .apply_inline_config(&HashMap::from([
                (
                    BRAIN_SHARED_INPUTS_PORT.to_string(),
                    DataValue::Json(json!([{ "name": "query", "data_type": "Json" }])),
                ),
                (
                    BRAIN_TOOLS_CONFIG_PORT.to_string(),
                    DataValue::Json(json!([passthrough_tool_definition("search")])),
                ),
            ]))
            .unwrap_err()
            .to_string();

        assert!(error.contains("conflicts with Brain shared input"));
    }

    #[test]
    fn tool_parameters_cannot_use_reserved_content_name() {
        ensure_registry_initialized();
        let mut node = BrainNode::new("brain_1", "Brain");
        let error = node
            .apply_inline_config(&HashMap::from([(
                BRAIN_TOOLS_CONFIG_PORT.to_string(),
                DataValue::Json(json!([{
                    "id": "tool_1",
                    "name": "search",
                    "parameters": [
                        { "name": "content", "data_type": "String" }
                    ]
                }])),
            )]))
            .unwrap_err()
            .to_string();

        assert!(error.contains("reserved for Brain tool-call content"));
    }

    #[test]
    fn execute_injects_tool_call_content_into_tool_subgraph() {
        ensure_registry_initialized();
        let mut subgraph = zihuan_node::function_graph::default_function_subgraph();
        let input_signature = vec![
            FunctionPortDef {
                name: "content".to_string(),
                data_type: DataType::String,
            },
            FunctionPortDef {
                name: "query".to_string(),
                data_type: DataType::String,
            },
        ];
        let output_signature = vec![FunctionPortDef {
            name: "content".to_string(),
            data_type: DataType::String,
        }];
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &output_signature);
        subgraph.edges.push(EdgeDefinition {
            from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
            from_port: "content".to_string(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: "content".to_string(),
        });
        let tool = BrainToolDefinition {
            id: "search_id".to_string(),
            name: "search".to_string(),
            description: "tool search".to_string(),
            parameters: vec![ToolParamDef {
                name: "query".to_string(),
                data_type: DataType::String,
                desc: "query".to_string(),
            }],
            outputs: output_signature,
            subgraph,
        };
        let llm = Arc::new(SequenceLlm::new(vec![
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("call with context".to_string()),
                tool_calls: vec![ToolCalls {
                    id: "tool_call_1".to_string(),
                    type_name: "function".to_string(),
                    function: ToolCallsFuncSpec {
                        name: "search".to_string(),
                        arguments: json!({"query": "rust"}),
                    },
                }],
                tool_call_id: None,
            },
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("done".to_string()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        ]));

        let mut node = BrainNode::new("brain_1", "Brain");
        node.apply_inline_config(&HashMap::from([(
            BRAIN_TOOLS_CONFIG_PORT.to_string(),
            DataValue::Json(json!([tool])),
        )]))
        .unwrap();

        let outputs = node
            .execute(HashMap::from([
                ("llm_model".to_string(), DataValue::LLModel(llm)),
                ("messages".to_string(), messages_input()),
            ]))
            .unwrap();

        match outputs.get("output") {
            Some(DataValue::Vec(_, items)) => match &items[1] {
                DataValue::OpenAIMessage(message) => {
                    assert_eq!(message.role, MessageRole::Tool);
                    assert_eq!(message.content.as_deref(), Some("{\"content\":\"call with context\"}"));
                }
                other => panic!("unexpected tool result item: {other:?}"),
            },
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn execute_injects_empty_string_when_tool_call_content_is_null() {
        ensure_registry_initialized();
        let mut subgraph = zihuan_node::function_graph::default_function_subgraph();
        let input_signature = vec![FunctionPortDef {
            name: "content".to_string(),
            data_type: DataType::String,
        }];
        let output_signature = vec![FunctionPortDef {
            name: "content".to_string(),
            data_type: DataType::String,
        }];
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &output_signature);
        subgraph.edges.push(EdgeDefinition {
            from_node_id: FUNCTION_INPUTS_NODE_ID.to_string(),
            from_port: "content".to_string(),
            to_node_id: FUNCTION_OUTPUTS_NODE_ID.to_string(),
            to_port: "content".to_string(),
        });
        let tool = BrainToolDefinition {
            id: "search_id".to_string(),
            name: "search".to_string(),
            description: "tool search".to_string(),
            parameters: Vec::new(),
            outputs: output_signature,
            subgraph,
        };
        let llm = Arc::new(SequenceLlm::new(vec![
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: None,
                tool_calls: vec![ToolCalls {
                    id: "tool_call_1".to_string(),
                    type_name: "function".to_string(),
                    function: ToolCallsFuncSpec {
                        name: "search".to_string(),
                        arguments: json!({}),
                    },
                }],
                tool_call_id: None,
            },
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("done".to_string()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        ]));

        let mut node = BrainNode::new("brain_1", "Brain");
        node.apply_inline_config(&HashMap::from([(
            BRAIN_TOOLS_CONFIG_PORT.to_string(),
            DataValue::Json(json!([tool])),
        )]))
        .unwrap();

        let outputs = node
            .execute(HashMap::from([
                ("llm_model".to_string(), DataValue::LLModel(llm)),
                ("messages".to_string(), messages_input()),
            ]))
            .unwrap();

        match outputs.get("output") {
            Some(DataValue::Vec(_, items)) => match &items[1] {
                DataValue::OpenAIMessage(message) => {
                    assert_eq!(message.role, MessageRole::Tool);
                    assert_eq!(message.content.as_deref(), Some("{\"content\":\"\"}"));
                }
                other => panic!("unexpected tool result item: {other:?}"),
            },
            other => panic!("unexpected output: {other:?}"),
        }
    }
}
