use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct PreviewMessageListNode {
    id: String,
    name: String,
}

impl PreviewMessageListNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for PreviewMessageListNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
            Some("Preview OpenAIMessage list inside the node card with scrollable message items")
    }

    node_input![
            port! { name = "messages", ty = Vec(OpenAIMessage), desc = "Vec<OpenAIMessage> to preview inside the node", optional },
    ];

    node_output![];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        if let Some(value) = inputs.get("messages") {
            outputs.insert("messages".to_string(), value.clone());
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
