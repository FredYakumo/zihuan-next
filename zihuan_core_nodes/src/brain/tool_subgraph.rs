use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use log::{info, warn};
use serde_json::{json, Map, Value};

use ims_bot_adapter::adapter::shared_from_handle;
use ims_bot_adapter::message_helpers::{
    send_friend_progress_notification_with_persistence,
    send_group_progress_notification_with_persistence, OutboundMessagePersistence,
};
use ims_bot_adapter::models::MessageType;

use crate::brain::brain_tool::{
    brain_tool_input_signature, fixed_tool_runtime_inputs, BrainToolDefinition, ToolParamDef,
    BRAIN_TOOL_FIXED_CONTENT_INPUT, QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT,
    QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT, QQ_AGENT_TOOL_OWNER_TYPE,
    QQ_AGENT_TOOL_OWNER_TYPE_LEGACY,
};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_graph_engine::function_graph::{
    sync_function_subgraph_signature, FunctionPortDef, FUNCTION_INPUTS_NODE_ID,
    FUNCTION_OUTPUTS_NODE_ID,
};
use zihuan_graph_engine::graph_io::refresh_port_types;
use zihuan_graph_engine::registry::{build_node_graph_from_definition, NODE_REGISTRY};
use zihuan_graph_engine::util::function::{
    data_value_from_json_with_declared_type, inject_runtime_values_into_function_inputs_node,
};
use zihuan_graph_engine::{DataType, DataValue, Port};

pub const QQ_AGENT_TOOL_OUTPUT_NAME: &str = "result";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResultMode {
    JsonObject,
    SingleString,
}

#[derive(Debug, Clone)]
pub struct ToolSubgraphRunner {
    pub node_id: String,
    pub owner_node_type: String,
    pub shared_inputs: Vec<FunctionPortDef>,
    pub definition: BrainToolDefinition,
    pub shared_runtime_values: HashMap<String, DataValue>,
    pub result_mode: ToolResultMode,
}

#[derive(Debug, Clone)]
pub struct SubgraphFunctionTool {
    definition: BrainToolDefinition,
}

impl SubgraphFunctionTool {
    pub fn new(definition: BrainToolDefinition) -> Self {
        Self { definition }
    }
}

impl FunctionTool for SubgraphFunctionTool {
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

pub fn data_type_to_json_schema_type(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::String | DataType::Password => "string",
        DataType::Integer => "integer",
        DataType::Float => "number",
        DataType::Boolean => "boolean",
        DataType::Binary => "string",
        DataType::Vec(_) | DataType::Vector => "array",
        DataType::Json
        | DataType::MessageEvent
        | DataType::OpenAIMessage
        | DataType::QQMessage
        | DataType::Image
        | DataType::FunctionTools
        | DataType::BotAdapterRef
        | DataType::S3Ref
        | DataType::RedisRef
        | DataType::MySqlRef
        | DataType::WeaviateRef
        | DataType::TavilyRef
        | DataType::SessionStateRef
        | DataType::OpenAIMessageSessionCacheRef
        | DataType::LLModel
        | DataType::EmbeddingModel
        | DataType::LoopControlRef
        | DataType::ContentPart
        | DataType::Custom(_)
        | DataType::Any => "object",
    }
}

pub fn tool_parameters_to_json_schema(parameters: &[ToolParamDef]) -> Value {
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

pub fn shared_inputs_ports(shared_inputs: &[FunctionPortDef], owner_label: &str) -> Vec<Port> {
    shared_inputs
        .iter()
        .map(|port| port.to_port(format!("{owner_label} 共享输入 '{}'", port.name)))
        .collect()
}

pub fn validate_shared_inputs(
    shared_inputs: &[FunctionPortDef],
    owner_label: &str,
) -> Result<Vec<FunctionPortDef>> {
    let mut seen_names = HashSet::new();
    let mut normalized = Vec::with_capacity(shared_inputs.len());

    for port in shared_inputs.iter().cloned() {
        let name = port.name.trim();
        if name.is_empty() {
            return Err(Error::ValidationError(format!(
                "{owner_label} shared input name cannot be empty"
            )));
        }
        if !seen_names.insert(name.to_string()) {
            return Err(Error::ValidationError(format!(
                "Duplicate {owner_label} shared input name: {name}"
            )));
        }
        normalized.push(FunctionPortDef {
            name: name.to_string(),
            data_type: port.data_type,
            description: port.description,
        });
    }

    Ok(normalized)
}

fn normalize_outputs_for_mode(
    tool: &mut BrainToolDefinition,
    result_mode: ToolResultMode,
) -> Result<()> {
    if result_mode == ToolResultMode::SingleString {
        if tool.outputs.len() != 1 {
            return Err(Error::ValidationError(format!(
                "Tool '{}' must declare exactly one String output",
                tool.name.trim()
            )));
        }
        let output = &tool.outputs[0];
        if output.data_type != DataType::String {
            return Err(Error::ValidationError(format!(
                "Tool '{}' output '{}' must use String type",
                tool.name.trim(),
                output.name
            )));
        }
    }

    Ok(())
}

pub fn validate_tool_definitions(
    tool_definitions: &[BrainToolDefinition],
    shared_inputs: &[FunctionPortDef],
    result_mode: ToolResultMode,
    owner_node_type: &str,
    owner_label: &str,
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
            return Err(Error::ValidationError(
                "Tool id cannot be empty".to_string(),
            ));
        }
        if tool_name.is_empty() {
            return Err(Error::ValidationError(
                "Tool name cannot be empty".to_string(),
            ));
        }
        if !seen_ids.insert(tool_id.to_string()) {
            return Err(Error::ValidationError(format!(
                "Duplicate tool id: {tool_id}"
            )));
        }
        if !seen_names.insert(tool_name.to_string()) {
            return Err(Error::ValidationError(format!(
                "Duplicate tool name: {tool_name}"
            )));
        }

        let mut seen_param_names = HashSet::new();
        let fixed_input_names = fixed_tool_runtime_inputs(owner_node_type)
            .into_iter()
            .map(|port| port.name)
            .collect::<HashSet<_>>();
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
                    "Tool '{}' parameter '{}' conflicts with {owner_label} shared input",
                    tool_name, param_name
                )));
            }
            if fixed_input_names.contains(param_name) {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' parameter '{}' is reserved for {owner_label} tool runtime input",
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

        normalize_outputs_for_mode(&mut tool, result_mode)?;
        let input_signature = brain_tool_input_signature(owner_node_type, shared_inputs, &tool);
        sync_function_subgraph_signature(&mut tool.subgraph, &input_signature, &tool.outputs);
        normalized.push(tool);
    }

    Ok(normalized)
}

pub fn build_tool_error_message(message: impl Into<String>) -> String {
    Value::Object(Map::from_iter([(
        "error".to_string(),
        Value::String(message.into()),
    )]))
    .to_string()
}

fn send_brain_tool_progress_notification(
    owner_node_type: &str,
    shared_runtime_values: &HashMap<String, DataValue>,
    call_content: &str,
) {
    if owner_node_type == QQ_AGENT_TOOL_OWNER_TYPE
        || owner_node_type == QQ_AGENT_TOOL_OWNER_TYPE_LEGACY
        || call_content.trim().is_empty()
    {
        return;
    }

    let event = match shared_runtime_values.get(QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT) {
        Some(DataValue::MessageEvent(event)) => event,
        _ => return,
    };
    let adapter = match shared_runtime_values.get(QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT) {
        Some(DataValue::BotAdapterRef(handle)) => shared_from_handle(handle),
        _ => return,
    };

    if event.message_type == MessageType::Group {
        if let Some(group_id) = event.group_id {
            send_group_progress_notification_with_persistence(
                &adapter,
                &group_id.to_string(),
                &event.sender.user_id.to_string(),
                call_content,
                &OutboundMessagePersistence {
                    group_name: event.group_name.clone(),
                    ..OutboundMessagePersistence::default()
                },
            );
        }
    } else {
        send_friend_progress_notification_with_persistence(
            &adapter,
            &event.sender.user_id.to_string(),
            call_content,
            &OutboundMessagePersistence::default(),
        );
    }
}

impl ToolSubgraphRunner {
    fn wrap_error(&self, msg: impl Into<String>) -> Error {
        Error::ValidationError(format!("[NODE_ERROR:{}] {}", self.node_id, msg.into()))
    }

    pub fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(SubgraphFunctionTool::new(self.definition.clone()))
    }

    pub fn execute_to_string(&self, call_content: &str, arguments: &Value) -> String {
        match self.run_subgraph(call_content.to_string(), arguments.clone()) {
            Ok(result) => result,
            Err(e) => {
                warn!(
                    "[ToolSubgraph:{}] tool '{}' failed; returning sanitized error to caller: {e}",
                    self.node_id, self.definition.name
                );
                format!("{} 执行出错", self.definition.name)
            }
        }
    }

    pub fn run_subgraph(&self, tool_call_content: String, arguments: Value) -> Result<String> {
        let tool = &self.definition;
        info!(
            "[ToolSubgraph:{}] executing tool '{}' with content='{}' arguments={}",
            self.node_id, tool.name, tool_call_content, arguments
        );
        send_brain_tool_progress_notification(
            &self.owner_node_type,
            &self.shared_runtime_values,
            &tool_call_content,
        );
        let _ = &NODE_REGISTRY;

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
        if self.owner_node_type == QQ_AGENT_TOOL_OWNER_TYPE {
            for fixed_name in [
                QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT,
                QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT,
            ] {
                if !runtime_values.contains_key(fixed_name) {
                    return Err(self.wrap_error(format!(
                        "Tool '{}' 缺少固定输入 '{}'",
                        tool.name, fixed_name
                    )));
                }
            }
        } else {
            runtime_values.insert(
                BRAIN_TOOL_FIXED_CONTENT_INPUT.to_string(),
                DataValue::String(tool_call_content),
            );
        }
        for (key, value) in tool_runtime_values {
            if runtime_values.contains_key(&key) {
                return Err(self.wrap_error(format!(
                    "Tool '{}' 参数 '{}' 与共享输入重名",
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
                            description: param.desc.clone(),
                        },
                        &value,
                    )
                })
                .transpose()?
                .unwrap_or(DataValue::Json(value));
            runtime_values.insert(key, parsed_value);
        }

        let input_signature =
            brain_tool_input_signature(&self.owner_node_type, &self.shared_inputs, tool);
        let mut subgraph = tool.subgraph.clone();
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &tool.outputs);
        refresh_port_types(&mut subgraph);

        let function_inputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_INPUTS_NODE_ID)
            .ok_or_else(|| {
                self.wrap_error(format!(
                    "Tool '{}' 缺少 function_inputs 边界节点",
                    tool.name
                ))
            })?;
        function_inputs_node.inline_values.insert(
            zihuan_graph_engine::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&input_signature).unwrap_or(Value::Null),
        );

        let function_outputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| {
                self.wrap_error(format!(
                    "Tool '{}' 缺少 function_outputs 边界节点",
                    tool.name
                ))
            })?;
        function_outputs_node.inline_values.insert(
            zihuan_graph_engine::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&tool.outputs).unwrap_or(Value::Null),
        );

        let mut graph = build_node_graph_from_definition(&subgraph)
            .map_err(|e| self.wrap_error(format!("Tool '{}' 子图构建失败: {e}", tool.name)))?;
        inject_runtime_values_into_function_inputs_node(&mut graph, runtime_values).map_err(
            |e| self.wrap_error(format!("Tool '{}' 注入子图运行时输入失败: {e}", tool.name)),
        )?;
        let execution_result = graph.execute_and_capture_results();
        if let Some(ref error_message) = execution_result.error_message {
            warn!(
                "[ToolSubgraph:{}] tool '{}' execution failed: {error_message}",
                self.node_id, tool.name
            );
            return Err(self.wrap_error(format!(
                "Tool '{}' 子图执行失败: {error_message}",
                tool.name
            )));
        }

        let result_node_values = execution_result
            .node_results
            .get(FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| {
                self.wrap_error(format!(
                    "Tool '{}' 缺少 function_outputs 执行结果",
                    tool.name
                ))
            })?;

        match self.result_mode {
            ToolResultMode::JsonObject => {
                let mut result_payload = Map::new();
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
                let result = Value::Object(result_payload).to_string();
                info!(
                    "[ToolSubgraph:{}] tool '{}' succeeded with result: {result}",
                    self.node_id, tool.name
                );
                Ok(result)
            }
            ToolResultMode::SingleString => {
                let output = tool.outputs.first().ok_or_else(|| {
                    self.wrap_error(format!("Tool '{}' 必须声明一个 String 输出", tool.name))
                })?;
                let value = result_node_values.get(&output.name).ok_or_else(|| {
                    self.wrap_error(format!(
                        "Tool '{}' 输出 '{}' 未在子图中提供",
                        tool.name, output.name
                    ))
                })?;
                match value {
                    DataValue::String(text) => {
                        info!(
                            "[ToolSubgraph:{}] tool '{}' succeeded with result: {text}",
                            self.node_id, tool.name
                        );
                        Ok(text.clone())
                    }
                    other => Err(self.wrap_error(format!(
                        "Tool '{}' 输出 '{}' 类型不匹配：声明为 String，实际为 {}",
                        tool.name,
                        output.name,
                        other.data_type()
                    ))),
                }
            }
        }
    }
}
