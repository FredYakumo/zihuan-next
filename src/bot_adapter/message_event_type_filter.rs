use crate::bot_adapter::models::event_model::{MessageEvent, MessageType};
use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct MessageEventTypeFilterNode {
    id: String,
    name: String,
}

impl MessageEventTypeFilterNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { id: id.into(), name: name.into() }
    }
}

impl Node for MessageEventTypeFilterNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> { Some("根据消息类型（好友/群组）路由消息事件") }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "输入的消息事件" },
        port! { name = "filter_type",   ty = String,       desc = "过滤类型：private（好友消息）或 group（群组消息）", optional },
    ];

    node_output![
        port! { name = "true_event",  ty = MessageEvent, desc = "消息类型匹配时的输出" },
        port! { name = "false_event", ty = MessageEvent, desc = "消息类型不匹配时的输出" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        let event = match inputs.get("message_event") {
            Some(DataValue::MessageEvent(e)) => e.clone(),
            _ => return Err("message_event input is required".into()),
        };

        let filter_type = match inputs.get("filter_type") {
            Some(DataValue::String(s)) => s.clone(),
            _ => "private".to_string(),
        };

        let matches = match filter_type.as_str() {
            "group" => event.message_type == MessageType::Group,
            _ => event.message_type == MessageType::Private,
        };

        let mut outputs = HashMap::new();
        if matches {
            outputs.insert("true_event".to_string(), DataValue::MessageEvent(event));
        } else {
            outputs.insert("false_event".to_string(), DataValue::MessageEvent(event));
        }
        Ok(outputs)
    }
}
