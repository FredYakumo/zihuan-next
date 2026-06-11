use std::collections::HashMap;

use zihuan_core::error::Result;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct ExtractSenderFromEventNode {
    id: String,
    name: String,
}

impl ExtractSenderFromEventNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ExtractSenderFromEventNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从消息事件中提取可用于回发的 Sender")
    }

    node_input![
        port! { name = "message_event", ty = crate::models::event_model::MessageEvent, desc = "输入的消息事件" },
    ];

    node_output![port! { name = "result", ty = Sender, desc = "可用于发送消息的 Sender" },];

    fn execute(&mut self, inputs: zihuan_graph_engine::NodeInputFlow) -> Result<zihuan_graph_engine::NodeOutputFlow> {
        let event = match inputs.get("message_event") {
            Some(DataValue::MessageEvent(event)) => event,
            _ => return Err("message_event input is required".into()),
        };

        let sender = crate::models::sender_model::Sender::from_message_event(event)
            .ok_or_else(|| "group message is missing group_id".to_string())?;

        zihuan_graph_engine::return_with_node_output![self;
            "result" => DataValue::Sender(sender),
        ]
    }
}
