use zihuan_core::error::Result;
use crate::data_value::OpenAIMessageSessionCacheRef;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;

pub struct OpenAIMessageSessionCacheGetNode {
    id: String,
    name: String,
}

impl OpenAIMessageSessionCacheGetNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for OpenAIMessageSessionCacheGetNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("根据缓存 Ref 和 sender_id 读取当前运行期累计的 Vec<OpenAIMessage>")
    }

    node_input![
        port! { name = "cache_ref", ty = OpenAIMessageSessionCacheRef, desc = "OpenAIMessage 会话暂存器输出的缓存引用" },
        port! { name = "sender_id", ty = String, desc = "要读取历史消息的 sender_id" },
        port! { name = "fallback", ty = Vec(OpenAIMessage), desc = "可选：未读取到 sender_id 历史消息时输出的回退消息列表", optional },
    ];

    node_output![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "读取到的历史 Vec<OpenAIMessage>" },
    ];

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

        let fallback_messages = inputs
            .get("fallback")
            .and_then(|value| match value {
                DataValue::Vec(inner_type, items) if **inner_type == DataType::OpenAIMessage => {
                    Some(
                        items
                            .iter()
                            .map(|item| match item {
                                DataValue::OpenAIMessage(message) => Ok(message.clone()),
                                _ => Err(zihuan_core::error::Error::InvalidNodeInput(
                                    "fallback must contain OpenAIMessage items".to_string(),
                                )),
                            })
                            .collect::<Result<Vec<_>>>(),
                    )
                }
                _ => None,
            })
            .transpose()?
            .unwrap_or_default();

        let read_messages = async move { cache_ref.get_messages(&sender_id).await };

        let cached_messages = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(read_messages))
        } else {
            tokio::runtime::Runtime::new()?.block_on(read_messages)
        }?;

        let messages = if cached_messages.is_empty() {
            fallback_messages
        } else {
            cached_messages
        };

        let mut outputs = HashMap::new();
        outputs.insert(
            "messages".to_string(),
            DataValue::Vec(
                Box::new(DataType::OpenAIMessage),
                messages.into_iter().map(DataValue::OpenAIMessage).collect(),
            ),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::OpenAIMessageSessionCacheGetNode;
    use zihuan_core::error::Result;
    use zihuan_llm_types::{MessageRole, OpenAIMessage};
    use crate::util::{OpenAIMessageSessionCacheNode, OpenAIMessageSessionCacheProviderNode};
    use crate::{DataType, DataValue, Node};
    use std::collections::HashMap;

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

    fn cache_input(
        cache_ref: DataValue,
        sender_id: &str,
        messages: Vec<OpenAIMessage>,
    ) -> HashMap<String, DataValue> {
        HashMap::from([
            ("cache_ref".to_string(), cache_ref),
            (
                "messages".to_string(),
                DataValue::Vec(
                    Box::new(DataType::OpenAIMessage),
                    messages.into_iter().map(DataValue::OpenAIMessage).collect(),
                ),
            ),
            (
                "sender_id".to_string(),
                DataValue::String(sender_id.to_string()),
            ),
        ])
    }

    fn extract_message_contents(outputs: &HashMap<String, DataValue>) -> Vec<String> {
        match outputs.get("messages") {
            Some(DataValue::Vec(_, items)) => items
                .iter()
                .filter_map(|item| match item {
                    DataValue::OpenAIMessage(message) => message.content.clone(),
                    _ => None,
                })
                .collect(),
            other => panic!("unexpected messages output: {:?}", other),
        }
    }

    #[test]
    fn reads_history_by_sender_from_cache_ref() -> Result<()> {
        let mut provider_node = OpenAIMessageSessionCacheProviderNode::new("provider", "Provider");
        let mut cache_node = OpenAIMessageSessionCacheNode::new("cache", "Cache");
        let mut get_node = OpenAIMessageSessionCacheGetNode::new("getter", "Getter");

        let provider_outputs = provider_node.execute(provider_input())?;
        let cache_ref = provider_outputs
            .get("cache_ref")
            .cloned()
            .expect("cache_ref output should exist");

        let _ = cache_node.execute(cache_input(
            cache_ref.clone(),
            "user-1",
            vec![message(MessageRole::User, "第一条")],
        ))?;
        let cache_outputs = cache_node.execute(cache_input(
            cache_ref.clone(),
            "user-1",
            vec![message(MessageRole::Assistant, "第二条")],
        ))?;

        assert!(matches!(
            cache_outputs.get("success"),
            Some(DataValue::Boolean(true))
        ));

        let outputs = get_node.execute(HashMap::from([
            ("cache_ref".to_string(), cache_ref),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
        ]))?;

        assert_eq!(extract_message_contents(&outputs), vec!["第一条", "第二条"]);
        Ok(())
    }

    #[test]
    fn returns_fallback_when_sender_history_is_missing() -> Result<()> {
        let mut provider_node = OpenAIMessageSessionCacheProviderNode::new("provider", "Provider");
        let mut cache_node = OpenAIMessageSessionCacheNode::new("cache", "Cache");
        let mut get_node = OpenAIMessageSessionCacheGetNode::new("getter", "Getter");

        let provider_outputs = provider_node.execute(provider_input())?;
        let cache_ref = provider_outputs
            .get("cache_ref")
            .cloned()
            .expect("cache_ref output should exist");

        let cache_outputs = cache_node.execute(cache_input(
            cache_ref.clone(),
            "user-1",
            vec![message(MessageRole::User, "第一条")],
        ))?;

        assert!(matches!(
            cache_outputs.get("success"),
            Some(DataValue::Boolean(true))
        ));

        let outputs = get_node.execute(HashMap::from([
            ("cache_ref".to_string(), cache_ref),
            (
                "sender_id".to_string(),
                DataValue::String("user-2".to_string()),
            ),
            (
                "fallback".to_string(),
                DataValue::Vec(
                    Box::new(DataType::OpenAIMessage),
                    vec![DataValue::OpenAIMessage(message(
                        MessageRole::System,
                        "fallback",
                    ))],
                ),
            ),
        ]))?;

        assert_eq!(extract_message_contents(&outputs), vec!["fallback"]);
        Ok(())
    }

    #[test]
    fn returns_empty_when_sender_history_is_missing_and_no_fallback() -> Result<()> {
        let mut provider_node = OpenAIMessageSessionCacheProviderNode::new("provider", "Provider");
        let mut cache_node = OpenAIMessageSessionCacheNode::new("cache", "Cache");
        let mut get_node = OpenAIMessageSessionCacheGetNode::new("getter", "Getter");

        let provider_outputs = provider_node.execute(provider_input())?;
        let cache_ref = provider_outputs
            .get("cache_ref")
            .cloned()
            .expect("cache_ref output should exist");

        let cache_outputs = cache_node.execute(cache_input(
            cache_ref.clone(),
            "user-1",
            vec![message(MessageRole::User, "第一条")],
        ))?;

        assert!(matches!(
            cache_outputs.get("success"),
            Some(DataValue::Boolean(true))
        ));

        let outputs = get_node.execute(HashMap::from([
            ("cache_ref".to_string(), cache_ref),
            (
                "sender_id".to_string(),
                DataValue::String("user-2".to_string()),
            ),
        ]))?;

        assert!(extract_message_contents(&outputs).is_empty());
        Ok(())
    }
}
