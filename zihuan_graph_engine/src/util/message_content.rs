use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use node_macros::node_output_flow;
use std::collections::HashMap;
use zihuan_core::{error::Result, validation_error};

/// Extracts the `content` field of an `LLMMessage` as a plain string.
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
        Some("从 LLMMessage 中提取 content 字段，以字符串形式输出")
    }

    node_input![port! { name = "message", ty = LLMMessage, desc = "输入的 LLM LLMMessage" },];

    node_output![
        port! { name = "content", ty = String, desc = "LLMMessage 的 content 字符串，若为 None 则输出空字符串" },
    ];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let content = inputs
            .get("message")
            .and_then(|v| match v {
                DataValue::LLMMessage(m) => m.content_text_owned(),
                _ => None,
            })
            .ok_or(validation_error!("LLMMessage content is None",))?;

        crate::return_with_node_output![self;
            "content" => DataValue::String(content),
        ]
    }
}
