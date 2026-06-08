use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::{str_to_role, LLMMessage, MessagePart};

/// Combines an optional text segment with a list of `MessagePart`s into a multimodal `LLMMessage`.
pub struct BuildMultimodalUserMessageNode {
    id: String,
    name: String,
}

impl BuildMultimodalUserMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for BuildMultimodalUserMessageNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将可选文本和若干 MessagePart 拼接为多模态 LLMMessage")
    }

    node_input![
        port! { name = "text", ty = String, desc = "前置文本段，可选；为空时只发送 parts", optional },
        port! { name = "parts", ty = Vec(MessagePart), desc = "要附加的多模态 MessagePart 列表", optional },
        port! { name = "role", ty = String, desc = "消息角色，可选 system / user / assistant / tool，默认 user", optional },
    ];

    node_output![port! { name = "message", ty = LLMMessage, desc = "封装后的多模态 LLMMessage" },];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let text = match inputs.get("text") {
            Some(DataValue::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(DataValue::String(_)) | None => None,
            Some(_) => return Err(Error::ValidationError("text must be a string".to_string())),
        };

        let parts: Vec<MessagePart> = match inputs.get("parts") {
            Some(DataValue::Vec(_, items)) => items
                .iter()
                .filter_map(|v| match v {
                    DataValue::MessagePart(p) => Some(p.clone()),
                    _ => None,
                })
                .collect(),
            Some(_) => {
                return Err(Error::ValidationError(
                    "parts must be Vec<MessagePart>".to_string(),
                ))
            }
            None => Vec::new(),
        };

        let role = match inputs.get("role") {
            Some(DataValue::String(s)) => str_to_role(s),
            Some(_) => return Err(Error::ValidationError("role must be a string".to_string())),
            None => str_to_role("user"),
        };

        let mut message_parts = Vec::with_capacity(parts.len() + usize::from(text.is_some()));
        if let Some(t) = text {
            message_parts.push(MessagePart::text(t));
        }
        message_parts.extend(parts);

        let message = LLMMessage {
            role,
            parts: message_parts,
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            usage: None,
        };

        crate::return_with_node_output![self;
            "message" => DataValue::LLMMessage(message),
        ]
    }
}
