use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct QQMessageListDataNode {
    id: String,
    name: String,
}

impl QQMessageListDataNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for QQMessageListDataNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("QQMessageList data source with inline UI editor")
    }

    node_input![
        port! { name = "messages", ty = Vec(QQMessage), desc = "Vec<QQMessage> provided by UI inline editor", optional },
    ];

    node_output![
        port! { name = "messages", ty = Vec(QQMessage), desc = "Output Vec<QQMessage> from UI data source" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        let value = inputs
            .get("messages")
            .cloned()
            .filter(|v| matches!(v, DataValue::Vec(..)))
            .unwrap_or_else(|| DataValue::Vec(Box::new(crate::node::DataType::QQMessage), Vec::new()));
        outputs.insert("messages".to_string(), value);

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
