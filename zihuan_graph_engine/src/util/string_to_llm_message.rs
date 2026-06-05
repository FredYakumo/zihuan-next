use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::{str_to_role, LLMMessage, MessagePart};

/// Converts a plain string into an `LLMMessage` with the selected role.
pub struct StringToLLMMessageNode {
    id: String,
    name: String,
}

impl StringToLLMMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for StringToLLMMessageNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将字符串封装为可选 role 的 LLMMessage")
    }

    node_input![
        port! { name = "content", ty = String, desc = "消息内容" },
        port! { name = "role", ty = String, desc = "LLMMessage 角色，可选 system / user / assistant / tool" },
    ];

    node_output![port! { name = "message", ty = LLMMessage, desc = "封装后的 LLMMessage" },];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
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

        crate::return_with_node_output![self;
            "message" => DataValue::LLMMessage(LLMMessage {
                role,
                parts: vec![MessagePart::text(content)],
                reasoning_content: None,
                tool_calls: Vec::new(),
                tool_call_id: None,
                usage: None,
            }),
        ]
    }
}
