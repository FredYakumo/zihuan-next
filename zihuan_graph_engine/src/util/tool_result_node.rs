use std::collections::HashMap;

use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::OpenAIMessage;

pub struct ToolResultNode {
    id: String,
    name: String,
}

impl ToolResultNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ToolResultNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将工具执行结果封装为 role=tool 的 OpenAIMessage，供 agentic loop 回写对话列表")
    }

    node_input![
        port! { name = "tool_call", ty = Json, desc = "BrainNode 工具端口输出的 {tool_call_id, arguments} JSON" },
        port! { name = "content", ty = String, desc = "工具执行结果内容" },
    ];

    node_output![
        port! { name = "message", ty = OpenAIMessage, desc = "role=tool 的结果消息，可拼入对话列表后重新送入 BrainNode" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let tool_call_id = match inputs.get("tool_call") {
            Some(DataValue::Json(v)) => v
                .get("tool_call_id")
                .and_then(|id| id.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    Error::ValidationError("tool_call missing 'tool_call_id' field".to_string())
                })?,
            _ => return Err(Error::ValidationError("tool_call is required".to_string())),
        };

        let content = match inputs.get("content") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err(Error::ValidationError("content is required".to_string())),
        };

        let message = OpenAIMessage::tool_result(tool_call_id, content);

        let mut outputs = HashMap::new();
        outputs.insert("message".to_string(), DataValue::OpenAIMessage(message));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
