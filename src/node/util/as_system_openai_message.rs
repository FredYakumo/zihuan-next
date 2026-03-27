use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::llm::OpenAIMessage;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};

/// Converts a plain string into an `OpenAIMessage` with `role=system`.
pub struct AsSystemOpenAIMessageNode {
    id: String,
    name: String,
}

impl AsSystemOpenAIMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AsSystemOpenAIMessageNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将字符串封装为 role=system 的 OpenAIMessage")
    }

    node_input![
        port! { name = "content", ty = String, desc = "系统提示词内容" },
    ];

    node_output![
        port! { name = "message", ty = OpenAIMessage, desc = "role=system 的 OpenAIMessage" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let content = match inputs.get("content") {
            Some(DataValue::String(content)) => content.clone(),
            _ => return Err(Error::ValidationError("content is required".to_string())),
        };

        let mut outputs = HashMap::new();
        outputs.insert(
            "message".to_string(),
            DataValue::OpenAIMessage(OpenAIMessage::system(content)),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::AsSystemOpenAIMessageNode;
    use crate::llm::MessageRole;
    use crate::node::{DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn converts_string_to_system_message() {
        let mut node = AsSystemOpenAIMessageNode::new("as_sys_1", "AsSystemOpenAIMessage");
        let outputs = node
            .execute(HashMap::from([(
                "content".to_string(),
                DataValue::String("你是一个助手".to_string()),
            )]))
            .unwrap();

        match outputs.get("message") {
            Some(DataValue::OpenAIMessage(message)) => {
                assert!(matches!(message.role, MessageRole::System));
                assert_eq!(message.content.as_deref(), Some("你是一个助手"));
            }
            other => panic!("unexpected output: {:?}", other),
        }
    }
}
