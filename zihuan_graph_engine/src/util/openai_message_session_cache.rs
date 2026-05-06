use crate::data_value::OpenAIMessageSessionCacheRef;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use zihuan_core::error::Result;
use zihuan_core::llm::OpenAIMessage;

pub struct OpenAIMessageSessionCacheNode {
    id: String,
    name: String,
}

impl OpenAIMessageSessionCacheNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for OpenAIMessageSessionCacheNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("根据缓存 Ref、sender_id 与消息列表，向当前运行期会话历史追加 Vec<OpenAIMessage>")
    }

    node_input![
        port! { name = "cache_ref", ty = OpenAIMessageSessionCacheRef, desc = "OpenAIMessage 会话暂存器输出的缓存引用" },
        port! { name = "sender_id", ty = String, desc = "用户唯一标识，用于区分不同会话" },
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "要暂存并追加到会话缓存中的 Vec<OpenAIMessage>" },
    ];

    node_output![port! { name = "success", ty = Boolean, desc = "是否成功写入 Redis 或内存缓存" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let cache_ref: Arc<OpenAIMessageSessionCacheRef> = inputs
            .get("cache_ref")
            .and_then(|value| match value {
                DataValue::OpenAIMessageSessionCacheRef(cache_ref) => Some(cache_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("cache_ref is required".to_string())
            })?;

        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("sender_id is required".to_string())
            })?;

        let messages: Vec<OpenAIMessage> = match inputs.get("messages") {
            Some(DataValue::Vec(inner_type, items)) if **inner_type == DataType::OpenAIMessage => {
                items
                    .iter()
                    .map(|item| match item {
                        DataValue::OpenAIMessage(message) => Ok(message.clone()),
                        _ => Err(zihuan_core::error::Error::InvalidNodeInput(
                            "messages must contain OpenAIMessage items".to_string(),
                        )),
                    })
                    .collect::<Result<Vec<_>>>()?
            }
            _ => {
                return Err(zihuan_core::error::Error::InvalidNodeInput(
                    "messages is required".to_string(),
                ))
            }
        };

        info!(
            "[OpenAIMessageSessionCacheNode] Appending {} message(s) for sender {}",
            messages.len(),
            sender_id
        );

        cache_ref.append_messages_blocking(&sender_id, messages)?;

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(true));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
