use std::collections::HashSet;

use crate::model_inference::llm::{LLMMessage, MessagePart, MessageRole};
use log::{debug, warn};

const IMAGE_OMITTED_PLACEHOLDER: &str = "[image omitted]";
const VIDEO_OMITTED_PLACEHOLDER: &str = "[video omitted]";

pub fn downgrade_messages_for_model(
    messages: Vec<LLMMessage>,
    supports_multimodal_input: bool,
) -> Vec<LLMMessage> {
    if supports_multimodal_input {
        return messages;
    }

    messages.into_iter().map(downgrade_message_for_text_only_model).collect()
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
    let has_non_text_parts =
        message.parts.iter().any(|part| !matches!(part, MessagePart::Text { .. }));
    if has_non_text_parts {
        message.parts = vec![MessagePart::text(parts_to_text(message.parts))];
    }
    message
}

fn parts_to_text(parts: Vec<MessagePart>) -> String {
    let mut segments = Vec::with_capacity(parts.len());

    for part in parts {
        match part {
            MessagePart::Text { text } => segments.push(text),
            MessagePart::Image { media } => {
                segments.push(media_placeholder(IMAGE_OMITTED_PLACEHOLDER, &media.media_id));
            }
            MessagePart::Video { media } => {
                segments.push(media_placeholder(VIDEO_OMITTED_PLACEHOLDER, &media.media_id));
            }
        }
    }

    segments.join("\n")
}

pub fn media_placeholder(prefix: &str, media_id: &str) -> String {
    let media_id = media_id.trim();
    if media_id.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix} (media_id: {media_id})")
    }
}

/// Full text content of one message as the model receives it: text parts joined, and for
/// multimodal messages one marker line per media part in its position.
pub fn raw_message_content(message: &LLMMessage) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            MessagePart::Text { text } => Some(text.clone()),
            MessagePart::Image { media } => Some(media_placeholder("[image]", &media.media_id)),
            MessagePart::Video { media } => Some(media_placeholder("[video]", &media.media_id)),
        })
        .collect::<Vec<_>>()
        .join("\n")
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

/// Normalizes the system-role layout to a single leading system message.
///
/// A conversation is legitimately assembled with several system prompts (planer prompt,
/// workspace prompt, and system-role history records). OpenAI-compatible providers are not
/// consistent about accepting that: some reject any request where a system message is not the
/// single first message, even a second consecutive one, with
/// `400 Bad Request: System message must be at the beginning.`
///
/// Merging keeps every instruction but guarantees the provider-compatible layout: the system
/// texts are concatenated in their original order into one message at index 0.
pub fn normalize_system_messages(messages: Vec<LLMMessage>) -> Vec<LLMMessage> {
    let mut system_texts: Vec<String> = Vec::new();
    let mut non_system: Vec<LLMMessage> = Vec::with_capacity(messages.len());
    let mut system_count = 0usize;

    for message in messages {
        if !matches!(message.role, MessageRole::System) {
            non_system.push(message);
            continue;
        }
        system_count += 1;
        if let Some(text) = message.content_text_owned() {
            let text = text.trim();
            if !text.is_empty() {
                system_texts.push(text.to_string());
            }
        }
    }

    if system_texts.is_empty() {
        return non_system;
    }
    if system_count > 1 {
        debug!(
            "[message_utils] Merged {system_count} system messages into one leading system message"
        );
    }

    let mut normalized = Vec::with_capacity(non_system.len() + 1);
    normalized.push(LLMMessage::system(system_texts.join("\n\n")));
    normalized.extend(non_system);
    normalized
}

/// Returns `true` if `content` looks like a transport-level LLM error string.
pub fn is_transport_error(content: &str) -> bool {
    content.starts_with("Error:")
}
