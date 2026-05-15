use std::collections::HashMap;

use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::{str_to_role, ContentPart, MessageContent, OpenAIMessage};

/// Combines an optional text segment with a list of `ContentPart`s into a multimodal `OpenAIMessage`.
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
        Some("将可选文本和若干 ContentPart 拼接为多模态 OpenAIMessage")
    }

    node_input![
        port! { name = "text", ty = String, desc = "前置文本段，可选；为空时只发送 parts", optional },
        port! { name = "parts", ty = Vec(ContentPart), desc = "要附加的多模态 ContentPart 列表", optional },
        port! { name = "role", ty = String, desc = "消息角色，可选 system / user / assistant / tool，默认 user", optional },
    ];

    node_output![
        port! { name = "message", ty = OpenAIMessage, desc = "封装后的多模态 OpenAIMessage" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let text = match inputs.get("text") {
            Some(DataValue::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(DataValue::String(_)) | None => None,
            Some(_) => return Err(Error::ValidationError("text must be a string".to_string())),
        };

        let parts: Vec<ContentPart> = match inputs.get("parts") {
            Some(DataValue::Vec(_, items)) => items
                .iter()
                .filter_map(|v| match v {
                    DataValue::ContentPart(p) => Some(p.clone()),
                    _ => None,
                })
                .collect(),
            Some(_) => {
                return Err(Error::ValidationError(
                    "parts must be Vec<ContentPart>".to_string(),
                ))
            }
            None => Vec::new(),
        };

        let role = match inputs.get("role") {
            Some(DataValue::String(s)) => str_to_role(s),
            Some(_) => return Err(Error::ValidationError("role must be a string".to_string())),
            None => str_to_role("user"),
        };

        let content = if parts.is_empty() {
            text.map(MessageContent::Text)
        } else {
            let mut all_parts = Vec::with_capacity(parts.len() + 1);
            if let Some(t) = text {
                all_parts.push(ContentPart::Text { text: t });
            }
            all_parts.extend(parts);
            Some(MessageContent::Parts(all_parts))
        };

        let message = OpenAIMessage {
            role,
            api_style: None,
            content,
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        };

        let mut outputs = HashMap::new();
        outputs.insert("message".to_string(), DataValue::OpenAIMessage(message));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
