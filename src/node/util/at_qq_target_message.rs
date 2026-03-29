use crate::bot_adapter::models::message::{AtTargetMessage, Message};
use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

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

    node_input![
        port! { name = "id", ty = String, desc = "要 @ 的 QQ 目标 id" },
    ];

    node_output![
        port! { name = "result", ty = QQMessage, desc = "输出 QQMessage At 消息段" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let id = match inputs.get("id") {
            Some(DataValue::String(id)) => id.clone(),
            _ => {
                return Err(crate::error::Error::InvalidNodeInput(
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

#[cfg(test)]
mod tests {
    use super::AtQQTargetMessageNode;
    use crate::bot_adapter::models::message::Message;
    use crate::node::{DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn converts_id_to_at_message() {
        let mut node = AtQQTargetMessageNode::new("at_1", "AtQQTargetMessage");
        let outputs = node
            .execute(HashMap::from([(
                "id".to_string(),
                DataValue::String("123456".to_string()),
            )]))
            .expect("at_qq_target_message should execute");

        match outputs.get("result") {
            Some(DataValue::QQMessage(Message::At(at))) => {
                assert_eq!(at.target.as_deref(), Some("123456"));
            }
            other => panic!("unexpected output: {:?}", other),
        }
    }
}
