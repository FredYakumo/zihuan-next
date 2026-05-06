use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::Result;

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

        let input = inputs.get("input").cloned().ok_or_else(|| {
            zihuan_core::error::Error::ValidationError("input 输入不存在".to_string())
        })?;

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
