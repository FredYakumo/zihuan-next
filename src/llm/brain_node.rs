pub use zihuan_llm::brain_node::*;
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, Once};

    use serde_json::json;

    use super::BrainNode;
    use crate::llm::brain_tool::{
        BrainToolDefinition, ToolParamDef, BRAIN_SHARED_INPUTS_PORT, BRAIN_TOOLS_CONFIG_PORT,
    };
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
            crate::init_registry::init_node_registry().expect("registry should initialize");
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
        let mut subgraph = crate::node::function_graph::default_function_subgraph();
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
        let mut subgraph = crate::node::function_graph::default_function_subgraph();
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
