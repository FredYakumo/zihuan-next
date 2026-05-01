use crate::adapter::BotAdapter;
use crate::models::message::MessageProp;
use crate::models::MessageEvent;
use std::collections::HashMap;
use tokio::task::block_in_place;
use zihuan_core::error::Result;
use zihuan_llm_types::{OpenAIMessage, SystemMessage};
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, Port};

/// Node that converts a MessageEvent to an LLM prompt message list
///
/// Inputs:
///   - message_event: MessageEvent containing message data
///   - bot_adapter: BotAdapterRef for building context-aware system message
///
/// Outputs:
///   - messages: Vec<OpenAIMessage>: One user message
pub struct ExtractMessageFromEventNode {
    id: String,
    name: String,
}

impl ExtractMessageFromEventNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ExtractMessageFromEventNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Converts MessageEvent to LLM prompt string")
    }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "MessageEvent containing message data" },
        port! { name = "bot_adapter", ty = BotAdapterRef, desc = "BotAdapter reference for context-aware system message", required = true }
    ];

    node_output![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "Vec<OpenAIMessage> containing system and user messages" },
        port! { name = "content", ty = String, desc = "Merged readable message body" },
        port! { name = "ref_content", ty = String, desc = "Referenced/replied message content" },
        port! { name = "is_at_me", ty = Boolean, desc = "Whether the message @'s the bot" },
        port! { name = "at_target_list", ty = Vec(String), desc = "List of all @ targets in the message" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::MessageEvent(event)) = inputs.get("message_event") {
            let bot_adapter_ref = inputs
                .get("bot_adapter")
                .and_then(|v| {
                    if let DataValue::BotAdapterRef(handle) = v {
                        Some(crate::adapter::shared_from_handle(handle))
                    } else {
                        None
                    }
                })
                .ok_or("bot_adapter input is required")?;

            // This node still has a sync execute() API, so if we're already on a Tokio
            // worker thread we must move the blocking lock into block_in_place.
            let bot_id = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| {
                    let adapter = bot_adapter_ref.blocking_lock();
                    adapter.get_bot_id().to_string()
                })
            } else {
                let adapter = bot_adapter_ref.blocking_lock();
                adapter.get_bot_id().to_string()
            };

            let msg_prop = MessageProp::from_messages(&event.message_list, Some(&bot_id));

            // Build user message from incoming MessageEvent
            let mut user_text = msg_prop.content.clone().unwrap_or_default();
            if let Some(ref_cnt) = msg_prop.ref_content.as_deref() {
                if !ref_cnt.is_empty() {
                    if !user_text.is_empty() {
                        user_text.push_str("\n\n");
                    }
                    user_text.push_str("[引用内容]\n");
                    user_text.push_str(ref_cnt);
                }
            }
            if user_text.trim().is_empty() {
                user_text = "(无文本内容，可能是仅@或回复)".to_string();
            }

            let user_msg = OpenAIMessage::user(user_text);

            let messages = vec![user_msg];
            outputs.insert(
                "messages".to_string(),
                DataValue::Vec(
                    Box::new(zihuan_node::DataType::OpenAIMessage),
                    messages.into_iter().map(DataValue::OpenAIMessage).collect(),
                ),
            );
            outputs.insert(
                "content".to_string(),
                DataValue::String(msg_prop.content.unwrap_or_default()),
            );
            outputs.insert(
                "ref_content".to_string(),
                DataValue::String(msg_prop.ref_content.unwrap_or_default()),
            );
            outputs.insert(
                "is_at_me".to_string(),
                DataValue::Boolean(msg_prop.is_at_me),
            );
            outputs.insert(
                "at_target_list".to_string(),
                DataValue::Vec(
                    Box::new(DataType::String),
                    msg_prop
                        .at_target_list
                        .into_iter()
                        .map(DataValue::String)
                        .collect(),
                ),
            );
        } else {
            return Err("message_event input is required and must be MessageEvent type".into());
        }
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_event_to_string_node_creation() {
        let node = ExtractMessageFromEventNode::new("msg_to_str_1", "ExtractMessageFromEvent");
        assert_eq!(node.id(), "msg_to_str_1");
        assert_eq!(node.name(), "ExtractMessageFromEvent");
    }

    #[test]
    fn test_input_output_ports() {
        let node = ExtractMessageFromEventNode::new("test", "test");
        let input_ports = node.input_ports();
        let output_ports = node.output_ports();

        assert_eq!(input_ports.len(), 2);
        assert_eq!(input_ports[0].name, "message_event");
        assert_eq!(input_ports[1].name, "bot_adapter");

        assert_eq!(output_ports.len(), 5);
        assert_eq!(output_ports[0].name, "messages");
        assert_eq!(output_ports[1].name, "content");
        assert_eq!(output_ports[2].name, "ref_content");
        assert_eq!(output_ports[3].name, "is_at_me");
        assert_eq!(output_ports[4].name, "at_target_list");
    }
}
