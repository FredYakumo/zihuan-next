use std::collections::HashMap;

use zihuan_core::error::{Error, Result};
use zihuan_llm_types::{str_to_role, OpenAIMessage};
use crate::{node_input, node_output, DataType, DataValue, Node, Port};

/// Converts a plain string into an `OpenAIMessage` with the selected role.
pub struct StringToOpenAIMessageNode {
    id: String,
    name: String,
}

impl StringToOpenAIMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for StringToOpenAIMessageNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将字符串封装为可选 role 的 OpenAIMessage")
    }

    node_input![
        port! { name = "content", ty = String, desc = "消息内容" },
        port! { name = "role", ty = String, desc = "OpenAIMessage 角色，可选 system / user / assistant / tool" },
    ];

    node_output![port! { name = "message", ty = OpenAIMessage, desc = "封装后的 OpenAIMessage" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let content = match inputs.get("content") {
            Some(DataValue::String(content)) => content.clone(),
            _ => return Err(Error::ValidationError("content is required".to_string())),
        };

        let role = match inputs.get("role") {
            Some(DataValue::String(role)) => str_to_role(role),
            Some(_) => return Err(Error::ValidationError("role must be a string".to_string())),
            None => str_to_role("system"),
        };

        let mut outputs = HashMap::new();
        outputs.insert(
            "message".to_string(),
            DataValue::OpenAIMessage(OpenAIMessage {
                role,
                content: Some(content),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::StringToOpenAIMessageNode;
    use zihuan_llm_types::MessageRole;
    use crate::{DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn converts_string_to_system_message_by_default() {
        let mut node = StringToOpenAIMessageNode::new("msg_1", "StringToOpenAIMessage");
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

    #[test]
    fn converts_string_to_selected_role_message() {
        let mut node = StringToOpenAIMessageNode::new("msg_2", "StringToOpenAIMessage");
        let outputs = node
            .execute(HashMap::from([
                ("content".to_string(), DataValue::String("你好".to_string())),
                (
                    "role".to_string(),
                    DataValue::String("assistant".to_string()),
                ),
            ]))
            .unwrap();

        match outputs.get("message") {
            Some(DataValue::OpenAIMessage(message)) => {
                assert!(matches!(message.role, MessageRole::Assistant));
                assert_eq!(message.content.as_deref(), Some("你好"));
            }
            other => panic!("unexpected output: {:?}", other),
        }
    }
}
