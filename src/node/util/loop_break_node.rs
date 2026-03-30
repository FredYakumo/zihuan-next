use crate::error::Result;
use crate::node::data_value::LoopControl;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;

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
                crate::error::Error::InvalidNodeInput("loop_control is required".to_string())
            })?;

        let condition = inputs
            .get("condition")
            .and_then(|v| match v {
                DataValue::Boolean(b) => Some(*b),
                _ => None,
            })
            .ok_or_else(|| {
                crate::error::Error::InvalidNodeInput("condition is required".to_string())
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

#[cfg(test)]
mod tests {
    use super::LoopBreakNode;
    use crate::node::data_value::LoopControl;
    use crate::node::{DataValue, Node};
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn loop_break_requests_break_and_forwards_optional_input() {
        let mut node = LoopBreakNode::new("loop_break", "Loop Break");
        let loop_control = Arc::new(LoopControl::new());
        let mut inputs = HashMap::new();
        inputs.insert(
            "loop_control".to_string(),
            DataValue::LoopControlRef(loop_control.clone()),
        );
        inputs.insert("condition".to_string(), DataValue::Boolean(true));
        inputs.insert("input".to_string(), DataValue::String("done".to_string()));

        let outputs = node.execute(inputs).expect("loop_break should execute");

        assert!(loop_control.should_break());
        match outputs.get("output") {
            Some(DataValue::String(value)) => assert_eq!(value, "done"),
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn loop_break_skips_output_when_optional_input_is_absent() {
        let mut node = LoopBreakNode::new("loop_break", "Loop Break");
        let loop_control = Arc::new(LoopControl::new());
        let mut inputs = HashMap::new();
        inputs.insert(
            "loop_control".to_string(),
            DataValue::LoopControlRef(loop_control.clone()),
        );
        inputs.insert("condition".to_string(), DataValue::Boolean(false));

        let outputs = node.execute(inputs).expect("loop_break should execute");

        assert!(!loop_control.should_break());
        assert!(outputs.is_empty());
    }
}
