use std::collections::HashSet;

use log::warn;
use zihuan_core::llm::{LLMMessage, LLMMessagePart, MessageRole};

const IMAGE_OMITTED_PLACEHOLDER: &str = "[image omitted]";
const VIDEO_OMITTED_PLACEHOLDER: &str = "[video omitted]";

pub fn downgrade_messages_for_model(
    messages: Vec<LLMMessage>,
    supports_multimodal_input: bool,
) -> Vec<LLMMessage> {
    if supports_multimodal_input {
        return messages;
    }

    messages
        .into_iter()
        .map(downgrade_message_for_text_only_model)
        .collect()
}

pub fn downgrade_message_for_model(
    message: LLMMessage,
    supports_multimodal_input: bool,
) -> LLMMessage {
    if supports_multimodal_input {
        return message;
    }

    downgrade_message_for_text_only_model(message)
}

fn downgrade_message_for_text_only_model(mut message: LLMMessage) -> LLMMessage {
    let has_non_text_parts = message
        .parts
        .iter()
        .any(|part| !matches!(part, LLMMessagePart::Text { .. }));
    if has_non_text_parts {
        message.parts = vec![LLMMessagePart::text(parts_to_text(message.parts))];
    }
    message
}

fn parts_to_text(parts: Vec<LLMMessagePart>) -> String {
    let mut segments = Vec::with_capacity(parts.len());

    for part in parts {
        match part {
            LLMMessagePart::Text { text } => segments.push(text),
            LLMMessagePart::Image { media } => {
                segments.push(media_placeholder(
                    IMAGE_OMITTED_PLACEHOLDER,
                    media.primary_locator().unwrap_or(""),
                ));
            }
            LLMMessagePart::Video { media } => {
                segments.push(media_placeholder(
                    VIDEO_OMITTED_PLACEHOLDER,
                    media.primary_locator().unwrap_or(""),
                ));
            }
        }
    }

    segments.join("\n")
}

fn media_placeholder(prefix: &str, url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix} {trimmed}")
    }
}

/// Remove dangling / unresolved tool-call sequences from a message history so
/// that the sequence passed to the LLM is always well-formed.
pub fn sanitize_messages_for_inference(messages: Vec<LLMMessage>) -> Vec<LLMMessage> {
    let mut sanitized: Vec<LLMMessage> = Vec::with_capacity(messages.len());
    let mut pending: Option<(usize, HashSet<String>)> = None;

    for message in messages {
        if !message.tool_calls.is_empty() {
            if let Some((start, ids)) = pending.take() {
                warn!(
                    "[message_utils] Dropping incomplete tool-call segment before new assistant tool-call: unresolved_ids={:?}",
                    ids
                );
                sanitized.truncate(start);
            }
            let ids: HashSet<String> = message.tool_calls.iter().map(|tc| tc.id.clone()).collect();
            let start = sanitized.len();
            sanitized.push(message);
            if !ids.is_empty() {
                pending = Some((start, ids));
            }
            continue;
        }

        if matches!(message.role, MessageRole::Tool) {
            let mut keep = false;
            if let Some((_, unresolved)) = pending.as_mut() {
                if let Some(id) = &message.tool_call_id {
                    if unresolved.remove(id) {
                        keep = true;
                    }
                }
            }
            if keep {
                sanitized.push(message);
                if pending.as_ref().is_some_and(|(_, ids)| ids.is_empty()) {
                    pending = None;
                }
            } else {
                warn!("[message_utils] Dropping orphan tool message");
            }
            continue;
        }

        if let Some((start, ids)) = pending.take() {
            warn!(
                "[message_utils] Dropping dangling tool-call segment before non-tool message: unresolved_ids={:?}",
                ids
            );
            sanitized.truncate(start);
        }
        sanitized.push(message);
    }

    if let Some((start, ids)) = pending {
        warn!(
            "[message_utils] Dropping dangling segment at end of history: unresolved_ids={:?}",
            ids
        );
        sanitized.truncate(start);
    }

    sanitized
}

/// Returns `true` if `content` looks like a transport-level LLM error string.
pub fn is_transport_error(content: &str) -> bool {
    content.starts_with("Error:")
}
