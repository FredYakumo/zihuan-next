use crate::error::Result;
use crate::node::{DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct BotAdapterNode {
    id: String,
    name: String,
}

impl BotAdapterNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for BotAdapterNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("QQ Bot Adapter - receives messages from QQ server")
    }

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("trigger", DataType::Boolean)
                .with_description("Trigger to start receiving messages"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("message", DataType::Json)
                .with_description("Raw message event from QQ server"),
            Port::new("message_type", DataType::String)
                .with_description("Type of the message"),
            Port::new("user_id", DataType::String)
                .with_description("User ID who sent the message"),
            Port::new("content", DataType::String)
                .with_description("Message content"),
        ]
    }

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();


        outputs.insert(
            "message".to_string(),
            DataValue::Json(serde_json::json!({
                "message_type": "text",
                "content": "example message"
            })),
        );
        outputs.insert(
            "message_type".to_string(),
            DataValue::String("text".to_string()),
        );
        outputs.insert(
            "user_id".to_string(),
            DataValue::String("12345".to_string()),
        );
        outputs.insert(
            "content".to_string(),
            DataValue::String("example message".to_string()),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

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

    fn input_ports(&self) -> Vec<Port> {
        vec![
            Port::new("target_id", DataType::String)
                .with_description("Target user or group ID"),
            Port::new("content", DataType::String)
                .with_description("Message content to send"),
            Port::new("message_type", DataType::String)
                .with_description("Type of message to send"),
        ]
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![
            Port::new("success", DataType::Boolean)
                .with_description("Whether the message was sent successfully"),
            Port::new("response", DataType::Json)
                .with_description("Response from the server"),
        ]
    }

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
