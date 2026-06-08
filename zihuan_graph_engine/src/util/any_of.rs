use crate::{node_input, node_output, node_output_flow, DataType, Node, Port};
use zihuan_core::error::Result;

/// Waits for multiple inputs, then forwards one of them unchanged.
pub struct AnyOfNode {
    id: String,
    name: String,
}

impl AnyOfNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for AnyOfNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("任意一个输入到齐后就原样透传该输入，适用于多个输入中只需要一个到齐即可继续执行的场景")
    }

    node_input![
        port! { name = "first", ty = Any, desc = "输入", optional },
        port! { name = "second", ty = Any, desc = "输入", optional },
    ];

    node_output![port! { name = "output", ty = Any, desc = "输出" },];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let output = inputs
            .get("first")
            .or_else(|| inputs.get("second"))
            .cloned()
            .ok_or_else(|| zihuan_core::validation_error!("AnyOfNode 至少需要一个输入 (first 或 second)"))?;

        crate::return_with_node_output![self;
            "output" => output,
        ]
    }
}
