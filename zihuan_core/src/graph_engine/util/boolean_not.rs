use crate::graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};
use crate::error::Result;

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

    node_input![port! { name = "input", ty = Boolean, desc = "输入布尔值" },];

    node_output![port! { name = "result", ty = Boolean, desc = "取反后的布尔值" },];

    fn execute(&mut self, inputs: crate::graph_engine::NodeInputFlow) -> Result<crate::graph_engine::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let input = match inputs.get("input") {
            Some(DataValue::Boolean(value)) => *value,
            _ => {
                return Err(crate::error::Error::ValidationError(
                    "input 输入必须为 Boolean 类型".to_string(),
                ))
            }
        };

        crate::graph_engine::return_with_node_output![self;
            "result" => DataValue::Boolean(!input),
        ]
    }
}
