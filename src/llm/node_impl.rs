use crate::error::Result;
use crate::node::{DataType, DataValue, Node, Port};
use std::collections::HashMap;
use super::llm_api::LLMAPI;

/// LLM API 作为 Node 的包装器
pub struct LLMNode {
    id: String,
    name: String,
    llm_api: Option<LLMAPI>,
    system_prompt: Option<String>,
}

impl LLMNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            llm_api: None,
            system_prompt: None,
        }
    }

    pub fn with_llm_api(mut self, llm_api: LLMAPI) -> Self {
        self.llm_api = Some(llm_api);
        self
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }
}

impl Node for LLMNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("LLM API Node - processes text with language model")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("prompt", DataType::String)
                .with_description("User prompt to send to LLM"),
            Port::new("messages", DataType::Json)
                .with_description("Full message history (optional)"),
            Port::new("max_tokens", DataType::Integer)
                .with_description("Maximum tokens in response (optional)"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("response", DataType::String)
                .with_description("LLM response text"),
            Port::new("full_message", DataType::Json)
                .with_description("Complete message object from LLM"),
            Port::new("token_usage", DataType::Json)
                .with_description("Token usage information"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        // 在实际实现中，这里应该调用真实的 LLM API
        // 现在提供一个示例输出
        if let Some(DataValue::String(prompt)) = inputs.get("prompt") {
            // 模拟 LLM 响应
            let response = format!("Response to: {}", prompt);
            
            outputs.insert(
                "response".to_string(),
                DataValue::String(response.clone()),
            );
            outputs.insert(
                "full_message".to_string(),
                DataValue::Json(serde_json::json!({
                    "role": "assistant",
                    "content": response,
                })),
            );
            outputs.insert(
                "token_usage".to_string(),
                DataValue::Json(serde_json::json!({
                    "prompt_tokens": 10,
                    "completion_tokens": 20,
                    "total_tokens": 30,
                })),
            );
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

/// Agent 节点 - 使用工具增强的 LLM
pub struct AgentNode {
    id: String,
    name: String,
    agent_type: String,
}

impl AgentNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>, agent_type: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            agent_type: agent_type.into(),
        }
    }
}

impl Node for AgentNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("AI Agent with tool-calling capabilities")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("task", DataType::String)
                .with_description("Task description for the agent"),
            Port::new("context", DataType::Json)
                .with_description("Additional context information"),
            Port::new("tools", DataType::Json)
                .with_description("Available tools for the agent"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("result", DataType::String)
                .with_description("Agent execution result"),
            Port::new("tool_calls", DataType::Json)
                .with_description("Tools called during execution"),
            Port::new("execution_log", DataType::Json)
                .with_description("Detailed execution log"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::String(task)) = inputs.get("task") {
            outputs.insert(
                "result".to_string(),
                DataValue::String(format!("Completed task: {}", task)),
            );
            outputs.insert(
                "tool_calls".to_string(),
                DataValue::Json(serde_json::json!([
                    {"tool": "search", "args": {"query": task}},
                ])),
            );
            outputs.insert(
                "execution_log".to_string(),
                DataValue::Json(serde_json::json!({
                    "steps": ["analyze task", "call tools", "synthesize result"],
                    "duration_ms": 1500,
                })),
            );
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

/// 文本处理节点
pub struct TextProcessorNode {
    id: String,
    name: String,
    operation: String,
}

impl TextProcessorNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            operation: operation.into(),
        }
    }
}

impl Node for TextProcessorNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Text processing node for various text operations")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("text", DataType::String)
                .with_description("Input text to process"),
            Port::new("params", DataType::Json)
                .with_description("Processing parameters"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("processed_text", DataType::String)
                .with_description("Processed text output"),
            Port::new("metadata", DataType::Json)
                .with_description("Processing metadata"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::String(text)) = inputs.get("text") {
            let processed = match self.operation.as_str() {
                "uppercase" => text.to_uppercase(),
                "lowercase" => text.to_lowercase(),
                "trim" => text.trim().to_string(),
                "reverse" => text.chars().rev().collect(),
                _ => text.clone(),
            };

            outputs.insert(
                "processed_text".to_string(),
                DataValue::String(processed),
            );
            outputs.insert(
                "metadata".to_string(),
                DataValue::Json(serde_json::json!({
                    "operation": self.operation,
                    "input_length": text.len(),
                })),
            );
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
