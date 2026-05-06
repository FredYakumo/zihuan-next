use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::{Error, Result};

/// Converts `reasoning_content` (if any) and `content` of an `OpenAIMessage`
/// into a single string.
pub struct OpenAIMessageToStringNode {
    id: String,
    name: String,
}

impl OpenAIMessageToStringNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for OpenAIMessageToStringNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将 OpenAIMessage 的 reasoning_content（如有）与 content 拼接为字符串")
    }

    node_input![port! { name = "message", ty = OpenAIMessage, desc = "输入的 OpenAIMessage" },];

    node_output![
        port! { name = "content", ty = String, desc = "拼接后的字符串：先 reasoning_content（如有），后 content" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let message = match inputs.get("message") {
            Some(DataValue::OpenAIMessage(message)) => message,
            _ => return Err(Error::InvalidNodeInput("message is required".to_string())),
        };

        let mut parts = Vec::new();

        if let Some(reasoning_content) = message.reasoning_content.as_ref() {
            if !reasoning_content.is_empty() {
                parts.push(reasoning_content.clone());
            }
        }

        if let Some(content) = message.content_text_owned() {
            if !content.is_empty() {
                parts.push(content);
            }
        }

        if parts.is_empty() {
            return Err(Error::ValidationError(
                "OpenAIMessage reasoning_content and content are both empty".to_string(),
            ));
        }

        let mut outputs = HashMap::new();
        outputs.insert("content".to_string(), DataValue::String(parts.join("\n\n")));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
