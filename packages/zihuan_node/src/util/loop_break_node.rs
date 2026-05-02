use crate::data_value::LoopControl;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;
use zihuan_core::error::Result;

pub struct LoopBreakNode {
    id: String,
    name: String,
}

impl LoopBreakNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for LoopBreakNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("当 condition 为 true 时，通知循环节点在下一轮退出；放置在循环链路最末端")
    }

    node_input![
        port! { name = "loop_control", ty = LoopControlRef, desc = "来自 LoopNode 的循环控制引用" },
        port! { name = "condition", ty = Boolean, desc = "为 true 时触发退出循环" },
        port! { name = "input", ty = Any, desc = "可选：循环结束后透传给后续节点的数据", optional },
    ];

    node_output![
        port! { name = "output", ty = Any, desc = "透传 input 的值，便于在循环结束后继续后续节点" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let loop_control: Arc<LoopControl> = inputs
            .get("loop_control")
            .and_then(|v| match v {
                DataValue::LoopControlRef(r) => Some(r.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("loop_control is required".to_string())
            })?;

        let condition = inputs
            .get("condition")
            .and_then(|v| match v {
                DataValue::Boolean(b) => Some(*b),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("condition is required".to_string())
            })?;

        if condition {
            loop_control.request_break();
        }

        let mut outputs = HashMap::new();
        if let Some(value) = inputs.get("input") {
            outputs.insert("output".to_string(), value.clone());
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

