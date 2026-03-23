use std::collections::HashMap;

use crate::bot_adapter::models::event_model::MessageType;
use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};


pub struct ExtractGroupIdFromEventNode {
    id: String,
    name: String,
}

impl ExtractGroupIdFromEventNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ExtractGroupIdFromEventNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从群消息事件中提取群号（字符串）")
    }

    node_input![
        port! { name = "message_event", ty = crate::bot_adapter::models::event_model::MessageEvent, desc = "输入的消息事件" },
    ];

    node_output![
        port! { name = "result", ty = String, desc = "群号字符串" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        let event = match inputs.get("message_event") {
            Some(DataValue::MessageEvent(event)) => event,
            _ => return Err("message_event input is required".into()),
        };

        if event.message_type != MessageType::Group {
            return Err("message_event must be a group message".into());
        }

        let group_id = event
            .group_id
            .ok_or("group_id is missing in group message event")?;

        let mut outputs = HashMap::new();
        outputs.insert("result".to_string(), DataValue::String(group_id.to_string()));
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::ExtractGroupIdFromEventNode;
    use crate::bot_adapter::models::event_model::{MessageEvent, MessageType, Sender};
    use crate::node::{DataValue, Node};
    use std::collections::HashMap;

    fn make_event(message_type: MessageType, group_id: Option<i64>) -> MessageEvent {
        MessageEvent {
            message_id: 1,
            message_type,
            sender: Sender {
                user_id: 10001,
                nickname: "tester".to_string(),
                card: String::new(),
                role: None,
            },
            message_list: Vec::new(),
            group_id,
            group_name: None,
            is_group_message: message_type == MessageType::Group,
        }
    }

    #[test]
    fn extracts_group_id_for_group_message() {
        let mut node = ExtractGroupIdFromEventNode::new("node-1", "提取群号");
        let mut inputs = HashMap::new();
        inputs.insert(
            "message_event".to_string(),
            DataValue::MessageEvent(make_event(MessageType::Group, Some(123456))),
        );

        let outputs = node.execute(inputs).expect("should extract group id");

        match outputs.get("result") {
            Some(DataValue::String(value)) => assert_eq!(value, "123456"),
            _ => panic!("expected string result"),
        }
    }

    #[test]
    fn errors_for_private_message() {
        let mut node = ExtractGroupIdFromEventNode::new("node-1", "提取群号");
        let mut inputs = HashMap::new();
        inputs.insert(
            "message_event".to_string(),
            DataValue::MessageEvent(make_event(MessageType::Private, None)),
        );

        let err = node.execute(inputs).expect_err("should reject private message");
        assert!(err.to_string().contains("group message"));
    }
}