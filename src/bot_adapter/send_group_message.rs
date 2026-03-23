use crate::bot_adapter::ws_action::{qq_message_list_to_json, ws_send_action};
use crate::error::Result;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct SendGroupMessageNode {
    id: String,
    name: String,
}

impl SendGroupMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { id: id.into(), name: name.into() }
    }
}

impl Node for SendGroupMessageNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> { Some("向QQ群组发送消息") }

    node_input![
        port! { name = "bot_adapter", ty = BotAdapterRef, desc = "Bot适配器引用" },
        port! { name = "target_id", ty = String, desc = "目标群的群号" },
        port! { name = "message", ty = Vec(QQMessage), desc = "要发送的QQ消息段列表" }
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "是否发送成功" },
        port! { name = "message_id", ty = Integer, desc = "服务器返回的消息ID" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let adapter_ref = match inputs.get("bot_adapter") {
            Some(DataValue::BotAdapterRef(r)) => r.clone(),
            _ => return Err("bot_adapter input is required".into()),
        };
        let target_id = match inputs.get("target_id") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err("target_id input is required".into()),
        };
        let messages: Vec<crate::bot_adapter::models::message::Message> = match inputs.get("message") {
            Some(DataValue::Vec(_, items)) => items.iter().filter_map(|item| {
                if let DataValue::QQMessage(m) = item { Some(m.clone()) } else { None }
            }).collect(),
            _ => return Err("message input is required".into()),
        };

        let params = serde_json::json!({
            "group_id": target_id,
            "message": qq_message_list_to_json(&messages),
        });
        let response = ws_send_action(&adapter_ref, "send_group_msg", params)?;

        let success = response.get("retcode").and_then(|v| v.as_i64()).unwrap_or(-1) == 0;
        let message_id = response
            .get("data").and_then(|d| d.get("message_id")).and_then(|v| v.as_i64())
            .unwrap_or(-1);

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert("message_id".to_string(), DataValue::Integer(message_id));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
