use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use log::{info, warn};
use serde_json::{json, Map, Value};

use zihuan_core::error::{Error, Result};
use crate::brain_tool::{
    brain_shared_inputs_from_value, brain_tool_input_signature, BrainToolDefinition, ToolParamDef,
    BRAIN_SHARED_INPUTS_PORT, BRAIN_TOOLS_CONFIG_PORT, BRAIN_TOOL_FIXED_CONTENT_INPUT,
};
use zihuan_llm_types::tooling::FunctionTool;
use zihuan_llm_types::{InferenceParam, OpenAIMessage};
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

const MAX_TOOL_ITERATIONS: usize = 25;

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

    fn sanitize_messages_for_inference(messages: Vec<OpenAIMessage>) -> Vec<OpenAIMessage> {
        let mut sanitized = Vec::with_capacity(messages.len());
        let mut pending_tool_calls: Option<(usize, HashSet<String>)> = None;

        for message in messages {
            if !message.tool_calls.is_empty() {
                if let Some((start_index, unresolved_ids)) = pending_tool_calls.take() {
                    warn!(
                        "[BrainNode] Dropping incomplete assistant tool-call segment before a new assistant tool-call message: unresolved_ids={:?}",
                        unresolved_ids
                    );
                    sanitized.truncate(start_index);
                }

                let tool_call_ids = message
                    .tool_calls
                    .iter()
                    .map(|tool_call| tool_call.id.clone())
                    .collect::<HashSet<_>>();
                let start_index = sanitized.len();
                sanitized.push(message);

                if !tool_call_ids.is_empty() {
                    pending_tool_calls = Some((start_index, tool_call_ids));
                }
                continue;
            }

            if matches!(message.role, zihuan_llm_types::MessageRole::Tool) {
                let tool_call_id = message.tool_call_id.clone();
                let mut should_keep_tool_message = false;

                if let Some((_, unresolved_ids)) = pending_tool_calls.as_mut() {
                    if let Some(tool_call_id) = tool_call_id.as_ref() {
                        if unresolved_ids.remove(tool_call_id) {
                            should_keep_tool_message = true;
                        }
                    }
                }

                if should_keep_tool_message {
                    sanitized.push(message);
                    if pending_tool_calls
                        .as_ref()
                        .is_some_and(|(_, unresolved_ids)| unresolved_ids.is_empty())
                    {
                        pending_tool_calls = None;
                    }
                } else {
                    match pending_tool_calls.as_ref() {
                        Some(_) => {
                            warn!(
                                "[BrainNode] Dropping orphan tool message without matching pending tool_call_id: {:?}",
                                tool_call_id
                            );
                        }
                        None => {
                            warn!(
                                "[BrainNode] Dropping tool message because no assistant tool-call message is pending: {:?}",
                                tool_call_id
                            );
                        }
                    }
                }
                continue;
            }

            if let Some((start_index, unresolved_ids)) = pending_tool_calls.take() {
                warn!(
                    "[BrainNode] Dropping incomplete assistant tool-call segment before non-tool message: unresolved_ids={:?}",
                    unresolved_ids
                );
                sanitized.truncate(start_index);
            }

            sanitized.push(message);
        }

        if let Some((start_index, unresolved_ids)) = pending_tool_calls {
            warn!(
                "[BrainNode] Dropping dangling assistant tool-call segment at end of history: unresolved_ids={:?}",
                unresolved_ids
            );
            sanitized.truncate(start_index);
        }

        sanitized
    }

    fn wrap_error(&self, message: impl Into<String>) -> Error {
        Error::ValidationError(format!("[NODE_ERROR:{}] {}", self.id, message.into()))
    }

    fn execute_tool_subgraph(
        &self,
        tool: &BrainToolDefinition,
        shared_runtime_values: HashMap<String, DataValue>,
        tool_call_content: String,
        arguments: Value,
    ) -> Result<String> {
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

        let mut runtime_values = shared_runtime_values;
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

    fn build_tool_error_message(&self, message: impl Into<String>) -> String {
        Value::Object(Map::from_iter([(
            "error".to_string(),
            Value::String(message.into()),
        )]))
        .to_string()
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
            Port::new(BRAIN_TOOLS_CONFIG_PORT, DataType::Json)
                .with_description("Tools 配置，由工具编辑器维护")
                .optional(),
            Port::new(BRAIN_SHARED_INPUTS_PORT, DataType::Json)
                .with_description("Brain 共享输入签名，由工具编辑器维护")
                .optional(),
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
            _ => {
                return Err(self.wrap_error("缺少必填输入 llm_model"));
            }
        };

        let mut conversation =
            Self::sanitize_messages_for_inference(Self::parse_messages_input(&inputs)?);
        let mut output_messages = Vec::new();
        let shared_runtime_values = self.parse_shared_inputs_input(&inputs)?;
        let tool_specs = self.tool_specs();

        for iteration in 0..MAX_TOOL_ITERATIONS {
            let response = model.inference(&InferenceParam {
                messages: &conversation,
                tools: Some(&tool_specs),
            });

            if let Some(content) = response.content.as_deref() {
                let is_transport_error = content.starts_with("Error: API request failed")
                    || content.starts_with("Error: Failed to send request")
                    || content.starts_with("Error: Failed to parse response")
                    || content.starts_with("Error: Invalid response structure");
                if is_transport_error {
                    return Err(self.wrap_error(format!("LLM request failed: {content}")));
                }
            }

            if response.tool_calls.is_empty() {
                output_messages.push(response);
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
                return Ok(outputs);
            }

            info!(
                "[BrainNode] iteration {} processing {} tool call(s)",
                iteration + 1,
                response.tool_calls.len()
            );

            conversation.push(response.clone());
            output_messages.push(response.clone());
            let tool_call_content = response.content.clone().unwrap_or_default();

            for tool_call in response.tool_calls {
                let tool_result_content = if let Some(tool) = self
                    .tool_definitions
                    .iter()
                    .find(|tool| tool.name == tool_call.function.name)
                {
                    match self.execute_tool_subgraph(
                        tool,
                        shared_runtime_values.clone(),
                        tool_call_content.clone(),
                        tool_call.function.arguments.clone(),
                    ) {
                        Ok(result) => result,
                        Err(error) => self.build_tool_error_message(error.to_string()),
                    }
                } else {
                    self.build_tool_error_message(format!(
                        "Tool '{}' not found",
                        tool_call.function.name
                    ))
                };

                info!(
                    "[BrainNode] tool result {}({}) => {}",
                    tool_call.function.name,
                    tool_call.id,
                    tool_result_content
                );

                let tool_result_message = OpenAIMessage::tool_result(
                    tool_call.id.clone(),
                    tool_result_content,
                );
                conversation.push(tool_result_message.clone());
                output_messages.push(tool_result_message);
            }
        }

        Err(self.wrap_error(format!(
            "Brain tool loop exceeded max iterations ({MAX_TOOL_ITERATIONS})"
        )))
    }
}

