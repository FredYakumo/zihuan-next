use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use log::{info, warn};
use serde_json::{json, Map, Value};

use crate::error::{Error, Result};
use crate::llm::brain_tool::{BrainToolDefinition, ToolParamDef};
use crate::llm::tooling::FunctionTool;
use crate::llm::{InferenceParam, OpenAIMessage};
use crate::node::function_graph::{
    sync_function_subgraph_signature, FunctionPortDef, FUNCTION_INPUTS_NODE_ID,
    FUNCTION_OUTPUTS_NODE_ID,
};
use crate::node::graph_io::refresh_port_types;
use crate::node::registry::{build_node_graph_from_definition, NODE_REGISTRY};
use crate::node::{node_input, DataType, DataValue, Node, Port};

const TOOLS_CONFIG_PORT: &str = "tools_config";
const MAX_TOOL_ITERATIONS: usize = 5;

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

fn tool_input_signature(tool: &BrainToolDefinition) -> Vec<FunctionPortDef> {
    tool.parameters
        .iter()
        .map(|param| FunctionPortDef {
            name: param.name.clone(),
            data_type: param.data_type.clone(),
        })
        .collect()
}

fn validate_tool_definitions(tool_definitions: &[BrainToolDefinition]) -> Result<Vec<BrainToolDefinition>> {
    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();
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
            if !seen_param_names.insert(param_name.to_string()) {
                return Err(Error::ValidationError(format!(
                    "Tool '{}' has duplicate parameter '{}'",
                    tool_name, param_name
                )));
            }
        }

        let input_signature = tool_input_signature(&tool);
        sync_function_subgraph_signature(&mut tool.subgraph, &input_signature, &tool.outputs);
        normalized.push(tool);
    }

    Ok(normalized)
}

#[derive(Debug, Clone)]
pub struct BrainNode {
    id: String,
    name: String,
    tool_definitions: Vec<BrainToolDefinition>,
}

impl BrainNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            tool_definitions: Vec::new(),
        }
    }

    fn set_tool_definitions(&mut self, tool_definitions: Vec<BrainToolDefinition>) -> Result<()> {
        self.tool_definitions = validate_tool_definitions(&tool_definitions)?;
        Ok(())
    }

    fn output_ports_static() -> Vec<Port> {
        vec![Port::new("assistant_message", DataType::OpenAIMessage)
            .with_description("完成工具调用循环后的 assistant 最终消息")]
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

            if matches!(message.role, crate::llm::MessageRole::Tool) {
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

        let runtime_values = match arguments {
            Value::Object(map) => map,
            Value::Null => Map::new(),
            other => {
                return Err(self.wrap_error(format!(
                    "Tool '{}' 的参数必须是 JSON 对象，实际为 {}",
                    tool.name, other
                )));
            }
        };

        let input_signature = tool_input_signature(tool);
        let mut subgraph = tool.subgraph.clone();
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &tool.outputs);
        refresh_port_types(&mut subgraph);

        let function_inputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_INPUTS_NODE_ID)
            .ok_or_else(|| self.wrap_error(format!("Tool '{}' 缺少 function_inputs 边界节点", tool.name)))?;
        function_inputs_node.inline_values.insert(
            crate::node::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&input_signature).unwrap_or(Value::Null),
        );
        function_inputs_node.inline_values.insert(
            crate::node::function_graph::FUNCTION_RUNTIME_VALUES_PORT.to_string(),
            Value::Object(runtime_values),
        );

        let function_outputs_node = subgraph
            .nodes
            .iter_mut()
            .find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID)
            .ok_or_else(|| self.wrap_error(format!("Tool '{}' 缺少 function_outputs 边界节点", tool.name)))?;
        function_outputs_node.inline_values.insert(
            crate::node::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&tool.outputs).unwrap_or(Value::Null),
        );

        let mut graph = build_node_graph_from_definition(&subgraph)
            .map_err(|e| self.wrap_error(format!("Tool '{}' 子图构建失败: {e}", tool.name)))?;
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

    fn tool_specs(&self) -> Vec<Arc<dyn FunctionTool>> {
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

    node_input![
        port! { name = "llm_model", ty = LLModel, desc = "LLM 模型引用，由 LLM API 节点提供" },
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "消息列表（包含 system/user/assistant/tool 等角色）" },
        port! { name = "tools_config", ty = Json, desc = "Tools 配置，由工具编辑器维护", optional },
    ];

    fn output_ports(&self) -> Vec<Port> {
        Self::output_ports_static()
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        match inline_values.get(TOOLS_CONFIG_PORT) {
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

        if let Some(DataValue::Json(value)) = inputs.get(TOOLS_CONFIG_PORT) {
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

        let mut conversation = Self::sanitize_messages_for_inference(Self::parse_messages_input(&inputs)?);
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
                let mut outputs = HashMap::new();
                outputs.insert(
                    "assistant_message".to_string(),
                    DataValue::OpenAIMessage(response.clone()),
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

            for tool_call in response.tool_calls {
                let tool_result_content = if let Some(tool) = self
                    .tool_definitions
                    .iter()
                    .find(|tool| tool.name == tool_call.function.name)
                {
                    match self.execute_tool_subgraph(tool, tool_call.function.arguments.clone()) {
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

                conversation.push(OpenAIMessage::tool_result(
                    tool_call.id.clone(),
                    tool_result_content,
                ));
            }
        }

        Err(self.wrap_error(format!(
            "Brain tool loop exceeded max iterations ({MAX_TOOL_ITERATIONS})"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, Once};

    use serde_json::json;

    use super::BrainNode;
    use crate::llm::brain_tool::{BrainToolDefinition, ToolParamDef};
    use crate::llm::llm_base::LLMBase;
    use crate::llm::tooling::{ToolCalls, ToolCallsFuncSpec};
    use crate::llm::{InferenceParam, MessageRole, OpenAIMessage};
    use crate::node::function_graph::{
        default_function_subgraph, sync_function_subgraph_signature, FunctionPortDef,
        FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
    };
    use crate::node::graph_io::EdgeDefinition;
    use crate::node::{DataType, DataValue, Node};

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
            crate::node::registry::init_node_registry().expect("registry should initialize");
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

    fn messages_input() -> DataValue {
        DataValue::Vec(
            Box::new(DataType::OpenAIMessage),
            vec![DataValue::OpenAIMessage(OpenAIMessage::user("hello"))],
        )
    }

    #[test]
    fn brain_output_is_static_assistant_message_only() {
        ensure_registry_initialized();
        let node = BrainNode::new("brain_1", "Brain");
        let output_names: Vec<String> = node.output_ports().into_iter().map(|port| port.name).collect();
        assert_eq!(output_names, vec!["assistant_message"]);
    }

    #[test]
    fn execute_runs_internal_tool_loop_and_returns_final_assistant_message() {
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
            "tools_config".to_string(),
            DataValue::Json(json!([passthrough_tool_definition("search")])),
        )]))
        .unwrap();

        let outputs = node
            .execute(HashMap::from([
                ("llm_model".to_string(), DataValue::LLModel(llm.clone())),
                ("messages".to_string(), messages_input()),
            ]))
            .unwrap();

        match outputs.get("assistant_message") {
            Some(DataValue::OpenAIMessage(message)) => {
                assert_eq!(message.content.as_deref(), Some("done"));
                assert!(message.tool_calls.is_empty());
            }
            other => panic!("unexpected assistant_message output: {other:?}"),
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
                ("tools_config".to_string(), DataValue::Json(json!([]))),
            ]))
            .unwrap();

        match outputs.get("assistant_message") {
            Some(DataValue::OpenAIMessage(message)) => {
                assert_eq!(message.content.as_deref(), Some("recovered"));
            }
            other => panic!("unexpected assistant_message output: {other:?}"),
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

        match outputs.get("assistant_message") {
            Some(DataValue::OpenAIMessage(message)) => {
                assert_eq!(message.content.as_deref(), Some("plain reply"));
            }
            other => panic!("unexpected assistant_message output: {other:?}"),
        }
    }
}
