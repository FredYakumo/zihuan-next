use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct IsAtMeNode {
    id: String,
    name: String,
}

impl IsAtMeNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for IsAtMeNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Extracts the is_at_me boolean field from a MessageProp")
    }

    node_input![
        port! { name = "msg_prop", ty = MessageProp, desc = "Parsed message properties" },
    ];

    node_output![
        port! { name = "is_at_me", ty = Boolean, desc = "Whether the message @'s the bot" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::MessageProp(prop)) = inputs.get("msg_prop") {
            outputs.insert("is_at_me".to_string(), DataValue::Boolean(prop.is_at_me));
        } else {
            return Err("msg_prop input is required and must be MessageProp type".into());
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
