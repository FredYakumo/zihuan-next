use crate::send_qq_message_batches::{describe_message_segments, qq_messages_from_data_value};
use crate::ws_action::{
    json_i64, qq_message_list_to_send_json, response_message_id, response_success, ws_send_action,
};
use log::{info, warn};
use std::collections::HashMap;
use zihuan_core::error::Result;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct SendMessageNode {
    id: String,
    name: String,
}

impl SendMessageNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for SendMessageNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("根据 Sender 向 QQ 好友或群组发送消息")
    }

    node_input![
        port! { name = "ims_bot_adapter", ty = BotAdapterRef, desc = "Bot适配器引用" },
        port! { name = "sender", ty = Sender, desc = "消息目标 Sender" },
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
        let sender = match inputs.get("sender") {
            Some(DataValue::Sender(sender)) => sender,
            _ => return Err("sender input is required".into()),
        };
        let messages = qq_messages_from_data_value(inputs.get("message"), "message")?;
        let segment_summary = describe_message_segments(&messages);

        let (action_name, target_id, params, target_label) = match sender {
            crate::models::sender_model::Sender::Friend(friend) => {
                let target_id = friend.user_id.to_string();
                (
                    "send_private_msg",
                    target_id.clone(),
                    serde_json::json!({
                        "user_id": target_id,
                        "message": qq_message_list_to_send_json(&adapter_ref, &messages)?,
                    }),
                    "private",
                )
            }
            crate::models::sender_model::Sender::Group(group) => {
                let target_id = group.group_id.to_string();
                (
                    "send_group_msg",
                    target_id.clone(),
                    serde_json::json!({
                        "group_id": target_id,
                        "message": qq_message_list_to_send_json(&adapter_ref, &messages)?,
                    }),
                    "group",
                )
            }
        };

        info!(
            "[SendMessageNode] Sending {target_label} message to {target_id} with {segment_summary}"
        );
        let response = ws_send_action(&adapter_ref, action_name, params)?;

        let success = response_success(&response);
        let message_id = response_message_id(&response).unwrap_or(-1);
        let retcode = json_i64(response.get("retcode"));
        let status = response.get("status").and_then(|value| value.as_str());
        let wording = response.get("wording").and_then(|value| value.as_str());

        if success {
            info!(
                "[SendMessageNode] Sent {target_label} message to {target_id} (message_id={message_id}, retcode={retcode:?}, status={status:?}, {segment_summary})"
            );
        } else {
            warn!(
                "[SendMessageNode] Failed to send {target_label} message to {target_id} (retcode={retcode:?}, status={status:?}, wording={wording:?}, {segment_summary}, response={response})"
            );
        }

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert("message_id".to_string(), DataValue::Integer(message_id));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
