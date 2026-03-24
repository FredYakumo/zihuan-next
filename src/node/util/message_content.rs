use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

/// Extracts the `content` field of an `OpenAIMessage` as a plain string.
pub struct MessageContentNode {
    id: String,
    name: String,
}

impl MessageContentNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MessageContentNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从 OpenAIMessage 中提取 content 字段，以字符串形式输出")
    }

    node_input![
        port! { name = "message", ty = OpenAIMessage, desc = "输入的 LLM OpenAIMessage" },
    ];

    node_output![
        port! { name = "content", ty = String, desc = "OpenAIMessage 的 content 字符串，若为 None 则输出空字符串" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let content = inputs
                    .get("message")
                    .and_then(|v| match v {
                        DataValue::OpenAIMessage(m) => m.content.clone(),
                        _ => None,
                    }).ok_or(crate::error::Error::ValidationError("OpenAIMessage content is None".to_string()))?;

        let mut outputs = HashMap::new();
        outputs.insert("content".to_string(), DataValue::String(content));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
