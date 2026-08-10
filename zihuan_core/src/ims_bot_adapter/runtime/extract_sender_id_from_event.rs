use crate::graph::NodeOutputFlow;
use std::collections::HashMap;
use crate::error::Result;
use crate::graph::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct ExtractSenderIdFromEventNode {
    id: String,
    name: String,
}

impl ExtractSenderIdFromEventNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ExtractSenderIdFromEventNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        Some("从消息事件中提取发送者的QQ号（字符串）")
    }

    node_input![
        port! { name = "message_event", ty = crate::ims_bot_adapter::runtime::models::event_model::MessageEvent, desc = "输入的消息事件" },
    ];

    node_output![port! { name = "result", ty = String, desc = "发送者的QQ号字符串" },];

    fn execute(&mut self, inputs: crate::graph::NodeInputFlow) -> Result<crate::graph::NodeOutputFlow> {
        let event = match inputs.get("message_event") {
            Some(DataValue::MessageEvent(e)) => e.clone(),
            _ => return Err("message_event input is required".into()),
        };

        crate::graph::return_with_node_output![self;
            "result" => DataValue::String(event.sender.user_id.to_string()),
        ]
    }
}
