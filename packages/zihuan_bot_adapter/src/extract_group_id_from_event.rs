use std::collections::HashMap;

use crate::models::event_model::MessageType;
use zihuan_core::error::Result;
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, Port};

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
        port! { name = "message_event", ty = crate::models::event_model::MessageEvent, desc = "输入的消息事件" },
    ];

    node_output![port! { name = "result", ty = String, desc = "群号字符串" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
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
        outputs.insert(
            "result".to_string(),
            DataValue::String(group_id.to_string()),
        );
        Ok(outputs)
    }
}

