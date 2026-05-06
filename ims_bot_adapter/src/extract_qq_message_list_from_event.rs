use std::collections::HashMap;
use zihuan_core::error::Result;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct ExtractQQMessageListFromEventNode {
    id: String,
    name: String,
}

impl ExtractQQMessageListFromEventNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ExtractQQMessageListFromEventNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从消息事件中提取 QQ 消息列表 (Vec<QQMessage>)")
    }

    node_input![
        port! { name = "message_event", ty = crate::models::event_model::MessageEvent, desc = "输入的消息事件" },
    ];

    node_output![port! { name = "message_list", ty = Vec(QQMessage), desc = "QQ 消息列表" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let event = match inputs.get("message_event") {
            Some(DataValue::MessageEvent(e)) => e.clone(),
            _ => return Err("message_event input is required".into()),
        };

        let message_list: Vec<DataValue> = event
            .message_list
            .into_iter()
            .map(DataValue::QQMessage)
            .collect();

        let mut outputs = HashMap::new();
        outputs.insert(
            "message_list".to_string(),
            DataValue::Vec(Box::new(DataType::QQMessage), message_list),
        );
        Ok(outputs)
    }
}
