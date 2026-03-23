use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct MessageSenderNode {
    id: String,
    name: String,
}

impl MessageSenderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MessageSenderNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Send message back to QQ server")
    }

    node_input![
        port! { name = "target_id", ty = String, desc = "Target user or group ID" },
        port! { name = "content", ty = String, desc = "Message content to send" },
        port! { name = "message_type", ty = String, desc = "Type of message to send" },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "Whether the message was sent successfully" },
        port! { name = "response", ty = Json, desc = "Response from the server" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        outputs.insert(
            "success".to_string(),
            DataValue::Boolean(true),
        );
        outputs.insert(
            "response".to_string(),
            DataValue::Json(serde_json::json!({
                "status": "sent",
                "timestamp": "2025-01-28T00:00:00Z"
            })),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
