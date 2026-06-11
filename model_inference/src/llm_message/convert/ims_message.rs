use zihuan_core::error::Result;
use zihuan_core::ims_bot_adapter::models::event_model::MessageEvent;
use zihuan_core::ims_bot_adapter::models::message::{Message, MessageProp};
use zihuan_core::llm::{LLMMessage, MessagePart};
use zihuan_graph_engine::object_storage::S3Ref;

pub fn qq_messages_to_llm_message(messages: &[Message], bot_id: &str, _s3_ref: Option<&S3Ref>) -> LLMMessage {
    let msg_prop = MessageProp::from_messages(messages, Some(bot_id));
    let mut parts = Vec::new();

    if let Some(content) = msg_prop.content.filter(|text| !text.trim().is_empty()) {
        parts.push(MessagePart::text(content));
    }
    if let Some(ref_content) = msg_prop.ref_content.filter(|text| !text.trim().is_empty()) {
        parts.push(MessagePart::text(format!("[reply]\n{ref_content}")));
    }

    if parts.is_empty() {
        LLMMessage::user("(无可用文本内容)")
    } else {
        LLMMessage::user_with_parts(parts)
    }
}

pub fn event_to_llm_message(event: &MessageEvent, bot_id: &str, s3_ref: Option<&S3Ref>) -> Result<LLMMessage> {
    Ok(qq_messages_to_llm_message(&event.message_list, bot_id, s3_ref))
}
