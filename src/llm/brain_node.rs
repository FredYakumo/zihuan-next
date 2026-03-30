use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::llm::tooling::FunctionTool;
use crate::llm::{InferenceParam, OpenAIMessage};
use crate::node::{DataType, DataValue, Node, Port, node_input};

const TOOLS_CONFIG_PORT: &str = "tools_config";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolParamDef {
    pub name: String,
    pub data_type: DataType,
    #[serde(default)]
    pub desc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub parameters: Vec<ToolParamDef>,
}

#[derive(Debug, Clone)]
struct DynamicFunctionTool {
    definition: ToolDefinition,
}

impl DynamicFunctionTool {
    fn new(definition: ToolDefinition) -> Self {
        Self { definition }
    }
}

impl FunctionTool for DynamicFunctionTool {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters(&self) -> Value {
        let mut properties = Map::new();
        let mut required = Vec::new();

        for param in &self.definition.parameters {
            if param.name.trim().is_empty() {
                continue;
            }
            required.push(Value::String(param.name.clone()));
            properties.insert(
                param.name.clone(),
                json!({
                    "type": data_type_to_json_schema_type(&param.data_type),
                    "description": if param.desc.trim().is_empty() {
                        format!("参数 {}", param.name)
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
        | DataType::Custom(_) => "object",
        DataType::Binary => "string",
        DataType::Vec(_) => "array",
        DataType::Any => "object",
    }
}

fn validate_tool_definitions(tool_definitions: &[ToolDefinition]) -> Result<()> {
    let mut seen_tool_names = HashSet::new();

    for tool in tool_definitions {
        let tool_name = tool.name.trim();
        if tool_name.is_empty() {
            return Err(Error::ValidationError("Tool name cannot be empty".to_string()));
        }
        if !seen_tool_names.insert(tool_name.to_string()) {
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
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct BrainNode {
    id: String,
    name: String,
    tool_definitions: Vec<ToolDefinition>,
}

impl BrainNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            tool_definitions: Vec::new(),
        }
    }

    fn set_tool_definitions(&mut self, tool_definitions: Vec<ToolDefinition>) -> Result<()> {
        validate_tool_definitions(&tool_definitions)?;
        self.tool_definitions = tool_definitions;
        Ok(())
    }

    fn outputs_from_tool_definitions(tool_definitions: &[ToolDefinition]) -> Vec<Port> {
        let mut ports = vec![
            Port::new("assistant_message", DataType::OpenAIMessage)
                .with_description("LLM 返回的完整 assistant 消息（含 tool_calls，用于 agentic loop）"),
            Port::new("has_tool_call", DataType::Boolean)
                .with_description("LLM 返回的 assistant 消息是否包含 tool_calls，用于控制 agentic loop 继续或结束"),
        ];

        ports.extend(tool_definitions.iter().map(|tool| {
            Port::new(tool.name.clone(), DataType::Json)
                .with_description(format!("Tool '{}' 的调用参数 JSON", tool.name))
        }));

        ports
    }

    fn build_tool_payload(tool_call_id: String, arguments: Value) -> Value {
        let mut payload = Map::new();
        payload.insert("tool_call_id".to_string(), Value::String(tool_call_id));

        if let Value::Object(argument_map) = &arguments {
            for (key, value) in argument_map {
                payload.insert(key.clone(), value.clone());
            }
        }

        payload.insert("arguments".to_string(), arguments);
        Value::Object(payload)
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
}

impl Node for BrainNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用 system prompt 和 user message 发起一次带动态 Tools 的 LLM 推理")
    }

    fn has_dynamic_output_ports(&self) -> bool {
        true
    }

    node_input![
        port! { name = "llm_model", ty = LLModel, desc = "LLM 模型引用，由 LLM API 节点提供" },
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "消息列表（包含 system/user/assistant 等角色）" },
        port! { name = "tools_config", ty = Json, desc = "Tools 配置，由工具编辑器维护", optional },
    ];

    fn output_ports(&self) -> Vec<Port> {
        Self::outputs_from_tool_definitions(&self.tool_definitions)
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        match inline_values.get(TOOLS_CONFIG_PORT) {
            Some(DataValue::Json(value)) => {
                if value.is_null() {
                    self.tool_definitions.clear();
                    return Ok(());
                }

                let parsed = serde_json::from_value::<Vec<ToolDefinition>>(value.clone())
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
            let parsed = serde_json::from_value::<Vec<ToolDefinition>>(value.clone())
                .map_err(|e| Error::ValidationError(format!("Invalid tools_config: {e}")))?;
            self.set_tool_definitions(parsed)?;
        }

        let model = match inputs.get("llm_model") {
            Some(DataValue::LLModel(model)) => model.clone(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: llm_model".to_string(),
                ));
            }
        };

        let messages: Vec<OpenAIMessage> = match inputs.get("messages") {
            Some(DataValue::Vec(_, items)) => items
                .iter()
                .filter_map(|item| {
                    if let DataValue::OpenAIMessage(msg) = item {
                        Some(msg.clone())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: messages".to_string(),
                ));
            }
        };
        let messages = Self::sanitize_messages_for_inference(messages);

        let tools: Vec<Arc<dyn FunctionTool>> = self
            .tool_definitions
            .iter()
            .cloned()
            .map(|definition| Arc::new(DynamicFunctionTool::new(definition)) as Arc<dyn FunctionTool>)
            .collect();

        let response = model.inference(&InferenceParam {
            messages: &messages,
            tools: Some(&tools),
        });

        if let Some(content) = response.content.as_deref() {
            let is_transport_error = content.starts_with("Error: API request failed")
                || content.starts_with("Error: Failed to send request")
                || content.starts_with("Error: Failed to parse response")
                || content.starts_with("Error: Invalid response structure");
            if is_transport_error {
                return Err(Error::ValidationError(format!("LLM request failed: {}", content)));
            }
        }

        let has_tool_call = !response.tool_calls.is_empty();

        let mut outputs = HashMap::new();
        outputs.insert(
            "assistant_message".to_string(),
            DataValue::OpenAIMessage(response.clone()),
        );
        outputs.insert(
            "has_tool_call".to_string(),
            DataValue::Boolean(has_tool_call),
        );

        let mut tool_payloads: HashMap<String, Vec<Value>> = HashMap::new();
        for tool_call in response.tool_calls {
            info!(
                "[BrainNode] tool call: {} args={}",
                tool_call.function.name,
                tool_call.function.arguments
            );
            tool_payloads
                .entry(tool_call.function.name.clone())
                .or_default()
                .push(Self::build_tool_payload(
                    tool_call.id,
                    tool_call.function.arguments,
                ));
        }

        for tool in &self.tool_definitions {
            if let Some(payloads) = tool_payloads.remove(&tool.name) {
                let value = if payloads.len() == 1 {
                    payloads.into_iter().next().unwrap_or(Value::Null)
                } else {
                    Value::Array(payloads)
                };
                outputs.insert(tool.name.clone(), DataValue::Json(value));
            }
        }

        for (name, _) in &tool_payloads {
            warn!("[BrainNode] LLM returned tool call '{}' which does not match any tool definition", name);
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use serde_json::{Map, json, Value};

    use super::{BrainNode, ToolDefinition, ToolParamDef};
    use crate::llm::tooling::{FunctionTool, ToolCalls, ToolCallsFuncSpec};
    use crate::llm::llm_base::LLMBase;
    use crate::llm::{InferenceParam, MessageRole, OpenAIMessage};
    use crate::node::{DataType, DataValue, Node};

    #[derive(Debug)]
    struct TestLlm {
        response: OpenAIMessage,
    }

    impl LLMBase for TestLlm {
        fn get_model_name(&self) -> &str {
            "test-model"
        }

        fn inference(&self, _param: &InferenceParam) -> OpenAIMessage {
            self.response.clone()
        }
    }

    #[derive(Debug)]
    struct RecordingLlm {
        seen_messages: std::sync::Mutex<Vec<OpenAIMessage>>,
        response: OpenAIMessage,
    }

    impl LLMBase for RecordingLlm {
        fn get_model_name(&self) -> &str {
            "recording-model"
        }

        fn inference(&self, param: &InferenceParam) -> OpenAIMessage {
            *self.seen_messages.lock().unwrap() = param.messages.to_vec();
            self.response.clone()
        }
    }

    #[test]
    fn apply_inline_config_updates_dynamic_outputs() {
        let mut node = BrainNode::new("brain_1", "Brain");
        let inline_values = HashMap::from([(
            "tools_config".to_string(),
            DataValue::Json(json!([
                {
                    "name": "search",
                    "description": "Search docs",
                    "parameters": [
                        { "name": "query", "data_type": "String" }
                    ]
                }
            ])),
        )]);

        node.apply_inline_config(&inline_values).unwrap();

        let output_names: Vec<String> = node.output_ports().into_iter().map(|p| p.name).collect();
        assert_eq!(output_names, vec!["assistant_message", "has_tool_call", "search"]);
    }

    #[test]
    fn execute_routes_tool_arguments_to_json_output() {
        let mut node = BrainNode::new("brain_1", "Brain");
        node.apply_inline_config(&HashMap::from([(
            "tools_config".to_string(),
            DataValue::Json(json!([
                {
                    "name": "search",
                    "description": "Search docs",
                    "parameters": [
                        { "name": "query", "data_type": "String" },
                        { "name": "limit", "data_type": "Integer" }
                    ]
                }
            ])),
        )])).unwrap();

        let llm = Arc::new(TestLlm {
            response: OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("done".to_string()),
                tool_calls: vec![ToolCalls {
                    id: "tool_1".to_string(),
                    type_name: "function".to_string(),
                    function: ToolCallsFuncSpec {
                        name: "search".to_string(),
                        arguments: json!({"query": "rust", "limit": 3}),
                    },
                }],
                tool_call_id: None,
            },
        });

        let outputs = node.execute(HashMap::from([
            ("llm_model".to_string(), DataValue::LLModel(llm)),
            (
                "messages".to_string(),
                DataValue::Vec(
                    Box::new(DataType::OpenAIMessage),
                    vec![
                        DataValue::OpenAIMessage(OpenAIMessage::system("You are helpful")),
                        DataValue::OpenAIMessage(OpenAIMessage::user("Find rust docs")),
                    ],
                ),
            ),
            (
                "tools_config".to_string(),
                DataValue::Json(json!([
                    {
                        "name": "search",
                        "description": "Search docs",
                        "parameters": [
                            { "name": "query", "data_type": "String" },
                            { "name": "limit", "data_type": "Integer" }
                        ]
                    }
                ])),
            ),
        ])).unwrap();

        assert!(matches!(
            outputs.get("has_tool_call"),
            Some(DataValue::Boolean(true))
        ));
        assert!(matches!(outputs.get("search"), Some(DataValue::Json(value)) if *value == json!({
            "tool_call_id": "tool_1",
            "query": "rust",
            "limit": 3,
            "arguments": {"query": "rust", "limit": 3}
        })));
    }

    #[test]
    fn execute_sets_has_tool_call_false_when_no_tools_are_requested() {
        let mut node = BrainNode::new("brain_1", "Brain");
        let llm = Arc::new(TestLlm {
            response: OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("done".to_string()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        });

        let outputs = node.execute(HashMap::from([
            ("llm_model".to_string(), DataValue::LLModel(llm)),
            (
                "messages".to_string(),
                DataValue::Vec(
                    Box::new(DataType::OpenAIMessage),
                    vec![
                        DataValue::OpenAIMessage(OpenAIMessage::system("You are helpful")),
                        DataValue::OpenAIMessage(OpenAIMessage::user("Say hi")),
                    ],
                ),
            ),
        ])).unwrap();

        assert!(matches!(
            outputs.get("has_tool_call"),
            Some(DataValue::Boolean(false))
        ));
        assert!(!outputs.contains_key("search"));
    }

    #[test]
    fn execute_drops_dangling_tool_conversation_before_inference() {
        let mut node = BrainNode::new("brain_1", "Brain");
        let llm = Arc::new(RecordingLlm {
            seen_messages: std::sync::Mutex::new(Vec::new()),
            response: OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("done".to_string()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        });

        let outputs = node.execute(HashMap::from([
            ("llm_model".to_string(), DataValue::LLModel(llm.clone())),
            (
                "messages".to_string(),
                DataValue::Vec(
                    Box::new(DataType::OpenAIMessage),
                    vec![
                        DataValue::OpenAIMessage(OpenAIMessage::system("You are helpful")),
                        DataValue::OpenAIMessage(OpenAIMessage {
                            role: MessageRole::Assistant,
                            content: None,
                            tool_calls: vec![ToolCalls {
                                id: "call_1".to_string(),
                                type_name: "function".to_string(),
                                function: ToolCallsFuncSpec {
                                    name: "natural_language_reply".to_string(),
                                    arguments: json!({"content": "你好呀"}),
                                },
                            }],
                            tool_call_id: None,
                        }),
                        DataValue::OpenAIMessage(OpenAIMessage::tool_result("call_1", "sent")),
                        DataValue::OpenAIMessage(OpenAIMessage::user("?")),
                    ],
                ),
            ),
        ])).unwrap();

        assert!(matches!(
            outputs.get("has_tool_call"),
            Some(DataValue::Boolean(false))
        ));

        let seen_messages = llm.seen_messages.lock().unwrap().clone();
        assert_eq!(seen_messages.len(), 4);
        assert!(matches!(seen_messages[0].role, MessageRole::System));
        assert_eq!(seen_messages[0].content.as_deref(), Some("You are helpful"));
        assert!(matches!(seen_messages[1].role, MessageRole::Assistant));
        assert_eq!(seen_messages[1].tool_calls.len(), 1);
        assert!(matches!(seen_messages[2].role, MessageRole::Tool));
        assert_eq!(seen_messages[2].tool_call_id.as_deref(), Some("call_1"));
        assert!(matches!(seen_messages[3].role, MessageRole::User));
        assert_eq!(seen_messages[3].content.as_deref(), Some("?"));
    }

    #[test]
    fn execute_drops_orphan_tool_message_without_matching_assistant_tool_call() {
        let mut node = BrainNode::new("brain_1", "Brain");
        let llm = Arc::new(RecordingLlm {
            seen_messages: std::sync::Mutex::new(Vec::new()),
            response: OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some("done".to_string()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        });

        let outputs = node.execute(HashMap::from([
            ("llm_model".to_string(), DataValue::LLModel(llm.clone())),
            (
                "messages".to_string(),
                DataValue::Vec(
                    Box::new(DataType::OpenAIMessage),
                    vec![
                        DataValue::OpenAIMessage(OpenAIMessage::system("You are helpful")),
                        DataValue::OpenAIMessage(OpenAIMessage::user("你好")),
                        DataValue::OpenAIMessage(OpenAIMessage::tool_result("call_1", "sent")),
                        DataValue::OpenAIMessage(OpenAIMessage::user("?")),
                    ],
                ),
            ),
        ])).unwrap();

        assert!(matches!(
            outputs.get("has_tool_call"),
            Some(DataValue::Boolean(false))
        ));

        let seen_messages = llm.seen_messages.lock().unwrap().clone();
        assert_eq!(seen_messages.len(), 3);
        assert!(matches!(seen_messages[0].role, MessageRole::System));
        assert!(matches!(seen_messages[1].role, MessageRole::User));
        assert_eq!(seen_messages[1].content.as_deref(), Some("你好"));
        assert!(matches!(seen_messages[2].role, MessageRole::User));
        assert_eq!(seen_messages[2].content.as_deref(), Some("?"));
    }

    #[test]
    fn build_tool_payload_flattens_object_arguments() {
        let payload = BrainNode::build_tool_payload(
            "tool_1".to_string(),
            json!({ "content": "hello" }),
        );

        assert_eq!(
            payload,
            json!({
                "tool_call_id": "tool_1",
                "content": "hello",
                "arguments": { "content": "hello" }
            })
        );
    }

    #[test]
    fn build_tool_payload_keeps_non_object_arguments_under_arguments() {
        let payload = BrainNode::build_tool_payload("tool_1".to_string(), Value::String("hello".to_string()));

        let mut expected = Map::new();
        expected.insert("tool_call_id".to_string(), Value::String("tool_1".to_string()));
        expected.insert("arguments".to_string(), Value::String("hello".to_string()));

        assert_eq!(payload, Value::Object(expected));
    }

    #[test]
    fn dynamic_function_tool_schema_uses_datatype_mapping() {
        let definition = ToolDefinition {
            name: "search".to_string(),
            description: "Search docs".to_string(),
            parameters: vec![
                ToolParamDef {
                    name: "query".to_string(),
                    data_type: DataType::String,
                    desc: "搜索关键词".to_string(),
                },
                ToolParamDef {
                    name: "limit".to_string(),
                    data_type: DataType::Integer,
                    desc: "".to_string(),
                },
            ],
        };

        let tool = super::DynamicFunctionTool::new(definition);
        let schema = tool.parameters();
        assert_eq!(schema["properties"]["query"]["type"], "string");
        assert_eq!(schema["properties"]["query"]["description"], "搜索关键词");
        assert_eq!(schema["properties"]["limit"]["type"], "integer");
        assert_eq!(schema["properties"]["limit"]["description"], "参数 limit");
    }
}
