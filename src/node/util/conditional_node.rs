use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct ConditionalNode {
    id: String,
    name: String,
}

impl ConditionalNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ConditionalNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Conditional branching based on input condition")
    }

    node_input![
        port! { name = "condition", ty = Boolean, desc = "Condition to evaluate" },
        port! { name = "true_value", ty = Json, desc = "Value to output if condition is true" },
        port! { name = "false_value", ty = Json, desc = "Value to output if condition is false" },
    ];

    node_output![
        port! { name = "result", ty = Json, desc = "Selected value based on condition" },
        port! { name = "branch_taken", ty = String, desc = "Which branch was taken" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::Boolean(condition)) = inputs.get("condition") {
            let (result, branch) = if *condition {
                (
                    inputs.get("true_value").cloned().unwrap_or(DataValue::Json(serde_json::json!(null))),
                    "true",
                )
            } else {
                (
                    inputs.get("false_value").cloned().unwrap_or(DataValue::Json(serde_json::json!(null))),
                    "false",
                )
            };

            outputs.insert("result".to_string(), result);
            outputs.insert("branch_taken".to_string(), DataValue::String(branch.to_string()));
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
