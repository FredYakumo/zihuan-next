use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::Result;

pub struct PreviewStringNode {
    id: String,
    name: String,
}

impl PreviewStringNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for PreviewStringNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Preview input string inside the node card")
    }

    node_input![
        port! { name = "text", ty = String, desc = "Text to preview inside the node", optional },
    ];

    node_output![];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();
        if let Some(value) = inputs.get("text") {
            outputs.insert("text".to_string(), value.clone());
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
