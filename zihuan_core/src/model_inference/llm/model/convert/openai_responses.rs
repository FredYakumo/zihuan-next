use serde_json::{json, Value};

use super::super::llm_message::LLMMessage;
use super::super::message_role::MessageRole;
use super::common::{build_responses_content_items, role_json};

pub(crate) fn convert(message: &LLMMessage) -> Vec<Value> {
    match message.role {
        MessageRole::Tool => vec![json!({
            "type": "function_call_output",
            "call_id": message.tool_call_id.clone().unwrap_or_default(),
            "output": message.content_text_owned().unwrap_or_default(),
        })],
        MessageRole::Assistant if !message.tool_calls.is_empty() => {
            let mut items = Vec::new();
            let content_items = build_responses_content_items(message, false);
            if !content_items.is_empty() {
                items.push(json!({
                    "type": "message",
                    "role": "assistant",
                    "content": content_items,
                }));
            }
            for tool_call in &message.tool_calls {
                items.push(json!({
                    "type": "function_call",
                    "call_id": tool_call.id,
                    "name": tool_call.function.name,
                    "arguments": tool_call.function.arguments.to_string(),
                }));
            }
            items
        }
        _ => vec![json!({
            "type": "message",
            "role": role_json(message),
            "content": build_responses_content_items(message, false),
        })],
    }
}
