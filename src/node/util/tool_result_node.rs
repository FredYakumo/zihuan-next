use std::collections::HashMap;

use crate::error::{Error, Result};
use zihuan_llm::OpenAIMessage;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};

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

#[cfg(test)]
mod tests {
    use super::ToolResultNode;
    use zihuan_llm::MessageRole;
    use crate::node::{DataValue, Node};
    use serde_json::json;
    use std::collections::HashMap;

    fn tool_call_json(tool_call_id: &str) -> DataValue {
        DataValue::Json(json!({ "tool_call_id": tool_call_id, "arguments": {} }))
    }

    #[test]
    fn builds_tool_result_message_with_correct_id() {
        let mut node = ToolResultNode::new("tr1", "ToolResult");
        let outputs = node
            .execute(HashMap::from([
                ("tool_call".to_string(), tool_call_json("call_abc")),
                ("content".to_string(), DataValue::String("42".to_string())),
            ]))
            .unwrap();

        match outputs.get("message") {
            Some(DataValue::OpenAIMessage(msg)) => {
                assert!(matches!(msg.role, MessageRole::Tool));
                assert_eq!(msg.tool_call_id.as_deref(), Some("call_abc"));
                assert_eq!(msg.content.as_deref(), Some("42"));
            }
            other => panic!("unexpected output: {:?}", other),
        }
    }

    #[test]
    fn errors_when_tool_call_id_missing() {
        let mut node = ToolResultNode::new("tr1", "ToolResult");
        let err = node
            .execute(HashMap::from([
                (
                    "tool_call".to_string(),
                    DataValue::Json(json!({ "arguments": {} })),
                ),
                (
                    "content".to_string(),
                    DataValue::String("result".to_string()),
                ),
            ]))
            .unwrap_err();

        assert!(err.to_string().contains("tool_call_id"));
    }
}
