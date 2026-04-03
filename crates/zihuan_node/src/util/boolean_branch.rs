use zihuan_core::error::Result;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

/// Routes a single input value to one of two outputs based on a boolean condition.
pub struct BooleanBranchNode {
    id: String,
    name: String,
}

impl BooleanBranchNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for BooleanBranchNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("根据 Boolean 输入将同一份数据分流到 true 或 false 分支，另一分支不输出")
    }

    node_input![
        port! { name = "condition", ty = Boolean, desc = "为 true 时走 true_output，否则走 false_output" },
        port! { name = "input", ty = Any, desc = "要分流到某个分支的输入值" },
    ];

    node_output![
        port! { name = "true_output", ty = Any, desc = "condition=true 时输出" },
        port! { name = "false_output", ty = Any, desc = "condition=false 时输出" },
        port! { name = "branch_taken", ty = String, desc = "实际走到的分支：true 或 false" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let condition = match inputs.get("condition") {
            Some(DataValue::Boolean(value)) => *value,
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "condition 输入必须为 Boolean".to_string(),
                ))
            }
        };

        let input = inputs
            .get("input")
            .cloned()
            .ok_or_else(|| zihuan_core::error::Error::ValidationError("input 输入不存在".to_string()))?;

        let mut outputs = HashMap::new();
        if condition {
            outputs.insert("true_output".to_string(), input);
            outputs.insert(
                "branch_taken".to_string(),
                DataValue::String("true".to_string()),
            );
        } else {
            outputs.insert("false_output".to_string(), input);
            outputs.insert(
                "branch_taken".to_string(),
                DataValue::String("false".to_string()),
            );
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::BooleanBranchNode;
    use zihuan_core::error::Result;
    use crate::{DataValue, Node};
    use std::collections::HashMap;

    #[test]
    fn routes_to_true_output_when_condition_is_true() -> Result<()> {
        let mut node = BooleanBranchNode::new("branch", "Branch");
        let outputs = node.execute(HashMap::from([
            ("condition".to_string(), DataValue::Boolean(true)),
            ("input".to_string(), DataValue::String("hello".to_string())),
        ]))?;

        assert!(matches!(
            outputs.get("true_output"),
            Some(DataValue::String(value)) if value == "hello"
        ));
        assert!(!outputs.contains_key("false_output"));
        assert!(matches!(
            outputs.get("branch_taken"),
            Some(DataValue::String(value)) if value == "true"
        ));

        Ok(())
    }

    #[test]
    fn routes_to_false_output_when_condition_is_false() -> Result<()> {
        let mut node = BooleanBranchNode::new("branch", "Branch");
        let outputs = node.execute(HashMap::from([
            ("condition".to_string(), DataValue::Boolean(false)),
            ("input".to_string(), DataValue::Integer(42)),
        ]))?;

        assert!(matches!(
            outputs.get("false_output"),
            Some(DataValue::Integer(value)) if *value == 42
        ));
        assert!(!outputs.contains_key("true_output"));
        assert!(matches!(
            outputs.get("branch_taken"),
            Some(DataValue::String(value)) if value == "false"
        ));

        Ok(())
    }
}
