use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::ims_bot_adapter::models::message::{Message, PlainTextMessage};
use zihuan_core::error::Result;

/// Converts a String input into a QQMessage PlainText variant.
pub struct StringToPlainTextNode {
    id: String,
    name: String,
}

impl StringToPlainTextNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for StringToPlainTextNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将字符串转换为 QQ 消息中的纯文本（PlainText）消息段")
    }

    node_input![port! { name = "text", ty = String, desc = "输入字符串" },];

    node_output![
        port! { name = "result", ty = QQMessage, desc = "输出 QQMessage PlainText 消息段" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let text = match inputs.get("text") {
            Some(DataValue::String(s)) => s.clone(),
            _ => {
                return Err(zihuan_core::error::Error::InvalidNodeInput(
                    "text is required".to_string(),
                ))
            }
        };

        let qq_message = Message::PlainText(PlainTextMessage { text });

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), DataValue::QQMessage(qq_message));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
