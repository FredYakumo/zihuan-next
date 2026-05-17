use std::collections::HashMap;

use crate::models::event_model::MessageType;
use zihuan_core::error::Result;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct ExtractOptionalGroupIdFromEventNode {
    id: String,
    name: String,
}

impl ExtractOptionalGroupIdFromEventNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ExtractOptionalGroupIdFromEventNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从消息事件中提取群号；私聊时返回空字符串")
    }

    node_input![
        port! { name = "message_event", ty = crate::models::event_model::MessageEvent, desc = "输入的消息事件" },
    ];

    node_output![port! { name = "result", ty = String, desc = "群号字符串；私聊时为空" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let event = match inputs.get("message_event") {
            Some(DataValue::MessageEvent(event)) => event,
            _ => return Err("message_event input is required".into()),
        };

        let group_id = if event.message_type == MessageType::Group {
            event
                .group_id
                .ok_or("group_id is missing in group message event")?
                .to_string()
        } else {
            String::new()
        };

        Ok(HashMap::from([(
            "result".to_string(),
            DataValue::String(group_id),
        )]))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::models::event_model::{MessageEvent, MessageType, Sender};
    use zihuan_graph_engine::{DataValue, Node};

    use super::ExtractOptionalGroupIdFromEventNode;

    fn build_event(message_type: MessageType, group_id: Option<i64>) -> MessageEvent {
        MessageEvent {
            message_id: 1,
            message_type: message_type.clone(),
            sender: Sender {
                user_id: 2,
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
    fn returns_group_id_for_group_message() {
        let mut node = ExtractOptionalGroupIdFromEventNode::new("test", "test");
        let outputs = node
            .execute(HashMap::from([(
                "message_event".to_string(),
                DataValue::MessageEvent(build_event(MessageType::Group, Some(123))),
            )]))
            .expect("group message should succeed");

        match outputs.get("result") {
            Some(DataValue::String(value)) => assert_eq!(value, "123"),
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn returns_empty_string_for_private_message() {
        let mut node = ExtractOptionalGroupIdFromEventNode::new("test", "test");
        let outputs = node
            .execute(HashMap::from([(
                "message_event".to_string(),
                DataValue::MessageEvent(build_event(MessageType::Private, None)),
            )]))
            .expect("private message should succeed");

        match outputs.get("result") {
            Some(DataValue::String(value)) => assert!(value.is_empty()),
            other => panic!("unexpected output: {other:?}"),
        }
    }
}
