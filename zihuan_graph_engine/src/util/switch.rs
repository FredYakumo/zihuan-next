use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::Result;

pub struct SwitchNode {
    id: String,
    name: String,
}

impl SwitchNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for SwitchNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Forward input only when enabled is true")
    }

    node_input![
        port! { name = "enabled", ty = Boolean, desc = "Whether the switch is open" },
        port! { name = "input", ty = Any, desc = "Input value to forward when enabled" },
    ];

    node_output![port! { name = "output", ty = Any, desc = "Forwarded output value" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if matches!(inputs.get("enabled"), Some(DataValue::Boolean(true))) {
            if let Some(value) = inputs.get("input") {
                outputs.insert("output".to_string(), value.clone());
            }
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
