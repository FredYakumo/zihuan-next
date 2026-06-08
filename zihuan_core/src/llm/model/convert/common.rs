use serde_json::{json, Value};

use crate::llm::tooling::ToolCalls;

use crate::message_part::MessagePart;

use super::super::llm_message::LLMMessage;
use super::super::message_role::MessageRole;
use crate::llm::util::role_to_str;

pub(crate) fn build_tool_calls_json(tool_calls: &[ToolCalls]) -> Value {
    json!(tool_calls
        .iter()
        .map(|tc| {
            json!({
                "id": tc.id,
                "type": tc.type_name,
                "function": {
                    "name": tc.function.name,
                    "arguments": tc.function.arguments.to_string(),
                }
            })
        })
        .collect::<Vec<_>>())
}

pub(crate) fn build_chat_multimodal_parts(parts: &[MessagePart]) -> Value {
    Value::Array(
        parts
            .iter()
            .map(|part| match part {
                MessagePart::Text { text } => json!({
                    "type": "text",
                    "text": text,
                }),
                MessagePart::Image { .. } => json!({
                    "type": "image_url",
                    "image_url": {
                        "url": part.media_locator().unwrap_or_default(),
                    }
                }),
                MessagePart::Video { .. } => json!({
                    "type": "video_url",
                    "video_url": {
                        "url": part.media_locator().unwrap_or_default(),
                    }
                }),
            })
            .collect(),
    )
}

pub(crate) fn build_responses_content_items(
    message: &LLMMessage,
    image_url_as_object: bool,
) -> Vec<Value> {
    if message.parts.is_empty() {
        return Vec::new();
    }

    message
        .parts
        .iter()
        .map(|part| match part {
            MessagePart::Text { text } => json!({
                "type": "input_text",
                "text": text,
            }),
            MessagePart::Image { .. } => {
                let locator = part.media_locator().unwrap_or_default();
                if matches!(message.role, MessageRole::Assistant) {
                    json!({
                        "type": "input_text",
                        "text": format!("[image omitted] {locator}"),
                    })
                } else if image_url_as_object {
                    json!({
                        "type": "input_image",
                        "image_url": { "url": locator },
                        "detail": "auto",
                    })
                } else {
                    json!({
                        "type": "input_image",
                        "image_url": locator,
                        "detail": "auto",
                    })
                }
            }
            MessagePart::Video { .. } => json!({
                "type": "input_text",
                "text": format!("[video omitted] {}", part.media_locator().unwrap_or_default()),
            }),
        })
        .collect()
}

pub(crate) fn with_reasoning(
    mut msg_obj: Value,
    message: &LLMMessage,
    include_reasoning_content: bool,
) -> Value {
    if include_reasoning_content {
        if let Some(reasoning_content) = &message.reasoning_content {
            msg_obj["reasoning_content"] = json!(reasoning_content);
        }
    }
    msg_obj
}

pub(crate) fn with_tool_fields(mut msg_obj: Value, message: &LLMMessage) -> Value {
    if !message.tool_calls.is_empty() {
        msg_obj["tool_calls"] = build_tool_calls_json(&message.tool_calls);
    }
    if let Some(ref id) = message.tool_call_id {
        msg_obj["tool_call_id"] = json!(id);
    }
    msg_obj
}

pub(crate) fn role_json(message: &LLMMessage) -> Value {
    json!(role_to_str(&message.role))
}
