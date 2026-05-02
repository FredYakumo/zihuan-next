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
            "Brain",
        )?;
        Ok(())
    }

    fn set_tool_definitions(&mut self, tool_definitions: Vec<BrainToolDefinition>) -> Result<()> {
        self.tool_definitions = validate_tool_definitions(
            &tool_definitions,
            &self.shared_inputs,
            ToolResultMode::JsonObject,
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
        let output_names: Vec<String> = node
            .output_ports()
            .into_iter()
            .map(|port| port.name)
            .collect();
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

        let input_names: Vec<String> = node
            .input_ports()
            .into_iter()
            .map(|port| port.name)
            .collect();
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
            reasoning_content: None,
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
            reasoning_content: None,
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
                reasoning_content: None,
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
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
        ]));

        let mut node = BrainNode::new("brain_1", "Brain");

        let outputs = node
            .execute(HashMap::from([
                ("llm_model".to_string(), DataValue::LLModel(llm.clone())),
                ("messages".to_string(), messages_input()),
                (
                    BRAIN_TOOLS_CONFIG_PORT.to_string(),
                    DataValue::Json(json!([])),
                ),
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
            reasoning_content: None,
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
                reasoning_content: None,
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
                reasoning_content: None,
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
                reasoning_content: None,
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
                reasoning_content: None,
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
                (
                    "llm_ref".to_string(),
                    DataValue::LLModel(shared_llm.clone()),
                ),
            ]))
            .unwrap();

        match outputs.get("output") {
            Some(DataValue::Vec(_, items)) => {
                match &items[1] {
                    DataValue::OpenAIMessage(message) => {
                        assert_eq!(message.role, MessageRole::Tool);
                        assert_eq!(
                        message.content.as_deref(),
                        Some("{\"llm_ref\":{\"model_name\":\"sequence-llm\",\"type\":\"LLModel\"}}")
                    );
                    }
                    other => panic!("unexpected tool result item: {other:?}"),
                }
            }
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

        assert!(error.contains("reserved for tool-call content"));
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
                reasoning_content: None,
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
                reasoning_content: None,
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
                    assert_eq!(
                        message.content.as_deref(),
                        Some("{\"content\":\"call with context\"}")
                    );
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
                reasoning_content: None,
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
                reasoning_content: None,
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
