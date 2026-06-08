use chrono::Utc;
use storage_handler::MessageRecord;
use zihuan_core::llm::{LLMMessage, MessageRole};

pub fn llm_message_to_message_record(
    message_id: impl Into<String>,
    sender_id: impl Into<String>,
    sender_name: impl Into<String>,
    group_id: Option<String>,
    group_name: Option<String>,
    message: &LLMMessage,
) -> MessageRecord {
    MessageRecord {
        message_id: message_id.into(),
        sender_id: sender_id.into(),
        sender_name: sender_name.into(),
        send_time: Utc::now().naive_utc(),
        group_id,
        group_name,
        content: message.content_text_owned().unwrap_or_default(),
        at_target_list: None,
        media_json: None,
        raw_message_json: serde_json::to_string(message).ok(),
    }
}

pub fn message_record_to_llm_message(record: &MessageRecord) -> LLMMessage {
    if let Some(raw_message_json) = record.raw_message_json.as_deref() {
        if let Ok(message) = serde_json::from_str::<LLMMessage>(raw_message_json) {
            return message;
        }
    }

    LLMMessage {
        role: MessageRole::User,
        parts: zihuan_core::llm::LLMMessage::user(record.content.clone()).parts,
        reasoning_content: None,
        tool_calls: Vec::new(),
        tool_call_id: None,
        usage: None,
    }
}
