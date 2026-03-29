use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct BooleanNotNode {
    id: String,
    name: String,
}

impl BooleanNotNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for BooleanNotNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("对 Boolean 输入取反")
    }

    node_input![
        port! { name = "input", ty = Boolean, desc = "输入布尔值" },
    ];

    node_output![
        port! { name = "result", ty = Boolean, desc = "取反后的布尔值" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let input = match inputs.get("input") {
            Some(DataValue::Boolean(value)) => *value,
            _ => {
                return Err(crate::error::Error::ValidationError(
                    "input 输入必须为 Boolean 类型".to_string(),
                ))
            }
        };

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), DataValue::Boolean(!input));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::BooleanNotNode;
    use crate::node::{DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn negates_true_to_false() {
        let mut node = BooleanNotNode::new("not", "Not");
        let outputs = node
            .execute(HashMap::from([(
                "input".to_string(),
                DataValue::Boolean(true),
            )]))
            .expect("boolean_not should execute");

        assert!(matches!(
            outputs.get("result"),
            Some(DataValue::Boolean(false))
        ));
    }

    #[test]
    fn negates_false_to_true() {
        let mut node = BooleanNotNode::new("not", "Not");
        let outputs = node
            .execute(HashMap::from([(
                "input".to_string(),
                DataValue::Boolean(false),
            )]))
            .expect("boolean_not should execute");

        assert!(matches!(
            outputs.get("result"),
            Some(DataValue::Boolean(true))
        ));
    }
}
