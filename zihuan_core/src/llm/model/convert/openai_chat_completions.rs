use serde_json::{json, Value};

use super::common::{build_chat_multimodal_parts, role_json, with_reasoning, with_tool_fields};
use super::super::llm_message::LLMMessage;

pub(crate) fn convert(message: &LLMMessage, include_reasoning_content: bool) -> Vec<Value> {
    let content = if message.parts.is_empty() {
        Value::String(String::new())
    } else if message.has_only_text_parts() {
        Value::String(message.text_parts_joined())
    } else {
        build_chat_multimodal_parts(&message.parts)
    };

    let msg_obj = json!({
        "role": role_json(message),
        "content": content,
    });

    vec![with_tool_fields(with_reasoning(msg_obj, message, include_reasoning_content), message)]
}
