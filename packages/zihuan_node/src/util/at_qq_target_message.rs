use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_bot_types::message::{AtTargetMessage, Message};
use zihuan_core::error::Result;

/// Converts a QQ target id string into an @ mention QQ message segment.
pub struct AtQQTargetMessageNode {
    id: String,
    name: String,
}

impl AtQQTargetMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AtQQTargetMessageNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将 QQ 目标 id 字符串转换为 @ 消息段")
    }

    node_input![port! { name = "id", ty = String, desc = "要 @ 的 QQ 目标 id" },];

    node_output![port! { name = "result", ty = QQMessage, desc = "输出 QQMessage At 消息段" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let id = match inputs.get("id") {
            Some(DataValue::String(id)) => id.clone(),
            _ => {
                return Err(zihuan_core::error::Error::InvalidNodeInput(
                    "id is required".to_string(),
                ))
            }
        };

        let qq_message = Message::At(AtTargetMessage { target: Some(id) });

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), DataValue::QQMessage(qq_message));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

