use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::llm::function_tools::FunctionTool;
use crate::llm::{InferenceParam, MessageRole, OpenAIMessage};
use crate::node::{DataType, DataValue, Node, Port, node_input};

const TOOLS_CONFIG_PORT: &str = "tools_config";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolParamDef {
    pub name: String,
    pub data_type: DataType,
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
                    "description": format!("参数 {}", param.name),
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
        | DataType::MessageProp
        | DataType::OpenAIMessage
        | DataType::QQMessage
        | DataType::FunctionTools
        | DataType::BotAdapterRef
        | DataType::RedisRef
        | DataType::MySqlRef
        | DataType::OpenAIMessageSessionCacheRef
        | DataType::LLModel
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
        let mut ports = vec![Port::new("response", DataType::String)
            .with_description("LLM 返回的最终文本回复")];

        ports.extend(tool_definitions.iter().map(|tool| {
            Port::new(tool.name.clone(), DataType::Json)
                .with_description(format!("Tool '{}' 的调用参数 JSON", tool.name))
        }));

        ports
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

    node_input![
        port! { name = "llm_model", ty = LLModel, desc = "LLM 模型引用，由 LLM API 节点提供" },
        port! { name = "system_prompt", ty = String, desc = "系统提示词" },
        port! { name = "user_message", ty = String, desc = "用户输入消息" },
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

        let system_prompt = match inputs.get("system_prompt") {
            Some(DataValue::String(value)) => value.clone(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: system_prompt".to_string(),
                ));
            }
        };

        let user_message = match inputs.get("user_message") {
            Some(DataValue::String(value)) => value.clone(),
            _ => {
                return Err(Error::ValidationError(
                    "Missing required input: user_message".to_string(),
                ));
            }
        };

        let tools: Vec<Arc<dyn FunctionTool>> = self
            .tool_definitions
            .iter()
            .cloned()
            .map(|definition| Arc::new(DynamicFunctionTool::new(definition)) as Arc<dyn FunctionTool>)
            .collect();

        let messages = vec![
            OpenAIMessage {
                role: MessageRole::System,
                content: Some(system_prompt),
                tool_calls: Vec::new(),
            },
            OpenAIMessage {
                role: MessageRole::User,
                content: Some(user_message),
                tool_calls: Vec::new(),
            },
        ];

        let response = model.inference(&InferenceParam {
            messages: &messages,
            tools: Some(&tools),
        });

        let mut outputs = HashMap::new();
        outputs.insert(
            "response".to_string(),
            DataValue::String(response.content.clone().unwrap_or_default()),
        );

        let mut tool_payloads: HashMap<String, Vec<Value>> = HashMap::new();
        for tool_call in response.tool_calls {
            tool_payloads
                .entry(tool_call.function.name)
                .or_default()
                .push(tool_call.function.arguments);
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

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use serde_json::json;

    use super::{BrainNode, ToolDefinition, ToolParamDef};
    use crate::llm::function_tools::{FunctionTool, ToolCalls, ToolCallsFuncSpec};
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
        assert_eq!(output_names, vec!["response", "search"]);
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
            },
        });

        let outputs = node.execute(HashMap::from([
            ("llm_model".to_string(), DataValue::LLModel(llm)),
            (
                "system_prompt".to_string(),
                DataValue::String("You are helpful".to_string()),
            ),
            (
                "user_message".to_string(),
                DataValue::String("Find rust docs".to_string()),
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

        assert!(matches!(outputs.get("response"), Some(DataValue::String(text)) if text == "done"));
        assert!(matches!(outputs.get("search"), Some(DataValue::Json(value)) if *value == json!({"query": "rust", "limit": 3})));
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
                },
                ToolParamDef {
                    name: "limit".to_string(),
                    data_type: DataType::Integer,
                },
            ],
        };

        let tool = super::DynamicFunctionTool::new(definition);
        let schema = tool.parameters();
        assert_eq!(schema["properties"]["query"]["type"], "string");
        assert_eq!(schema["properties"]["limit"]["type"], "integer");
    }
}