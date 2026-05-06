use crate::send_qq_message_batches::{execute_fixed_target_batch_send, TARGET_TYPE_GROUP};
use std::collections::HashMap;
use zihuan_core::error::Result;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

const LOG_PREFIX: &str = "[SendGroupMessageBatchesNode]";

pub struct SendGroupMessageBatchesNode {
    id: String,
    name: String,
}

impl SendGroupMessageBatchesNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for SendGroupMessageBatchesNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("向QQ群组批量发送多条消息")
    }

    node_input![
        port! { name = "ims_bot_adapter", ty = BotAdapterRef, desc = "Bot适配器引用" },
        port! { name = "target_id", ty = String, desc = "目标群的群号" },
        port! { name = "message_batches", ty = Vec(Vec(QQMessage)), desc = "要发送的 QQ 消息批次列表" },
        port! { name = "delay_millis", ty = Integer, desc = "两次实际发送之间的间隔毫秒数，默认 0", optional },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "是否所有实际发送批次都成功" },
        port! { name = "summary", ty = String, desc = "批量发送汇总信息" },
        port! { name = "message_ids", ty = Vec(Integer), desc = "每个输入批次对应的 message_id；失败或跳过为空批次时为 -1" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;
        let outputs = execute_fixed_target_batch_send(&inputs, TARGET_TYPE_GROUP, LOG_PREFIX)?;
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

