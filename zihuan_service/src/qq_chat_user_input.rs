use std::sync::Arc;

use ims_bot_adapter::adapter::{restore_messages_for_message_id, SharedBotAdapter};
use ims_bot_adapter::message_helpers::render_current_message_body;
use ims_bot_adapter::models::event_model::MessageEvent;
use ims_bot_adapter::models::message::{Message, MessageProp, PlainTextMessage, ReplyMessage};
use ims_bot_adapter::multimodal_image_url::{
    resolve_image_message_part, resolve_plain_text_segments, ImagePartSource, ResolvedTextSegment,
};
use ims_bot_adapter::utils;
use ims_bot_adapter::{
    CURRENT_MESSAGE_LABEL, FORWARD_CONTENT_LABEL, FORWARD_END_MARKER, FORWARD_NODE_LABEL, FORWARD_START_MARKER,
    REPLAY_CONTENT_LABEL, REPLY_END_MARKER, REPLY_MESSAGE_LABEL, REPLY_START_MARKER, SENDER_LABEL,
};
use log::{info, warn};
use zihuan_core::llm::MessagePart;
use zihuan_core::runtime::block_async;
use zihuan_graph_engine::object_storage::S3Ref;

const LOG_PREFIX: &str = "[QqChatAgent]";

#[derive(Debug, Clone, Default)]
pub struct MultimodalImageStats {
    pub image_parts: usize,
    pub local_file_images: usize,
    pub object_storage_images: usize,
    pub downloaded_remote_images: usize,
    pub uploaded_to_s3_images: usize,
    pub data_url_images: usize,
    pub skipped_images: usize,
}

impl MultimodalImageStats {
    fn record_success(&mut self, source: ImagePartSource) {
        self.image_parts += 1;
        match source {
            ImagePartSource::LocalFile => self.local_file_images += 1,
            ImagePartSource::ObjectStorage => self.object_storage_images += 1,
            ImagePartSource::DownloadedRemote => self.downloaded_remote_images += 1,
            ImagePartSource::UploadedToS3 => self.uploaded_to_s3_images += 1,
            ImagePartSource::DataUrl => self.data_url_images += 1,
        }
    }

    fn record_skipped(&mut self) {
        self.skipped_images += 1;
    }
}

#[derive(Debug, Clone)]
pub struct PreparedCurrentTurnUserInput {
    pub event: MessageEvent,
    pub current_text: String,
    pub reference_blocks: Vec<String>,
    pub is_at_me: bool,
    pub at_target_list: Vec<String>,
    pub current_parts: Vec<MessagePart>,
    pub reference_parts: Vec<MessagePart>,
    pub has_media: bool,
    pub current_image_reference_lines: Vec<String>,
    pub reference_image_reference_lines: Vec<String>,
    pub multimodal_stats: MultimodalImageStats,
}

impl PreparedCurrentTurnUserInput {
    pub fn has_reference_context(&self) -> bool {
        !self.reference_blocks.is_empty()
            || !self.reference_parts.is_empty()
            || !self.reference_image_reference_lines.is_empty()
    }

    pub fn current_text_for_prompt(&self) -> &str {
        let trimmed = self.current_text.trim();
        if trimmed.is_empty() {
            "(无当前正文，可能是仅回复、转发或图片)"
        } else {
            trimmed
        }
    }

    pub fn referenced_context_text(&self) -> String {
        self.reference_blocks.join("\n\n")
    }
}

pub(crate) fn prepare_current_turn_user_input(
    event: &MessageEvent,
    adapter: &SharedBotAdapter,
    bot_id: &str,
    bot_name: &str,
    s3_ref: Option<&Arc<S3Ref>>,
) -> PreparedCurrentTurnUserInput {
    let hydrated_event = hydrate_reply_sources(event, adapter);
    prepare_current_turn_user_input_from_event(&hydrated_event, bot_id, bot_name, s3_ref)
}

pub(crate) fn prepare_current_turn_user_input_from_event(
    event: &MessageEvent,
    bot_id: &str,
    bot_name: &str,
    s3_ref: Option<&Arc<S3Ref>>,
) -> PreparedCurrentTurnUserInput {
    let msg_prop = MessageProp::from_messages_with_bot_name(&event.message_list, Some(bot_id), Some(bot_name));
    let mut current_text = render_direct_current_message_body(&event.message_list).unwrap_or_default();
    if msg_prop.is_at_me {
        current_text = zihuan_core::utils::string_utils::strip_leading_bot_mention(&current_text, bot_id, bot_name);
    }

    let reference_blocks = collect_reference_context_text(&event.message_list);
    if current_text.trim().is_empty() && reference_blocks.is_empty() {
        current_text = "(无文本内容，可能是仅@或回复)".to_string();
    }

    let mut current_parts = Vec::new();
    let mut current_text_buffer = String::new();
    let mut reference_parts = Vec::new();
    let mut reference_text_buffer = String::new();
    let mut has_media = false;
    let mut multimodal_stats = MultimodalImageStats::default();
    append_current_messages_as_parts(
        &event.message_list,
        &mut current_parts,
        &mut current_text_buffer,
        &mut has_media,
        s3_ref,
        &mut multimodal_stats,
    );
    flush_text_part(&mut current_parts, &mut current_text_buffer);

    append_reference_messages_as_parts(
        &event.message_list,
        &mut reference_parts,
        &mut reference_text_buffer,
        &mut has_media,
        s3_ref,
        &mut multimodal_stats,
    );
    flush_text_part(&mut reference_parts, &mut reference_text_buffer);

    if msg_prop.is_at_me {
        for part in &mut current_parts {
            if let MessagePart::Text { text } = part {
                let stripped = zihuan_core::utils::string_utils::strip_leading_bot_mention(text, bot_id, bot_name);
                if stripped.len() < text.len() {
                    *text = stripped;
                    break;
                }
            }
        }
    }

    info!(
        "{LOG_PREFIX} Prepared multimodal user input: current_parts={}, reference_parts={}, image_parts={}, local_file_images={}, object_storage_images={}, downloaded_remote_images={}, uploaded_to_s3_images={}, data_url_images={}, skipped_images={}",
        current_parts.len(),
        reference_parts.len(),
        multimodal_stats.image_parts,
        multimodal_stats.local_file_images,
        multimodal_stats.object_storage_images,
        multimodal_stats.downloaded_remote_images,
        multimodal_stats.uploaded_to_s3_images,
        multimodal_stats.data_url_images,
        multimodal_stats.skipped_images,
    );

    PreparedCurrentTurnUserInput {
        event: event.clone(),
        current_text,
        reference_blocks,
        is_at_me: msg_prop.is_at_me,
        at_target_list: msg_prop.at_target_list,
        current_parts,
        reference_parts,
        has_media,
        current_image_reference_lines: current_image_prompt_reference_lines(&event.message_list),
        reference_image_reference_lines: reference_image_prompt_reference_lines(&event.message_list),
        multimodal_stats,
    }
}

pub(crate) fn expand_messages_for_inference(messages: &[Message]) -> Vec<Message> {
    let mut expanded = Vec::new();

    for message in messages {
        match message {
            Message::Reply(reply) => {
                expanded.push(Message::PlainText(PlainTextMessage {
                    text: REPLY_START_MARKER.to_string(),
                }));
                if let Some(source_messages) = valid_reply_source_messages(reply) {
                    expanded.extend(expand_messages_for_inference(source_messages));
                } else {
                    expanded.push(message.clone());
                }
                expanded.push(Message::PlainText(PlainTextMessage {
                    text: REPLY_END_MARKER.to_string(),
                }));
            }
            Message::Forward(forward) => {
                if forward.content.is_empty() {
                    expanded.push(message.clone());
                    continue;
                }

                expanded.push(Message::PlainText(PlainTextMessage {
                    text: FORWARD_START_MARKER.to_string(),
                }));

                for (index, node) in forward.content.iter().enumerate() {
                    let sender = node.nickname.as_deref().or(node.user_id.as_deref()).unwrap_or("unknown");
                    expanded.push(Message::PlainText(PlainTextMessage {
                        text: format!("[{} {} {}: {}]", FORWARD_NODE_LABEL, index + 1, SENDER_LABEL, sender),
                    }));
                    expanded.extend(expand_messages_for_inference(&node.content));
                }

                expanded.push(Message::PlainText(PlainTextMessage {
                    text: FORWARD_END_MARKER.to_string(),
                }));
            }
            _ => expanded.push(message.clone()),
        }
    }

    expanded
}

fn hydrate_reply_sources(event: &MessageEvent, adapter: &SharedBotAdapter) -> MessageEvent {
    fn hydrate_messages(messages: &mut [Message], adapter: &SharedBotAdapter) {
        for message in messages {
            match message {
                Message::Reply(reply) => {
                    if valid_reply_source_messages(reply).is_none() {
                        match block_async(restore_messages_for_message_id(adapter, reply.id)) {
                            Ok(Some(messages)) => {
                                reply.message_source = Some(messages);
                            }
                            Ok(None) => {}
                            Err(error) => {
                                warn!(
                                    "{LOG_PREFIX} failed to restore reply source inside qq_chat_agent for message_id={}: {}",
                                    reply.id, error
                                );
                            }
                        }
                    }

                    if let Some(source_messages) = reply.message_source.as_mut() {
                        hydrate_messages(source_messages, adapter);
                    }
                }
                Message::Forward(forward) => {
                    for node in &mut forward.content {
                        hydrate_messages(&mut node.content, adapter);
                    }
                }
                _ => {}
            }
        }
    }

    let mut hydrated = event.clone();
    hydrate_messages(&mut hydrated.message_list, adapter);
    hydrated
}

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

pub(crate) fn flush_text_part(parts: &mut Vec<MessagePart>, buffer: &mut String) {
    let text = buffer.trim();
    if !text.is_empty() {
        parts.push(MessagePart::text(text.to_string()));
    }
    buffer.clear();
}

pub(crate) fn append_prepared_parts(
    parts: &mut Vec<MessagePart>,
    text_buffer: &mut String,
    prefix: &str,
    prepared_parts: &[MessagePart],
) {
    if !prefix.is_empty() {
        text_buffer.push_str(prefix);
    }

    for part in prepared_parts {
        match part {
            MessagePart::Text { text } => text_buffer.push_str(text),
            MessagePart::Image { .. } | MessagePart::Video { .. } => {
                flush_text_part(parts, text_buffer);
                parts.push(part.clone());
            }
        }
    }
}

pub(crate) fn build_prepared_input_metadata(input: &PreparedCurrentTurnUserInput, bot_name: &str) -> String {
    let environment = format!("[Environment]\n- Your name: {bot_name}");
    let sender_name =
        ims_bot_adapter::utils::sender_display_name!(&input.event.sender.nickname, &input.event.sender.card);
    let at_mention = if input.is_at_me {
        "\n- You were @-mentioned in this message"
    } else {
        ""
    };
    let at_targets = if input.at_target_list.is_empty() {
        String::new()
    } else {
        format!("\n- At targets: {}", input.at_target_list.join(", "))
    };
    let metadata = format!(
        "[User Message Metadata]\n- Message type: {ty}\n- Sender name: {sender_name}{at_mention}{at_targets}",
        ty = input.event.message_type.as_str(),
    );
    format!("{environment}\n\n{metadata}")
}

fn append_plain_text_as_parts(
    text: &str,
    parts: &mut Vec<MessagePart>,
    text_buffer: &mut String,
    has_media: &mut bool,
    s3_ref: Option<&Arc<S3Ref>>,
    image_stats: &mut MultimodalImageStats,
) {
    for segment in resolve_plain_text_segments(text, s3_ref.map(AsRef::as_ref), true, LOG_PREFIX) {
        match segment {
            ResolvedTextSegment::Text(text) => append_text_segment(text_buffer, &text),
            ResolvedTextSegment::Image(resolved) => {
                flush_text_part(parts, text_buffer);
                parts.push(resolved.part);
                *has_media = true;
                image_stats.record_success(resolved.source);
            }
        }
    }
}

fn append_current_messages_as_parts(
    messages: &[Message],
    parts: &mut Vec<MessagePart>,
    text_buffer: &mut String,
    has_media: &mut bool,
    s3_ref: Option<&Arc<S3Ref>>,
    image_stats: &mut MultimodalImageStats,
) {
    for message in messages {
        match message {
            Message::PlainText(plain) => {
                append_plain_text_as_parts(&plain.text, parts, text_buffer, has_media, s3_ref, image_stats);
            }
            Message::Image(image) => {
                if let Some(resolved) = resolve_image_message_part(image, s3_ref.map(AsRef::as_ref), true, LOG_PREFIX) {
                    flush_text_part(parts, text_buffer);
                    parts.push(resolved.part);
                    *has_media = true;
                    image_stats.record_success(resolved.source);
                } else {
                    append_text_segment(text_buffer, &image.to_string());
                    image_stats.record_skipped();
                }
            }
            Message::Reply(_) | Message::Forward(_) => {}
            other => append_text_segment(text_buffer, &other.to_string()),
        }
    }
}

fn append_reference_messages_as_parts(
    messages: &[Message],
    parts: &mut Vec<MessagePart>,
    text_buffer: &mut String,
    has_media: &mut bool,
    s3_ref: Option<&Arc<S3Ref>>,
    image_stats: &mut MultimodalImageStats,
) {
    for message in messages {
        match message {
            Message::Reply(reply) => {
                let Some(source_messages) = valid_reply_source_messages(reply) else {
                    continue;
                };
                if !text_buffer.is_empty() {
                    text_buffer.push_str("\n\n");
                }
                text_buffer.push_str(&format!("[{}]\n", REPLAY_CONTENT_LABEL));
                append_current_messages_as_parts(source_messages, parts, text_buffer, has_media, s3_ref, image_stats);
                append_reference_messages_as_parts(source_messages, parts, text_buffer, has_media, s3_ref, image_stats);
            }
            Message::Forward(forward) => {
                if forward.content.is_empty() {
                    continue;
                }
                if !text_buffer.is_empty() {
                    text_buffer.push_str("\n\n");
                }
                text_buffer.push_str(&format!("[{}]\n", FORWARD_CONTENT_LABEL));
                for (index, node) in forward.content.iter().enumerate() {
                    if index > 0 && !text_buffer.ends_with('\n') {
                        text_buffer.push('\n');
                    }
                    let sender = node.nickname.as_deref().or(node.user_id.as_deref()).unwrap_or("unknown");
                    text_buffer.push_str(sender);
                    text_buffer.push_str(": ");
                    append_current_messages_as_parts(&node.content, parts, text_buffer, has_media, s3_ref, image_stats);
                    append_reference_messages_as_parts(
                        &node.content,
                        parts,
                        text_buffer,
                        has_media,
                        s3_ref,
                        image_stats,
                    );
                    if !text_buffer.ends_with('\n') {
                        text_buffer.push('\n');
                    }
                }
            }
            _ => {}
        }
    }
}

fn valid_reply_source_messages(reply: &ReplyMessage) -> Option<&[Message]> {
    let source_messages = reply.message_source.as_deref()?;
    if utils::messages_have_effective_content(source_messages, 0) {
        Some(source_messages)
    } else {
        None
    }
}

fn collect_reference_context_text(messages: &[Message]) -> Vec<String> {
    let mut blocks = Vec::new();
    for message in messages {
        match message {
            Message::Reply(reply) => {
                if let Some(source_messages) = valid_reply_source_messages(reply) {
                    let rendered =
                        zihuan_core::ims_bot_adapter::models::message::render_messages_readable(source_messages);
                    let trimmed = rendered.trim();
                    if !trimmed.is_empty() {
                        blocks.push(format!("[{}]\n{trimmed}", REPLAY_CONTENT_LABEL));
                    }
                }
            }
            Message::Forward(forward) => {
                if forward.content.is_empty() {
                    continue;
                }
                let rendered =
                    zihuan_core::ims_bot_adapter::models::message::render_messages_readable(&[Message::Forward(
                        forward.clone(),
                    )]);
                let trimmed = rendered.trim();
                if !trimmed.is_empty() {
                    blocks.push(format!("[{}]\n{trimmed}", FORWARD_CONTENT_LABEL));
                }
            }
            _ => {}
        }
    }
    blocks
}

#[derive(Debug, Clone)]
struct ImagePromptReference {
    location: String,
    media_id: String,
}

fn traverse_current_messages_for_image_references(
    messages: &[Message],
    current_path: &str,
    references: &mut Vec<ImagePromptReference>,
) {
    for message in messages {
        match message {
            Message::Image(image) => {
                let media_id = image.media.media_id.trim();
                if media_id.is_empty() {
                    continue;
                }
                references.push(ImagePromptReference {
                    location: current_path.to_string(),
                    media_id: media_id.to_string(),
                });
            }
            Message::PlainText(_) | Message::At(_) | Message::Reply(_) | Message::Forward(_) => {}
        }
    }
}

fn traverse_reference_messages_for_image_references(messages: &[Message], references: &mut Vec<ImagePromptReference>) {
    for message in messages {
        match message {
            Message::Reply(reply) => {
                if let Some(source_messages) = valid_reply_source_messages(reply) {
                    traverse_current_messages_for_image_references(source_messages, REPLY_MESSAGE_LABEL, references);
                    traverse_reference_messages_for_image_references(source_messages, references);
                }
            }
            Message::Forward(forward) => {
                for (node_index, node) in forward.content.iter().enumerate() {
                    let sender = node.nickname.as_deref().or(node.user_id.as_deref()).unwrap_or("unknown");
                    traverse_current_messages_for_image_references(
                        &node.content,
                        &format!(
                            "{} / {} {}({})",
                            FORWARD_CONTENT_LABEL,
                            FORWARD_NODE_LABEL,
                            node_index + 1,
                            sender
                        ),
                        references,
                    );
                    traverse_reference_messages_for_image_references(&node.content, references);
                }
            }
            Message::PlainText(_) | Message::At(_) | Message::Image(_) => {}
        }
    }
}

fn current_image_prompt_reference_lines(messages: &[Message]) -> Vec<String> {
    let mut references = Vec::new();
    traverse_current_messages_for_image_references(messages, CURRENT_MESSAGE_LABEL, &mut references);
    references
        .into_iter()
        .map(|reference| format!("{} media_id={}", reference.location, reference.media_id))
        .collect()
}

fn reference_image_prompt_reference_lines(messages: &[Message]) -> Vec<String> {
    let mut references = Vec::new();
    traverse_reference_messages_for_image_references(messages, &mut references);
    references
        .into_iter()
        .map(|reference| format!("{} media_id={}", reference.location, reference.media_id))
        .collect()
}

fn render_direct_current_message_body(messages: &[Message]) -> Option<String> {
    let filtered: Vec<Message> = messages
        .iter()
        .filter(|message| !matches!(message, Message::Reply(_) | Message::Forward(_)))
        .cloned()
        .collect();
    if filtered.is_empty() {
        return None;
    }

    let rendered = render_current_message_body(&filtered)?;
    let trimmed = rendered.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
