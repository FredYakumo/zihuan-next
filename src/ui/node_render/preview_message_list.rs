use crate::node::graph_io::NodeGraphDefinition;
use crate::node::DataValue;
use crate::llm::{Message, MessageRole};
use super::{NodeRenderer, InlinePortValue};
use std::collections::HashMap;

pub struct PreviewMessageListRenderer;

impl NodeRenderer for PreviewMessageListRenderer {
    fn get_preview_text(
        node_id: &str,
        graph: &NodeGraphDefinition,
        _inline_inputs: &HashMap<String, InlinePortValue>,
    ) -> String {
        // Get messages from execution results
        if let Some(results) = graph.execution_results.get(node_id) {
            if let Some(DataValue::MessageList(messages)) = results.get("messages") {
                return format_message_list(messages);
            }
        }

        String::new()
    }
    
    fn handles_node_type(node_type: &str) -> bool {
        node_type == "preview_message_list"
    }
}

/// Format a list of messages as a preview text
fn format_message_list(messages: &[Message]) -> String {
    messages.iter()
        .map(|msg| {
            let role_str = match msg.role {
                MessageRole::System => "System",
                MessageRole::User => "User",
                MessageRole::Assistant => "Assistant",
                MessageRole::Tool => "Tool",
            };
            
            let content = msg.content.as_deref().unwrap_or("");
            
            format!("[{}] {}", role_str, content)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Get structured message data for UI rendering
pub fn get_message_list_data(
    node_id: &str,
    graph: &NodeGraphDefinition,
) -> Vec<MessageItem> {
    if let Some(results) = graph.execution_results.get(node_id) {
        if let Some(DataValue::MessageList(messages)) = results.get("messages") {
            return messages.iter().map(|msg| {
                let role_str = match msg.role {
                    MessageRole::System => "system",
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                };
                
                MessageItem {
                    role: role_str.to_string(),
                    content: msg.content.clone().unwrap_or_default(),
                }
            }).collect();
        }
    }
    Vec::new()
}

#[derive(Debug, Clone)]
pub struct MessageItem {
    pub role: String,
    pub content: String,
}
