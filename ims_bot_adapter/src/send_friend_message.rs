use crate::send_qq_message_batches::{describe_message_segments, qq_messages_from_data_value};
use crate::ws_action::{
    json_i64, qq_message_list_to_json, response_message_id, response_success, ws_send_action,
};
use log::{info, warn};
use std::collections::HashMap;
use zihuan_core::error::Result;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct SendFriendMessageNode {
    id: String,
    name: String,
}

impl SendFriendMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for SendFriendMessageNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        Some("向QQ好友发送消息")
    }

    node_input![
        port! { name = "ims_bot_adapter", ty = BotAdapterRef, desc = "Bot适配器引用" },
        port! { name = "target_id", ty = String, desc = "目标好友的QQ号" },
        port! { name = "message", ty = Vec(QQMessage), desc = "要发送的QQ消息段列表" },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "是否发送成功" },
        port! { name = "message_id", ty = Integer, desc = "服务器返回的消息ID" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let adapter_ref = match inputs.get("ims_bot_adapter") {
            Some(DataValue::BotAdapterRef(handle)) => crate::adapter::shared_from_handle(handle),
            _ => return Err("ims_bot_adapter input is required".into()),
        };
        let target_id = match inputs.get("target_id") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err("target_id input is required".into()),
        };
        let messages = qq_messages_from_data_value(inputs.get("message"), "message")?;
        let segment_summary = describe_message_segments(&messages);

        let params = serde_json::json!({
            "user_id": target_id,
            "message": qq_message_list_to_json(&messages),
        });
        info!(
            "[SendFriendMessageNode] Sending private message to {} with {}",
            target_id, segment_summary
        );
        let response = ws_send_action(&adapter_ref, "send_private_msg", params)?;

        let success = response_success(&response);
        let message_id = response_message_id(&response).unwrap_or(-1);
        let retcode = json_i64(response.get("retcode"));
        let status = response.get("status").and_then(|value| value.as_str());
        let wording = response.get("wording").and_then(|value| value.as_str());

        if success {
            info!(
                "[SendFriendMessageNode] Sent private message to {} (message_id={}, retcode={:?}, status={:?}, {})",
                target_id,
                message_id,
                retcode,
                status,
                segment_summary
            );
        } else {
            warn!(
                "[SendFriendMessageNode] Failed to send private message to {} (retcode={:?}, status={:?}, wording={:?}, {}, response={})",
                target_id,
                retcode,
                status,
                wording,
                segment_summary,
                response
            );
        }

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert("message_id".to_string(), DataValue::Integer(message_id));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
