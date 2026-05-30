use crate::{node_input, node_output, node_output_flow, DataType, Node, Port};
use zihuan_core::error::Result;

pub struct PreviewQQMessageListNode {
    id: String,
    name: String,
}

impl PreviewQQMessageListNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for PreviewQQMessageListNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("在节点卡片内实时预览 QQMessage 列表（含图片）")
    }

    node_input![
        port! { name = "messages", ty = Vec(QQMessage), desc = "要预览的 QQ 消息列表", optional },
    ];

    node_output![];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let outputs = if let Some(value) = inputs.get("messages") {
            node_output_flow!["messages" => value.clone()]
        } else {
            node_output_flow![]
        };

        let outputs = inputs.get("messages").cloned().map_or_else(
            || node_output_flow![],
            |messages| node_output_flow!["messages" => messages],
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
