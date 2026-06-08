use crate::{node_input, node_output, DataType, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::Result;

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
        Some("Preview LLMMessage list inside the node card with scrollable message items")
    }

    node_input![
        port! { name = "messages", ty = Vec(LLMMessage), desc = "Vec<LLMMessage> to preview inside the node", optional },
    ];

    node_output![];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        if let Some(value) = inputs.get("messages") {
            outputs.insert("messages".to_string(), value.clone());
        }

        let outputs = crate::NodeOutputFlow::from(outputs);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
