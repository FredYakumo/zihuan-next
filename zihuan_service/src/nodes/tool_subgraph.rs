use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ims_bot_adapter::adapter::shared_from_handle;
use ims_bot_adapter::models::MessageType;
use log::{info, warn};
use serde_json::{json, Map, Value};

use zihuan_agent::brain::{consume_tool_progress_notification, current_task_progress_message};
use zihuan_core::agent_config::qq_chat::{with_current_qq_chat_agent_service_config, QqChatAgentServiceConfig};
use zihuan_core::config::ConfigCenter;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::task_context::append_current_task_progress;
use zihuan_graph_engine::brain_tool_spec::{
    brain_tool_input_signature, fixed_tool_runtime_inputs, BrainToolDefinition, BrainToolImplementation,
    BuiltInBrainToolKind, PythonScriptToolConfig, ToolParamDef, BRAIN_TOOL_FIXED_CONTENT_INPUT,
    QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT, QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT, QQ_AGENT_TOOL_OWNER_TYPE,
};
use zihuan_graph_engine::function_graph::{
    sync_function_subgraph_signature, FunctionPortDef, FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
};
use zihuan_graph_engine::graph_io::refresh_port_types;
use zihuan_graph_engine::registry::{build_node_graph_from_definition, NODE_REGISTRY};
use zihuan_graph_engine::util::function::{
    data_value_from_json_with_declared_type, inject_runtime_values_into_function_inputs_node,
};
use zihuan_graph_engine::{DataType, DataValue, Port};

use crate::agent::execute_image_understand_tool;
use crate::agent::qq_chat::msg_send::{send_notification_text, QqChatServiceSendContext};
use crate::agent::QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS;
use crate::python_runtime::resolve_python_runtime;

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
    pub shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
    pub qq_chat_agent_config: Option<QqChatAgentServiceConfig>,
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
        | DataType::Sender
        | DataType::LLMMessage
        | DataType::QQMessage
        | DataType::Image
        | DataType::FunctionTools
        | DataType::BotAdapterRef
        | DataType::S3Ref
        | DataType::RedisRef
        | DataType::RdbRef
        | DataType::WeaviateRef
        | DataType::WebSearchEngineRef
        | DataType::SessionStateRef
        | DataType::LLMMessageSessionCacheRef
        | DataType::LLModel
        | DataType::EmbeddingModel
        | DataType::LoopControlRef
        | DataType::MessagePart
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
        if param.required {
            required.push(Value::String(param_name.to_string()));
        }
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

pub fn validate_shared_inputs(shared_inputs: &[FunctionPortDef], owner_label: &str) -> Result<Vec<FunctionPortDef>> {
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
            required: port.required,
        });
    }

    Ok(normalized)
}

fn normalize_outputs_for_mode(tool: &mut BrainToolDefinition, result_mode: ToolResultMode) -> Result<()> {
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

fn validate_tool_implementation(tool: &BrainToolDefinition) -> Result<()> {
    match tool.implementation {
        BrainToolImplementation::NodeGraph => Ok(()),
        BrainToolImplementation::BuiltIn => match tool.builtin_kind() {
            Some(BuiltInBrainToolKind::ImageUnderstand) => Ok(()),
            None => Err(Error::ValidationError(format!(
                "Tool '{}' 使用 built_in implementation 时必须声明 built_in_kind",
                tool.name.trim()
            ))),
        },
        BrainToolImplementation::PythonScript => {
            let python_config = tool.python_config().ok_or_else(|| {
                Error::ValidationError(format!(
                    "Tool '{}' 使用 python_script implementation 时必须声明 python_config",
                    tool.name.trim()
                ))
            })?;
            validate_python_tool_config(tool.name.trim(), python_config)
        }
    }
}

fn validate_python_tool_config(tool_name: &str, config: &PythonScriptToolConfig) -> Result<()> {
    if config.script_path.trim().is_empty() {
        return Err(Error::ValidationError(format!(
            "Tool '{}' 的 python script_path 不能为空",
            tool_name
        )));
    }
    if !config.script_path.trim().ends_with(".py") {
        return Err(Error::ValidationError(format!(
            "Tool '{}' 的 python script_path 必须指向 .py 文件",
            tool_name
        )));
    }
    if config.module_entry.trim().is_empty() {
        return Err(Error::ValidationError(format!(
            "Tool '{}' 的 python module_entry 不能为空",
            tool_name
        )));
    }
    if config.timeout_secs == 0 {
        return Err(Error::ValidationError(format!(
            "Tool '{}' 的 python timeout_secs 必须大于 0",
            tool_name
        )));
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
        validate_tool_implementation(&tool)?;
        if tool.uses_subgraph() {
            let input_signature = brain_tool_input_signature(owner_node_type, shared_inputs, &tool);
            sync_function_subgraph_signature(&mut tool.subgraph, &input_signature, &tool.outputs);
        }
        normalized.push(tool);
    }

    Ok(normalized)
}

pub fn build_tool_error_message(message: impl Into<String>) -> String {
    Value::Object(Map::from_iter([("error".to_string(), Value::String(message.into()))])).to_string()
}

/// Sends a progress notification for a brain tool call if notifications are enabled.
///
/// Looks up the runtime context to determine whether the caller requested progress updates,
/// consumes a throttle token, and then routes the notification as a group or friend message.
fn send_brain_tool_progress_notification(
    shared_runtime_values: &Arc<Mutex<HashMap<String, DataValue>>>,
    call_content: &str,
) {
    let shared_rt = shared_runtime_values.lock().unwrap();
    if let Some(progress_text) = current_task_progress_message(call_content) {
        if append_current_task_progress(progress_text) {
            return;
        }
    }

    if matches!(
        shared_rt.get(QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS),
        Some(DataValue::Boolean(false))
    ) {
        return;
    }

    if !consume_tool_progress_notification(call_content) {
        return;
    }

    let event = match shared_rt.get(QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT) {
        Some(DataValue::MessageEvent(event)) => event,
        _ => return,
    };
    let adapter = match shared_rt.get(QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT) {
        Some(DataValue::BotAdapterRef(handle)) => shared_from_handle(handle),
        _ => return,
    };

    if event.message_type == MessageType::Group {
        if let Some(group_id) = event.group_id {
            let group_id = group_id.to_string();
            let sender_id = event.sender.user_id.to_string();
            let send_ctx = QqChatServiceSendContext {
                adapter: &adapter,
                target_id: &group_id,
                is_group: true,
                group_name: event.group_name.as_deref(),
                bot_id: "",
                bot_name: "",
                mention_target_id: Some(&sender_id),
                persistence: Default::default(),
                max_text_chars: 250,
            };
            let _ = send_notification_text(&send_ctx, call_content);
        }
    } else {
        let target_id = event.sender.user_id.to_string();
        let send_ctx = QqChatServiceSendContext {
            adapter: &adapter,
            target_id: &target_id,
            is_group: false,
            group_name: None,
            bot_id: "",
            bot_name: "",
            mention_target_id: None,
            persistence: Default::default(),
            max_text_chars: 250,
        };
        let _ = send_notification_text(&send_ctx, call_content);
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
        if self.owner_node_type != QQ_AGENT_TOOL_OWNER_TYPE {
            send_brain_tool_progress_notification(&self.shared_runtime_values, &tool_call_content);
        }
        let _ = &NODE_REGISTRY;

        let tool_runtime_values = match arguments {
            Value::Object(map) => map,
            Value::Null => Map::new(),
            other => {
                return Err(self.wrap_error(format!("Tool '{}' 的参数必须是 JSON 对象，实际为 {}", tool.name, other)));
            }
        };
        let builtin_arguments = Value::Object(tool_runtime_values.clone());

        let mut runtime_values = self.shared_runtime_values.lock().unwrap().clone();
        runtime_values.insert(
            BRAIN_TOOL_FIXED_CONTENT_INPUT.to_string(),
            DataValue::String(tool_call_content.clone()),
        );
        if self.owner_node_type == QQ_AGENT_TOOL_OWNER_TYPE {
            for fixed_name in [
                BRAIN_TOOL_FIXED_CONTENT_INPUT,
                QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT,
                QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT,
            ] {
                if !runtime_values.contains_key(fixed_name) {
                    return Err(self.wrap_error(format!("Tool '{}' 缺少固定输入 '{}'", tool.name, fixed_name)));
                }
            }
        }
        for (key, value) in tool_runtime_values {
            if runtime_values.contains_key(&key) {
                return Err(self.wrap_error(format!("Tool '{}' 参数 '{}' 与共享输入重名", tool.name, key)));
            }
            let param_definition = tool.parameters.iter().find(|param| param.name == key);
            if matches!(param_definition, Some(param) if !param.required) && value.is_null() {
                continue;
            }

            let parsed_value = match param_definition {
                Some(param) => data_value_from_json_with_declared_type(
                    &FunctionPortDef {
                        name: param.name.clone(),
                        data_type: param.data_type.clone(),
                        description: param.desc.clone(),
                        required: param.required,
                    },
                    &value,
                )?,
                None => DataValue::Json(value),
            };
            runtime_values.insert(key, parsed_value);
        }

        let input_signature = brain_tool_input_signature(&self.owner_node_type, &self.shared_inputs, tool);
        if !tool.uses_subgraph() {
            return self.run_non_subgraph_tool(tool, &tool_call_content, &builtin_arguments, &runtime_values);
        }

        let mut subgraph = tool.subgraph.clone();
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &tool.outputs);
        refresh_port_types(&mut subgraph);

        let function_inputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_INPUTS_NODE_ID)
            .ok_or_else(|| self.wrap_error(format!("Tool '{}' 缺少 function_inputs 边界节点", tool.name)))?;
        function_inputs_node.inline_values.insert(
            zihuan_graph_engine::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&input_signature).unwrap_or(Value::Null),
        );

        let function_outputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| self.wrap_error(format!("Tool '{}' 缺少 function_outputs 边界节点", tool.name)))?;
        function_outputs_node.inline_values.insert(
            zihuan_graph_engine::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&tool.outputs).unwrap_or(Value::Null),
        );

        let mut graph = build_node_graph_from_definition(&subgraph)
            .map_err(|e| self.wrap_error(format!("Tool '{}' 子图构建失败: {e}", tool.name)))?;
        inject_runtime_values_into_function_inputs_node(&mut graph, runtime_values.into())
            .map_err(|e| self.wrap_error(format!("Tool '{}' 注入子图运行时输入失败: {e}", tool.name)))?;
        let execution_result = if let Some(config) = self.qq_chat_agent_config.clone() {
            with_current_qq_chat_agent_service_config(config, || graph.execute_and_capture_results())
        } else {
            graph.execute_and_capture_results()
        };
        if let Some(ref error_message) = execution_result.error_message {
            warn!(
                "[ToolSubgraph:{}] tool '{}' execution failed: {error_message}",
                self.node_id, tool.name
            );
            return Err(self.wrap_error(format!("Tool '{}' 子图执行失败: {error_message}", tool.name)));
        }

        let result_node_values = execution_result
            .node_results
            .get(FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| self.wrap_error(format!("Tool '{}' 缺少 function_outputs 执行结果", tool.name)))?;

        match self.result_mode {
            ToolResultMode::JsonObject => {
                let mut result_payload = Map::new();
                for port in &tool.outputs {
                    let value = result_node_values.get(&port.name).ok_or_else(|| {
                        self.wrap_error(format!("Tool '{}' 输出 '{}' 未在子图中提供", tool.name, port.name))
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
                let output = tool
                    .outputs
                    .first()
                    .ok_or_else(|| self.wrap_error(format!("Tool '{}' 必须声明一个 String 输出", tool.name)))?;
                let value = result_node_values.get(&output.name).ok_or_else(|| {
                    self.wrap_error(format!("Tool '{}' 输出 '{}' 未在子图中提供", tool.name, output.name))
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

    fn run_non_subgraph_tool(
        &self,
        tool: &BrainToolDefinition,
        tool_call_content: &str,
        builtin_arguments: &Value,
        runtime_values: &HashMap<String, DataValue>,
    ) -> Result<String> {
        match tool.implementation {
            BrainToolImplementation::BuiltIn => {
                let result = match tool.builtin_kind() {
                    Some(BuiltInBrainToolKind::ImageUnderstand) => {
                        execute_image_understand_tool(builtin_arguments, runtime_values)
                    }
                    None => Err(self.wrap_error(format!("Tool '{}' missing built_in_kind", tool.name))),
                }?;
                self.format_scalar_result(tool, result)
            }
            BrainToolImplementation::PythonScript => {
                let result = self.run_python_script_tool(tool, tool_call_content, builtin_arguments, runtime_values)?;
                self.format_python_result(tool, result)
            }
            BrainToolImplementation::NodeGraph => {
                Err(self.wrap_error(format!("Tool '{}' 非预期地进入了非子图分支", tool.name)))
            }
        }
    }

    fn format_scalar_result(&self, tool: &BrainToolDefinition, result: String) -> Result<String> {
        match self.result_mode {
            ToolResultMode::JsonObject => {
                let output = tool
                    .outputs
                    .first()
                    .ok_or_else(|| self.wrap_error(format!("Tool '{}' 必须声明一个 String 输出", tool.name)))?;
                let result_payload =
                    Value::Object(Map::from_iter([(output.name.clone(), Value::String(result))])).to_string();
                info!(
                    "[ToolSubgraph:{}] tool '{}' succeeded with result: {}",
                    self.node_id, tool.name, result_payload
                );
                Ok(result_payload)
            }
            ToolResultMode::SingleString => {
                info!(
                    "[ToolSubgraph:{}] tool '{}' succeeded with result: {}",
                    self.node_id, tool.name, result
                );
                Ok(result)
            }
        }
    }

    fn format_python_result(&self, tool: &BrainToolDefinition, result: Value) -> Result<String> {
        match self.result_mode {
            ToolResultMode::JsonObject => {
                let object = result
                    .as_object()
                    .ok_or_else(|| self.wrap_error(format!("Tool '{}' 的 python 返回 result 必须是对象", tool.name)))?;
                let mut result_payload = Map::new();
                for port in &tool.outputs {
                    let value = object.get(&port.name).ok_or_else(|| {
                        self.wrap_error(format!("Tool '{}' 输出 '{}' 未在 python result 中提供", tool.name, port.name))
                    })?;
                    let parsed = data_value_from_json_with_declared_type(port, value)?;
                    result_payload.insert(port.name.clone(), parsed.to_json());
                }
                let encoded = Value::Object(result_payload).to_string();
                info!(
                    "[ToolSubgraph:{}] tool '{}' succeeded with result: {}",
                    self.node_id, tool.name, encoded
                );
                Ok(encoded)
            }
            ToolResultMode::SingleString => {
                let text = result.as_str().ok_or_else(|| {
                    self.wrap_error(format!("Tool '{}' 的 python 返回 result 必须是字符串", tool.name))
                })?;
                info!(
                    "[ToolSubgraph:{}] tool '{}' succeeded with result: {}",
                    self.node_id, tool.name, text
                );
                Ok(text.to_string())
            }
        }
    }

    fn run_python_script_tool(
        &self,
        tool: &BrainToolDefinition,
        tool_call_content: &str,
        builtin_arguments: &Value,
        runtime_values: &HashMap<String, DataValue>,
    ) -> Result<Value> {
        let python_config = tool
            .python_config()
            .ok_or_else(|| self.wrap_error(format!("Tool '{}' 缺少 python_config", tool.name)))?;
        let request = self.build_python_request(tool_call_content, builtin_arguments, runtime_values);
        let raw = self.execute_python_process(tool, python_config, &request)?;
        let response: Value = serde_json::from_str(&raw)
            .map_err(|e| self.wrap_error(format!("Tool '{}' 的 python 输出不是合法 JSON: {e}", tool.name)))?;
        let ok = response.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if !ok {
            let error = response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("python tool returned unknown error");
            return Err(self.wrap_error(format!("Tool '{}' python 执行失败: {}", tool.name, error)));
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| self.wrap_error(format!("Tool '{}' 的 python 输出缺少 result 字段", tool.name)))
    }

    fn build_python_request(
        &self,
        tool_call_content: &str,
        builtin_arguments: &Value,
        runtime_values: &HashMap<String, DataValue>,
    ) -> Value {
        let shared_input_names = self.shared_inputs.iter().map(|port| port.name.as_str()).collect::<HashSet<_>>();
        let fixed_input_names = fixed_tool_runtime_inputs(&self.owner_node_type)
            .into_iter()
            .map(|port| port.name)
            .collect::<HashSet<_>>();

        let mut shared_inputs = Map::new();
        let mut fixed_runtime_inputs = Map::new();
        for (key, value) in runtime_values {
            if shared_input_names.contains(key.as_str()) {
                shared_inputs.insert(key.clone(), value.to_json());
            } else if fixed_input_names.contains(key) {
                fixed_runtime_inputs.insert(key.clone(), value.to_json());
            }
        }

        json!({
            "call_content": tool_call_content,
            "arguments": builtin_arguments,
            "parameters": self.definition.parameters,
            "outputs": self.definition.outputs,
            "shared_inputs": shared_inputs,
            "fixed_runtime_inputs": fixed_runtime_inputs,
            "owner_node_type": self.owner_node_type,
        })
    }

    fn execute_python_process(
        &self,
        tool: &BrainToolDefinition,
        config: &PythonScriptToolConfig,
        request: &Value,
    ) -> Result<String> {
        let workspace_root = std::env::current_dir()
            .map_err(|e| self.wrap_error(format!("Tool '{}' 无法获取当前工作目录: {e}", tool.name)))?;
        let script_path = resolve_path(&workspace_root, &config.script_path);
        if !script_path.exists() {
            return Err(self.wrap_error(format!(
                "Tool '{}' 的 python 脚本不存在: {}",
                tool.name,
                script_path.display()
            )));
        }

        let bootstrap = workspace_root.join("utils").join("python_tool_runtime.py");
        if !bootstrap.exists() {
            return Err(self.wrap_error(format!(
                "Tool '{}' 缺少 python bootstrap: {}",
                tool.name,
                bootstrap.display()
            )));
        }

        let runtime_config = match config.runtime_override() {
            Some(runtime) => runtime,
            None => {
                ConfigCenter::shared()
                    .load_root()
                    .map_err(|error| {
                        self.wrap_error(format!("Tool '{}' 无法加载 Python 运行时配置: {error}", tool.name))
                    })?
                    .python_runtime
            }
        };
        let command_spec = resolve_python_runtime(&workspace_root, &runtime_config)
            .map_err(|error| self.wrap_error(format!("Tool '{}' 无法解析 Python 运行时: {error}", tool.name)))?;
        let mut command = command_spec.to_command();

        command
            .arg(&bootstrap)
            .arg("--script")
            .arg(&script_path)
            .arg("--entry")
            .arg(&config.module_entry)
            .current_dir(&workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|e| self.wrap_error(format!("Tool '{}' 启动 python 进程失败: {e}", tool.name)))?;

        if let Some(mut stdin) = child.stdin.take() {
            let payload = serde_json::to_vec(request)
                .map_err(|e| self.wrap_error(format!("Tool '{}' 序列化 python 请求失败: {e}", tool.name)))?;
            stdin
                .write_all(&payload)
                .map_err(|e| self.wrap_error(format!("Tool '{}' 写入 python stdin 失败: {e}", tool.name)))?;
        }

        let deadline = Instant::now() + Duration::from_secs(config.timeout_secs);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(self.wrap_error(format!(
                            "Tool '{}' 的 python 执行超时（{} 秒）",
                            tool.name, config.timeout_secs
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(self.wrap_error(format!("Tool '{}' 等待 python 进程失败: {e}", tool.name)));
                }
            }
        }

        let output = child
            .wait_with_output()
            .map_err(|e| self.wrap_error(format!("Tool '{}' 读取 python 输出失败: {e}", tool.name)))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(self.wrap_error(format!(
                "Tool '{}' 的 python 进程退出失败: {}",
                tool.name,
                if stderr.is_empty() {
                    "unknown error".to_string()
                } else {
                    stderr
                }
            )));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| self.wrap_error(format!("Tool '{}' 的 python stdout 不是有效 UTF-8: {e}", tool.name)))?;
        Ok(stdout)
    }
}

fn resolve_path(root: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}
