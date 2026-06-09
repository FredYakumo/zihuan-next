use crate::data_value::LLMMessageSessionCacheRef;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::info;
use std::sync::Arc;
use zihuan_core::error::Result;
use zihuan_core::llm::LLMMessage;

pub struct LLMMessageSessionCacheNode {
    id: String,
    name: String,
}

impl LLMMessageSessionCacheNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for LLMMessageSessionCacheNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("根据缓存 Ref、sender_id 与消息列表，向当前运行期会话历史追加 Vec<LLMMessage>")
    }

    node_input![
        port! { name = "cache_ref", ty = LLMMessageSessionCacheRef, desc = "LLMMessage 会话暂存器输出的缓存引用" },
        port! { name = "sender_id", ty = String, desc = "用户唯一标识，用于区分不同会话" },
        port! { name = "messages", ty = Vec(LLMMessage), desc = "要暂存并追加到会话缓存中的 Vec<LLMMessage>" },
    ];

    node_output![port! { name = "success", ty = Boolean, desc = "是否成功写入 Redis 或内存缓存" },];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let cache_ref: Arc<LLMMessageSessionCacheRef> = inputs
            .get("cache_ref")
            .and_then(|value| match value {
                DataValue::LLMMessageSessionCacheRef(cache_ref) => Some(cache_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| zihuan_core::error::Error::InvalidNodeInput("cache_ref is required".to_string()))?;

        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| zihuan_core::error::Error::InvalidNodeInput("sender_id is required".to_string()))?;

        let messages: Vec<LLMMessage> = match inputs.get("messages") {
            Some(DataValue::Vec(inner_type, items)) if **inner_type == DataType::LLMMessage => items
                .iter()
                .map(|item| match item {
                    DataValue::LLMMessage(message) => Ok(message.clone()),
                    _ => Err(zihuan_core::error::Error::InvalidNodeInput(
                        "messages must contain LLMMessage items".to_string(),
                    )),
                })
                .collect::<Result<Vec<_>>>()?,
            _ => return Err(zihuan_core::error::Error::InvalidNodeInput("messages is required".to_string())),
        };

        info!(
            "[LLMMessageSessionCacheNode] Appending {} message(s) for sender {}",
            messages.len(),
            sender_id
        );

        cache_ref.append_messages_blocking(&sender_id, messages)?;

        crate::return_with_node_output![self;
            "success" => DataValue::Boolean(true),
        ]
    }
}
