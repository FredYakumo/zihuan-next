use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::Result;

/// Routes one of two inputs to the output based on a boolean condition.
pub struct ConditionalRouterNode {
    id: String,
    name: String,
}

impl ConditionalRouterNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ConditionalRouterNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("按布尔条件在 primary 和 fallback 两路输入之间选择一路输出")
    }

    node_input![
        port! { name = "condition", ty = Boolean, desc = "条件为 true 时选择 primary，否则选择 fallback" },
        port! { name = "primary", ty = Any, desc = "condition=true 时输出的值" },
        port! { name = "fallback", ty = Any, desc = "condition=false 时输出的值" },
    ];

    node_output![
        port! { name = "result", ty = Any, desc = "被选中的输入值，原样透传" },
        port! { name = "branch_taken", ty = String, desc = "实际走到的分支：primary 或 fallback" },
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

        let (result, branch_taken) = if condition {
            (
                inputs.get("primary").cloned().ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError("primary 输入不存在".to_string())
                })?,
                "primary",
            )
        } else {
            (
                inputs.get("fallback").cloned().ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError("fallback 输入不存在".to_string())
                })?,
                "fallback",
            )
        };

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), result);
        outputs.insert(
            "branch_taken".to_string(),
            DataValue::String(branch_taken.to_string()),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

