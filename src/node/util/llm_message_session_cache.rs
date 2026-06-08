use crate::error::Result;
use zihuan_core::llm::LLMMessage;
use crate::node::data_value::LLMMessageSessionCacheRef;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;

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

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let cache_ref: Arc<LLMMessageSessionCacheRef> = inputs
            .get("cache_ref")
            .and_then(|value| match value {
                DataValue::LLMMessageSessionCacheRef(cache_ref) => Some(cache_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                crate::error::Error::InvalidNodeInput("cache_ref is required".to_string())
            })?;

        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                crate::error::Error::InvalidNodeInput("sender_id is required".to_string())
            })?;

        let messages: Vec<LLMMessage> = match inputs.get("messages") {
            Some(DataValue::Vec(inner_type, items)) if **inner_type == DataType::LLMMessage => {
                items
                    .iter()
                    .map(|item| match item {
                        DataValue::LLMMessage(message) => Ok(message.clone()),
                        _ => Err(crate::error::Error::InvalidNodeInput(
                            "messages must contain LLMMessage items".to_string(),
                        )),
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            _ => {
                return Err(crate::error::Error::InvalidNodeInput(
                    "messages is required".to_string(),
                ))
            }
        };

        info!(
            "[LLMMessageSessionCacheNode] Appending {} message(s) for sender {}",
            messages.len(),
            sender_id
        );

        let append_messages = async move { cache_ref.append_messages(&sender_id, messages).await };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(append_messages))
        } else {
            tokio::runtime::Runtime::new()?.block_on(append_messages)
        }?;

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(true));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

