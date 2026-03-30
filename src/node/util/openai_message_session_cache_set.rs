use crate::error::Result;
use crate::llm::OpenAIMessage;
use crate::node::data_value::OpenAIMessageSessionCacheRef;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;

pub struct OpenAIMessageSessionCacheSetNode {
    id: String,
    name: String,
}

impl OpenAIMessageSessionCacheSetNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for OpenAIMessageSessionCacheSetNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("根据缓存 Ref、sender_id 与消息列表，覆写当前运行期累计的 Vec<OpenAIMessage>")
    }

    node_input![
        port! { name = "cache_ref", ty = OpenAIMessageSessionCacheRef, desc = "OpenAIMessage 会话暂存器输出的缓存引用" },
        port! { name = "sender_id", ty = String, desc = "要覆写历史消息的 sender_id" },
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "要写回并覆写到缓存中的 Vec<OpenAIMessage>" },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "是否成功覆写历史消息" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let cache_ref: Arc<OpenAIMessageSessionCacheRef> = inputs
            .get("cache_ref")
            .and_then(|value| match value {
                DataValue::OpenAIMessageSessionCacheRef(cache_ref) => Some(cache_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("cache_ref is required".to_string()))?;

        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("sender_id is required".to_string()))?;

        let messages: Vec<OpenAIMessage> = match inputs.get("messages") {
            Some(DataValue::Vec(inner_type, items)) if **inner_type == DataType::OpenAIMessage => items
                .iter()
                .map(|item| match item {
                    DataValue::OpenAIMessage(message) => Ok(message.clone()),
                    _ => Err(crate::error::Error::InvalidNodeInput(
                        "messages must contain OpenAIMessage items".to_string(),
                    )),
                })
                .collect::<Result<Vec<_>>>()?,
            _ => {
                return Err(crate::error::Error::InvalidNodeInput(
                    "messages is required".to_string(),
                ))
            }
        };

        let write_messages = async move { cache_ref.set_messages(&sender_id, messages).await };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(write_messages))
        } else {
            tokio::runtime::Runtime::new()?.block_on(write_messages)
        }?;

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(true));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::OpenAIMessageSessionCacheSetNode;
    use crate::error::Result;
    use crate::llm::{MessageRole, OpenAIMessage};
    use crate::node::util::{
        OpenAIMessageSessionCacheGetNode, OpenAIMessageSessionCacheNode,
        OpenAIMessageSessionCacheProviderNode,
    };
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;

    fn message(role: MessageRole, content: &str) -> OpenAIMessage {
        OpenAIMessage {
            role,
            content: Some(content.to_string()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    fn vec_value(messages: Vec<OpenAIMessage>) -> DataValue {
        DataValue::Vec(
            Box::new(DataType::OpenAIMessage),
            messages.into_iter().map(DataValue::OpenAIMessage).collect(),
        )
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
    fn overwrites_history_by_sender_from_cache_ref() -> Result<()> {
        let mut provider_node = OpenAIMessageSessionCacheProviderNode::new("provider", "Provider");
        let mut cache_node = OpenAIMessageSessionCacheNode::new("cache", "Cache");
        let mut get_node = OpenAIMessageSessionCacheGetNode::new("getter", "Getter");
        let mut set_node = OpenAIMessageSessionCacheSetNode::new("setter", "Setter");

        let provider_outputs = provider_node.execute(HashMap::new())?;
        let cache_ref = provider_outputs
            .get("cache_ref")
            .cloned()
            .expect("cache_ref output should exist");

        let cache_outputs = cache_node.execute(HashMap::from([
            ("cache_ref".to_string(), cache_ref.clone()),
            ("messages".to_string(), vec_value(vec![message(MessageRole::User, "旧消息")])),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
        ]))?;

        assert!(matches!(
            cache_outputs.get("success"),
            Some(DataValue::Boolean(true))
        ));

        let set_outputs = set_node.execute(HashMap::from([
            ("cache_ref".to_string(), cache_ref.clone()),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
            (
                "messages".to_string(),
                vec_value(vec![
                    message(MessageRole::System, "系统消息"),
                    message(MessageRole::Assistant, "新回复"),
                ]),
            ),
        ]))?;

        assert!(matches!(
            set_outputs.get("success"),
            Some(DataValue::Boolean(true))
        ));

        let read_outputs = get_node.execute(HashMap::from([
            ("cache_ref".to_string(), cache_ref),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
        ]))?;

        assert_eq!(
            extract_message_contents(&read_outputs),
            vec!["系统消息", "新回复"]
        );

        Ok(())
    }
}
