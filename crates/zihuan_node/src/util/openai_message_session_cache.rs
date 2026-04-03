use zihuan_core::error::Result;
use zihuan_llm_types::OpenAIMessage;
use crate::data_value::OpenAIMessageSessionCacheRef;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;

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

#[cfg(test)]
mod tests {
    use super::OpenAIMessageSessionCacheNode;
    use zihuan_core::error::Result;
    use zihuan_llm_types::{MessageRole, OpenAIMessage};
    use crate::data_value::OpenAIMessageSessionCacheRef;
    use crate::util::OpenAIMessageSessionCacheProviderNode;
    use crate::{DataType, DataValue, Node};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn message(role: MessageRole, content: &str) -> OpenAIMessage {
        OpenAIMessage {
            role,
            content: Some(content.to_string()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    fn provider_input() -> HashMap<String, DataValue> {
        HashMap::new()
    }

    fn input(
        cache_ref: Arc<OpenAIMessageSessionCacheRef>,
        sender_id: &str,
        messages: Vec<OpenAIMessage>,
    ) -> HashMap<String, DataValue> {
        HashMap::from([
            (
                "cache_ref".to_string(),
                DataValue::OpenAIMessageSessionCacheRef(cache_ref),
            ),
            (
                "sender_id".to_string(),
                DataValue::String(sender_id.to_string()),
            ),
            (
                "messages".to_string(),
                DataValue::Vec(
                    Box::new(DataType::OpenAIMessage),
                    messages.into_iter().map(DataValue::OpenAIMessage).collect(),
                ),
            ),
        ])
    }

    fn extract_cache_ref(
        outputs: &HashMap<String, DataValue>,
    ) -> Arc<OpenAIMessageSessionCacheRef> {
        match outputs.get("cache_ref") {
            Some(DataValue::OpenAIMessageSessionCacheRef(cache_ref)) => cache_ref.clone(),
            other => panic!("unexpected cache_ref output: {:?}", other),
        }
    }

    fn load_contents(
        cache_ref: &Arc<OpenAIMessageSessionCacheRef>,
        sender_id: &str,
    ) -> Result<Vec<String>> {
        let runtime = tokio::runtime::Runtime::new()?;
        let messages = runtime.block_on(cache_ref.get_messages(sender_id))?;
        Ok(messages
            .into_iter()
            .filter_map(|message| message.content)
            .collect())
    }

    #[test]
    fn accumulates_messages_for_same_sender_within_one_run() -> Result<()> {
        let mut provider =
            OpenAIMessageSessionCacheProviderNode::new("cache_provider", "Cache Provider");
        let mut node = OpenAIMessageSessionCacheNode::new("cache", "Cache");

        let provider_outputs = provider.execute(provider_input())?;
        let cache_ref = extract_cache_ref(&provider_outputs);

        let first_outputs = node.execute(input(
            cache_ref.clone(),
            "user-1",
            vec![message(MessageRole::User, "你好")],
        ))?;
        assert!(matches!(
            first_outputs.get("success"),
            Some(DataValue::Boolean(true))
        ));
        assert_eq!(load_contents(&cache_ref, "user-1")?, vec!["你好"]);

        let second_outputs = node.execute(input(
            cache_ref.clone(),
            "user-1",
            vec![message(MessageRole::Assistant, "你好呀")],
        ))?;
        assert!(matches!(
            second_outputs.get("success"),
            Some(DataValue::Boolean(true))
        ));
        assert_eq!(load_contents(&cache_ref, "user-1")?, vec!["你好", "你好呀"]);

        let third_outputs = node.execute(input(
            cache_ref.clone(),
            "user-2",
            vec![message(MessageRole::User, "另一位用户")],
        ))?;
        assert!(matches!(
            third_outputs.get("success"),
            Some(DataValue::Boolean(true))
        ));
        assert_eq!(load_contents(&cache_ref, "user-2")?, vec!["另一位用户"]);

        Ok(())
    }

    #[test]
    fn clears_history_when_graph_restarts() -> Result<()> {
        let mut provider =
            OpenAIMessageSessionCacheProviderNode::new("cache_provider", "Cache Provider");
        let mut node = OpenAIMessageSessionCacheNode::new("cache", "Cache");

        let provider_outputs = provider.execute(provider_input())?;
        let cache_ref = extract_cache_ref(&provider_outputs);

        let _ = node.execute(input(
            cache_ref.clone(),
            "user-1",
            vec![message(MessageRole::User, "第一条")],
        ))?;
        let before_reset = node.execute(input(
            cache_ref.clone(),
            "user-1",
            vec![message(MessageRole::Assistant, "第二条")],
        ))?;
        assert!(matches!(
            before_reset.get("success"),
            Some(DataValue::Boolean(true))
        ));
        assert_eq!(
            load_contents(&cache_ref, "user-1")?,
            vec!["第一条", "第二条"]
        );

        provider.on_graph_start()?;

        let refreshed_outputs = provider.execute(provider_input())?;
        let refreshed_cache_ref = extract_cache_ref(&refreshed_outputs);

        let after_reset = node.execute(input(
            refreshed_cache_ref.clone(),
            "user-1",
            vec![message(MessageRole::User, "重启后")],
        ))?;
        assert!(matches!(
            after_reset.get("success"),
            Some(DataValue::Boolean(true))
        ));
        assert_eq!(
            load_contents(&refreshed_cache_ref, "user-1")?,
            vec!["重启后"]
        );

        Ok(())
    }
}
