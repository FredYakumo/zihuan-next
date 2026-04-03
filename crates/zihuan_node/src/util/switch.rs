use zihuan_core::error::Result;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

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

#[cfg(test)]
mod tests {
    use super::SwitchNode;
    use crate::{DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn switch_node_forwards_when_enabled() {
        let mut node = SwitchNode::new("switch", "Switch");
        let mut inputs = HashMap::new();
        inputs.insert("enabled".to_string(), DataValue::Boolean(true));
        inputs.insert("input".to_string(), DataValue::String("hello".to_string()));

        let outputs = node.execute(inputs).expect("switch should execute");
        match outputs.get("output") {
            Some(DataValue::String(value)) => assert_eq!(value, "hello"),
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn switch_node_blocks_when_disabled() {
        let mut node = SwitchNode::new("switch", "Switch");
        let mut inputs = HashMap::new();
        inputs.insert("enabled".to_string(), DataValue::Boolean(false));
        inputs.insert("input".to_string(), DataValue::Integer(42));

        let outputs = node.execute(inputs).expect("switch should execute");
        assert!(outputs.is_empty());
    }
}
