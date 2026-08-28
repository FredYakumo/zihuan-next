use crate::ims_bot_adapter::runtime::adapter::restore_message_list_for_message_id;
use crate::ims_bot_adapter::runtime::models::message::MessageProp;
use log::{info, warn};
use serde_json::Value;
use std::sync::Arc;
use tokio::task::block_in_place;
use crate::error::{Error, Result};
use crate::ims_bot_adapter::logging::{LOG_DATA_URL_PREVIEW_CHARS, LOG_MESSAGE_PREVIEW_CHARS};
use crate::model_inference::llm::{LLMMessage, MessagePart};
use crate::graph::object_storage::S3Ref;

use crate::ims_bot_adapter::runtime::models::message::Message;
use crate::ims_bot_adapter::runtime::multimodal_image_url::{resolve_image_message_part, resolve_plain_text_segments, ResolvedTextSegment};

pub struct MessageEventExtractor;

pub(crate) struct ExtractedMessageOutputs {
    pub user_message: LLMMessage,
    pub content: String,
    pub ref_content: String,
    pub is_at_me: bool,
    pub at_target_list: Vec<String>,
}

impl MessageEventExtractor {
    const LOG_PREFIX: &str = "[MessageEventExtractor]";

    fn append_text_segment(buffer: &mut String, segment: &str) {
        let segment = segment.trim();
        if segment.is_empty() {
            return;
        }

        if !buffer.is_empty() {
            buffer.push(' ');
        }
        buffer.push_str(segment);
    }

    fn flush_text_part(parts: &mut Vec<MessagePart>, buffer: &mut String) {
        let text = buffer.trim();
        if !text.is_empty() {
            parts.push(MessagePart::text(text.to_string()));
        }
        buffer.clear();
    }

    fn truncate_for_log(text: &str, max_chars: usize) -> String {
        let total_chars = text.chars().count();
        if total_chars <= max_chars {
            return text.to_string();
        }

        let truncated: String = text.chars().take(max_chars).collect();
        format!("{truncated}...(truncated,total_chars={total_chars})")
    }

    fn truncate_data_url_for_log(url: &str) -> String {
        const BASE64_MARKER: &str = ";base64,";

        let Some(marker_index) = url.find(BASE64_MARKER) else {
            return Self::truncate_for_log(url, LOG_DATA_URL_PREVIEW_CHARS);
        };

        let payload_start = marker_index + BASE64_MARKER.len();
        let payload = &url[payload_start..];
        let payload_chars = payload.chars().count();
        if payload_chars <= LOG_DATA_URL_PREVIEW_CHARS {
            return url.to_string();
        }

        let prefix = &url[..payload_start];
        let payload_preview: String = payload.chars().take(LOG_DATA_URL_PREVIEW_CHARS).collect();
        format!("{prefix}{payload_preview}...(truncated,base64_chars={payload_chars})")
    }

    fn sanitize_json_for_log(value: &mut Value) {
        match value {
            Value::String(text) => {
                if text.starts_with("data:") {
                    *text = Self::truncate_data_url_for_log(text);
                }
            }
            Value::Array(items) => {
                for item in items {
                    Self::sanitize_json_for_log(item);
                }
            }
            Value::Object(map) => {
                for value in map.values_mut() {
                    Self::sanitize_json_for_log(value);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn json_for_log<T: serde::Serialize>(value: &T) -> String {
        let mut json_value = match serde_json::to_value(value) {
            Ok(value) => value,
            Err(err) => return format!("<serialize failed: {err}>"),
        };
        Self::sanitize_json_for_log(&mut json_value);

        match serde_json::to_string(&json_value) {
            Ok(json) => Self::truncate_for_log(&json, LOG_MESSAGE_PREVIEW_CHARS),
            Err(err) => format!("<serialize failed: {err}>"),
        }
    }

    fn append_plain_text_as_parts(
        text: &str,
        parts: &mut Vec<MessagePart>,
        text_buffer: &mut String,
        has_media: &mut bool,
        s3_ref: Option<&S3Ref>,
    ) {
        for segment in resolve_plain_text_segments(text, s3_ref, false, Self::LOG_PREFIX) {
            match segment {
                ResolvedTextSegment::Text(text) => Self::append_text_segment(text_buffer, &text),
                ResolvedTextSegment::Image(resolved) => {
                    Self::flush_text_part(parts, text_buffer);
                    parts.push(resolved.part);
                    *has_media = true;
                }
            }
        }
    }

    /// Purpose: Recursively process a list of messages, <br />
    /// appending text segments to the text buffer and flushing to parts when media is encountered.
    fn append_messages_as_parts(
        messages: &[Message],
        parts: &mut Vec<MessagePart>,
        text_buffer: &mut String,
        has_media: &mut bool,
        include_reply_source_block: bool,
        s3_ref: Option<&S3Ref>,
    ) {
        for message in messages {
            match message {
                Message::PlainText(plain) => {
                    Self::append_plain_text_as_parts(&plain.text, parts, text_buffer, has_media, s3_ref);
                }
                Message::Image(image) => {
                    if let Some(resolved) = resolve_image_message_part(image, s3_ref, false, Self::LOG_PREFIX) {
                        Self::flush_text_part(parts, text_buffer);
                        parts.push(resolved.part);
                        *has_media = true;
                    } else {
                        Self::append_text_segment(text_buffer, &image.to_string());
                    }
                }
                Message::Reply(reply) => {
                    Self::append_text_segment(text_buffer, &reply.to_string());

                    if include_reply_source_block {
                        if let Some(source_messages) = reply.message_source.as_deref() {
                            if !text_buffer.is_empty() {
                                text_buffer.push_str("\n\n");
                            }
                            text_buffer.push_str(&format!("[{}]\n", crate::ims_bot_adapter::runtime::REPLAY_CONTENT_LABEL));
                            Self::append_messages_as_parts(
                                source_messages,
                                parts,
                                text_buffer,
                                has_media,
                                false,
                                s3_ref,
                            );
                        }
                    }
                }
                Message::Forward(forward) => {
                    if forward.content.is_empty() {
                        Self::append_text_segment(text_buffer, &forward.to_string());
                    } else {
                        if !text_buffer.is_empty() {
                            text_buffer.push_str("\n\n");
                        }
                        text_buffer.push_str(&format!("[{}]\n", crate::ims_bot_adapter::runtime::FORWARD_CONTENT_LABEL));
                        for (index, node) in forward.content.iter().enumerate() {
                            if index > 0 && !text_buffer.ends_with('\n') {
                                text_buffer.push('\n');
                            }
                            let sender = node.nickname.as_deref().or(node.user_id.as_deref()).unwrap_or("unknown");
                            text_buffer.push_str(sender);
                            text_buffer.push_str(": ");
                            Self::append_messages_as_parts(&node.content, parts, text_buffer, has_media, false, s3_ref);
                            if !text_buffer.ends_with('\n') {
                                text_buffer.push('\n');
                            }
                        }
                    }
                }
                other => {
                    Self::append_text_segment(text_buffer, &other.to_string());
                }
            }
        }
    }

    fn build_user_message(messages: &[Message], msg_prop: &MessageProp, s3_ref: Option<&S3Ref>) -> LLMMessage {
        let mut parts = Vec::new();
        let mut text_buffer = String::new();
        let mut has_media = false;

        Self::append_messages_as_parts(messages, &mut parts, &mut text_buffer, &mut has_media, true, s3_ref);

        if let Some(ref_cnt) = msg_prop.ref_content.as_deref().filter(|value| !value.is_empty()) {
            if text_buffer.contains(crate::ims_bot_adapter::runtime::REPLAY_CONTENT_LABEL) {
                if !text_buffer.is_empty() {
                    text_buffer.push_str("\n\n");
                }
                text_buffer.push_str(&format!("[{}]\n", crate::ims_bot_adapter::runtime::QUOTE_CONTENT_APPENDIX_LABEL));
                text_buffer.push_str(ref_cnt);
            } else {
                if !text_buffer.is_empty() {
                    text_buffer.push_str("\n\n");
                }
                text_buffer.push_str(&format!("[{}]\n", crate::ims_bot_adapter::runtime::REPLAY_CONTENT_LABEL));
                text_buffer.push_str(ref_cnt);
            }
        }

        Self::flush_text_part(&mut parts, &mut text_buffer);

        if has_media {
            if parts.is_empty() {
                warn!(
                    "{} build_user_message detected media but produced no parts; falling back to text",
                    Self::LOG_PREFIX
                );
                LLMMessage::user("(无可用文本内容)")
            } else {
                let image_part_count = parts.iter().filter(|part| matches!(part, MessagePart::Image { .. })).count();
                info!(
                    "{} build_user_message produced multimodal parts total_parts={} image_parts={}",
                    Self::LOG_PREFIX,
                    parts.len(),
                    image_part_count
                );
                LLMMessage::user_with_parts(parts)
            }
        } else {
            let user_text = msg_prop
                .content
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .or_else(|| {
                    parts.into_iter().find_map(|part| match part {
                        MessagePart::Text { text } if !text.trim().is_empty() => Some(text),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| "(无文本内容，可能是仅@或回复)".to_string());

            info!(
                "{} build_user_message produced text-only message because no media part was resolved",
                Self::LOG_PREFIX
            );
            LLMMessage::user(user_text)
        }
    }

    pub(crate) fn build_extracted_message_outputs(
        messages: &[Message],
        bot_id: &str,
        s3_ref: Option<&S3Ref>,
    ) -> ExtractedMessageOutputs {
        let msg_prop = MessageProp::from_messages(messages, Some(bot_id));
        let user_message = Self::build_user_message(messages, &msg_prop, s3_ref);

        ExtractedMessageOutputs {
            user_message,
            content: msg_prop.content.unwrap_or_default(),
            ref_content: msg_prop.ref_content.unwrap_or_default(),
            is_at_me: msg_prop.is_at_me,
            at_target_list: msg_prop.at_target_list,
        }
    }

}

pub(crate) fn extract_message_outputs(
    event: &crate::ims_bot_adapter::runtime::models::event_model::MessageEvent,
    ims_bot_adapter_ref: &crate::ims_bot_adapter::runtime::adapter::SharedBotAdapter,
    target_message_id: Option<i64>,
    explicit_s3_ref: Option<Arc<S3Ref>>,
) -> Result<ExtractedMessageOutputs> {
    let (bot_id, adapter_object_storage) = if tokio::runtime::Handle::try_current().is_ok() {
        block_in_place(|| {
            let adapter = ims_bot_adapter_ref.blocking_lock();
            (adapter.get_bot_id().to_string(), adapter.get_object_storage())
        })
    } else {
        let adapter = ims_bot_adapter_ref.blocking_lock();
        (adapter.get_bot_id().to_string(), adapter.get_object_storage())
    };
    let object_storage = explicit_s3_ref.or(adapter_object_storage);

    info!(
        "[MessageEventExtractor] resolving message content: target_message_id={target_message_id:?} explicit_s3_ref_present={}",
        object_storage.is_some(),
    );

    let message_list = if let Some(message_id) = target_message_id {
        let resolved = if tokio::runtime::Handle::try_current().is_ok() {
            block_in_place(|| {
                tokio::runtime::Handle::current()
                    .block_on(restore_message_list_for_message_id(ims_bot_adapter_ref, message_id))
            })
        } else {
            tokio::runtime::Runtime::new()?
                .block_on(restore_message_list_for_message_id(ims_bot_adapter_ref, message_id))
        }?;

        match resolved {
            Some(resolved) => {
                info!(
                    "[MessageEventExtractor] restored target message_id={} via {} (segments={})",
                    message_id,
                    resolved.source_label,
                    resolved.messages.len()
                );
                resolved.messages
            }
            None if event.message_id == message_id && !event.message_list.is_empty() => {
                event.message_list.clone()
            }
            None => {
                return Err(Error::ValidationError(format!(
                    "message_id {message_id} could not be restored from cache/redis/mysql/get_msg"
                )));
            }
        }
    } else {
        event.message_list.clone()
    };

    let extracted = MessageEventExtractor::build_extracted_message_outputs(
        &message_list,
        &bot_id,
        object_storage.as_deref(),
    );
    info!(
        "[MessageEventExtractor] output user message={}",
        MessageEventExtractor::json_for_log(&extracted.user_message)
    );
    Ok(extracted)
}
