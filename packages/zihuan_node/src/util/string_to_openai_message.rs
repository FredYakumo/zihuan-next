use std::collections::HashMap;

use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use zihuan_core::error::{Error, Result};
use zihuan_llm_types::{str_to_role, OpenAIMessage};

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
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: None,
            }),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

