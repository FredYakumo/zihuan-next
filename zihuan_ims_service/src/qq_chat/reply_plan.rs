//! Reply formatting: translate free-form model output into protocol-compliant QQ message
//! batches. This is the implementation behind the `qq_format_output` procedure stage; the
//! legacy `reply_batch_builder` closure used by non-turn callers delegates here too.
//!
//! Sits at the boundary between free-form model output and the QQ messaging protocol.
//! Performs three semantic transformations:
//!
//! - **Segmentation** — decomposes raw text into typed semantic units (text, mentions,
//!   images, control directives) so each carries its own protocol-level representation.
//! - **Line dispatch** — every newline boundary in the model output becomes a separate
//!   outgoing message (blank lines are dropped), so a reply written as several lines is
//!   delivered as several human-paced messages instead of one bubble with embedded gaps.
//! - **Adaptive dispatch** — voluminous content is consolidated into forward-messages to
//!   avoid chat flooding; concise replies stay as individual sequential messages.
//! - **Protocol normalization** — applies QQ-specific invariants (e.g. mandatory spacing
//!   after mentions) that the LLM cannot be expected to produce reliably.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use zihuan_core::agent::utils::string_utils::is_no_reply_directive;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_core::graph::data_value::DataValue;
use zihuan_core::ims_bot_adapter::models::message::{
    AtTargetMessage, ForwardMessage, ForwardNodeMessage, ImageMessage, Message, PersistedMedia,
    PersistedMediaSource, PlainTextMessage, ReplyMessage,
};
use zihuan_core::nlp::{PunctuationSegmenter, TextSegmenter};
use zihuan_core::utils::string_utils::{parse_at_segment, parse_tag_value};

pub(crate) use super::model::{
    QqChatServiceReplyBatchBuilder, QqChatServiceReplyBuildRequest, QqChatServiceReplyBuildResult,
};

pub(crate) const QQ_CHAT_REPLY_DIRECTIVE_RUNTIME_KEY: &str = "qq_chat_reply_directive";

/// Simulated typing delay inserted before each follow-up message: the receiving side
/// sees the bot "type" for `chars * 15ms` instead of dumping all bubbles at once.
pub(crate) const REPLY_TYPING_DELAY_MS_PER_CHAR: u64 = 15;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum QqChatServiceReplyDirective {
    Explicit { message_id: i64 },
    TriggerMessage,
}

pub(crate) fn store_reply_directive(
    shared_runtime_values: &Arc<Mutex<HashMap<String, DataValue>>>,
    directive: QqChatServiceReplyDirective,
) {
    shared_runtime_values.lock().unwrap().insert(
        QQ_CHAT_REPLY_DIRECTIVE_RUNTIME_KEY.to_string(),
        DataValue::Json(serde_json::to_value(directive).unwrap_or(Value::Null)),
    );
}

pub(crate) fn take_reply_directive(
    shared_runtime_values: &Arc<Mutex<HashMap<String, DataValue>>>,
) -> Option<QqChatServiceReplyDirective> {
    let value = shared_runtime_values
        .lock()
        .unwrap()
        .remove(QQ_CHAT_REPLY_DIRECTIVE_RUNTIME_KEY)?;
    match value {
        DataValue::Json(value) => serde_json::from_value(value).ok(),
        _ => None,
    }
}

/// Per-batch typing delay: batch 0 sends immediately, every later batch waits for the
/// char count of its own text (the "second sentence" the receiver is waiting for).
pub(crate) fn reply_batch_delays_ms(batches: &[Vec<Message>]) -> Vec<u64> {
    batches
        .iter()
        .enumerate()
        .map(|(index, batch)| {
            if index == 0 {
                0
            } else {
                batch
                    .iter()
                    .map(|message| match message {
                        Message::PlainText(text) => text.text.chars().count() as u64,
                        _ => 0,
                    })
                    .sum::<u64>()
                    * REPLY_TYPING_DELAY_MS_PER_CHAR
            }
        })
        .collect()
}

/// Render-friendly description of one outbound bubble for the context view: text segments
/// carry their full text and forward nodes are listed as an array, so the view can draw the
/// actual chat bubbles instead of a wire-protocol dump.
pub(crate) fn bubble_stage_summaries(batches: &[Vec<Message>], delays_ms: &[u64]) -> Vec<Value> {
    batches
        .iter()
        .enumerate()
        .map(|(index, batch)| {
            json!({
                "index": index + 1,
                "delay_ms": delays_ms.get(index).copied().unwrap_or(0),
                "segments": batch.iter().map(message_stage_summary).collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn message_stage_summary(message: &Message) -> Value {
    match message {
        Message::PlainText(text) => json!({ "type": "text", "text": text.text }),
        Message::At(at) => json!({ "type": "at", "target": at.target }),
        Message::Reply(reply) => json!({ "type": "reply", "id": reply.id }),
        Message::Image(image) => json!({
            "type": "image",
            "source": image.name().or(image.rustfs_path()).or(image.original_source()),
        }),
        Message::Forward(forward) => json!({
            "type": "forward",
            "nodes": forward
                .content
                .iter()
                .map(|node| {
                    node.content
                        .iter()
                        .filter_map(|message| match message {
                            Message::PlainText(text) => Some(text.text.as_str()),
                            _ => None,
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>(),
        }),
    }
}

pub(crate) const MAX_FORWARD_NODE_CHARS: usize = 800;

/// Upper bound of individually sent text messages per reply before the content is
/// consolidated into a forward message. Above this (or above `2 * max_message_length`
/// total text chars) the reply ships as a forward card instead of a message burst.
const MAX_REGULAR_TEXT_MESSAGES: usize = 5;

#[derive(Debug, Clone, Default)]
pub(crate) struct QqOutboundPlan {
    pub batches: Vec<Vec<Message>>,
    pub suppress_send: bool,
    pub visible_text: Option<String>,
}

#[derive(Debug, Clone)]
enum ReplySegment {
    Text(String),
    At(String),
    Image(ImageMessage),
    NoReply,
}

#[derive(Debug, Clone)]
enum PlannedSegment {
    At(AtTargetMessage),
    Text(String),
    Image(ImageMessage),
}

#[derive(Debug, Clone, Copy, Default)]
struct SplitRepairState {
    in_code_fence: bool,
    in_double_quote: bool,
    in_cn_quote: bool,
}

pub(crate) fn build_reply_batch_builder(
    segmenter: Arc<dyn TextSegmenter>,
) -> QqChatServiceReplyBatchBuilder {
    Arc::new(move |request| {
        let plan = plan_model_reply(request, segmenter.as_ref())?;
        Ok(QqChatServiceReplyBuildResult {
            batches: plan.batches,
            suppress_send: plan.suppress_send,
        })
    })
}

/// Build a `QqChatServiceReplyBuildResult` from raw reply parameters by calling the
/// provided `reply_batch_builder`.
pub(crate) fn build_reply_result(
    reply_text: &str,
    is_group: bool,
    sender_id: &str,
    sender_nickname: &str,
    sender_card: &str,
    bot_id: &str,
    bot_name: &str,
    max_message_length: usize,
    reply_directive: Option<QqChatServiceReplyDirective>,
    trigger_message_id: Option<i64>,
    available_media: HashMap<String, PersistedMedia>,
    rdb_pool: Option<RelationalDbConnection>,
    reply_batch_builder: Option<&QqChatServiceReplyBatchBuilder>,
) -> Result<QqChatServiceReplyBuildResult> {
    let request = QqChatServiceReplyBuildRequest {
        assistant_text: reply_text.to_string(),
        is_group,
        sender_id: sender_id.to_string(),
        sender_nickname: sender_nickname.to_string(),
        sender_card: sender_card.to_string(),
        bot_id: bot_id.to_string(),
        bot_name: bot_name.to_string(),
        max_message_length,
        reply_directive,
        trigger_message_id,
        available_media,
        rdb_pool,
    };

    if let Some(builder) = reply_batch_builder {
        builder(&request)
    } else {
        Err(Error::ValidationError(
            "no reply_batch_builder available for build_reply_result".to_string(),
        ))
    }
}

/// Translate LLM-generated reply text into protocol-compliant QQ message batches.
pub(crate) fn plan_model_reply(
    request: &QqChatServiceReplyBuildRequest,
    segmenter: &dyn TextSegmenter,
) -> Result<QqOutboundPlan> {
    let normalized_text = normalize_assistant_text(request);
    let segments = parse_reply_line_units(&normalized_text);
    if segments.iter().any(|segment| matches!(segment, ReplySegment::NoReply)) {
        return Ok(QqOutboundPlan {
            batches: Vec::new(),
            suppress_send: true,
            visible_text: None,
        });
    }

    let mut expanded_segments = Vec::new();
    for segment in segments {
        match segment {
            ReplySegment::Text(text) => {
                for chunk in
                    split_plain_text_for_forward(&text, request.max_message_length, segmenter)
                {
                    expanded_segments.push(PlannedSegment::Text(chunk));
                }
            }
            ReplySegment::At(target) => {
                expanded_segments.push(PlannedSegment::At(AtTargetMessage { target: Some(target) }))
            }
            ReplySegment::Image(image) => {
                expanded_segments.push(PlannedSegment::Image(image));
            }
            ReplySegment::NoReply => {}
        }
    }

    let reply_message =
        resolve_reply_message(request.reply_directive.as_ref(), request.trigger_message_id);

    // Use forward messages only for voluminous content because QQ clients may not render
    // images in forward nodes. Line-split short replies stay as individual messages.
    let total_text_chars: usize = expanded_segments
        .iter()
        .map(|segment| match segment {
            PlannedSegment::Text(text) => text.chars().count(),
            _ => 0,
        })
        .sum();
    let text_unit_count = expanded_segments
        .iter()
        .filter(|segment| matches!(segment, PlannedSegment::Text(_)))
        .count();
    let forced_forward = total_text_chars > request.max_message_length.saturating_mul(2)
        || text_unit_count > MAX_REGULAR_TEXT_MESSAGES;

    let mut batches = if forced_forward {
        build_forced_forward_batches(
            expanded_segments,
            reply_message,
            &request.bot_id,
            &request.bot_name,
        )
    } else {
        build_regular_batches(expanded_segments, reply_message)
    };

    // QQ protocol requires a space after @, otherwise the mention marker is ignored
    ensure_space_after_at(&mut batches);

    crate::storage::media::resolve_media_references(
        &mut batches,
        &request.available_media,
        request.rdb_pool.as_ref(),
    )?;

    Ok(QqOutboundPlan {
        batches,
        suppress_send: false,
        visible_text: Some(normalized_text),
    })
}

/// Split the reply into per-line units: every newline boundary produces a separate
/// outgoing message and blank lines are dropped. This implements the multi-message
/// dispatch — `aaa\nbbb` and `aaa\n\nbbb` both become two sends.
fn split_reply_line_units(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n");
    normalized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_reply_line_units(text: &str) -> Vec<ReplySegment> {
    let mut segments = Vec::new();
    for unit in split_reply_line_units(text) {
        segments.extend(parse_reply_segments(&unit));
    }
    segments
}

fn normalize_assistant_text(request: &QqChatServiceReplyBuildRequest) -> String {
    if request.is_group {
        request.assistant_text.replace("@sender", &format!("@{}", request.sender_id))
    } else {
        request.assistant_text.clone()
    }
}

fn resolve_reply_message(
    directive: Option<&QqChatServiceReplyDirective>,
    trigger_message_id: Option<i64>,
) -> Option<ReplyMessage> {
    let message_id = match directive {
        Some(QqChatServiceReplyDirective::Explicit { message_id }) => Some(*message_id),
        Some(QqChatServiceReplyDirective::TriggerMessage) => trigger_message_id,
        None => None,
    }?;

    Some(ReplyMessage { id: message_id, message_source: None })
}

/// Batch segments so each text unit becomes its own outgoing message. Mention segments
/// ride along with the text (or image) they introduce instead of standing alone.
fn build_regular_batches(
    segments: Vec<PlannedSegment>,
    reply: Option<ReplyMessage>,
) -> Vec<Vec<Message>> {
    let mut batches = Vec::new();
    let mut current_batch = Vec::new();

    let mention_only =
        |batch: &[Message]| batch.iter().all(|message| matches!(message, Message::At(_)));

    for segment in segments {
        match segment {
            PlannedSegment::At(at) => current_batch.push(Message::At(at)),
            PlannedSegment::Text(text) => {
                if !current_batch.is_empty() && !mention_only(&current_batch) {
                    flush_batch(&mut batches, &mut current_batch);
                }
                current_batch.push(Message::PlainText(PlainTextMessage { text }));
                flush_batch(&mut batches, &mut current_batch);
            }
            PlannedSegment::Image(image) => {
                if !current_batch.is_empty() && mention_only(&current_batch) {
                    current_batch.push(Message::Image(image));
                    flush_batch(&mut batches, &mut current_batch);
                } else {
                    flush_batch(&mut batches, &mut current_batch);
                    batches.push(vec![Message::Image(image)]);
                }
            }
        }
    }

    flush_batch(&mut batches, &mut current_batch);
    attach_reply_to_first_batch(&mut batches, reply);
    batches
}

fn build_forced_forward_batches(
    segments: Vec<PlannedSegment>,
    reply: Option<ReplyMessage>,
    bot_id: &str,
    bot_name: &str,
) -> Vec<Vec<Message>> {
    let mut prefix_messages = Vec::new();
    let mut forward_nodes = Vec::new();

    for segment in segments {
        match segment {
            PlannedSegment::At(at) => prefix_messages.push(Message::At(at)),
            PlannedSegment::Text(text) => forward_nodes.push(ForwardNodeMessage {
                user_id: Some(bot_id.to_string()),
                nickname: Some(bot_name.to_string()),
                id: None,
                content: vec![Message::PlainText(PlainTextMessage { text })],
            }),
            PlannedSegment::Image(image) => forward_nodes.push(ForwardNodeMessage {
                user_id: Some(bot_id.to_string()),
                nickname: Some(bot_name.to_string()),
                id: None,
                content: vec![Message::Image(image)],
            }),
        }
    }

    let needs_carrier = reply.is_some() || !prefix_messages.is_empty();
    let mut batches = Vec::new();

    if needs_carrier {
        let mut carrier = std::mem::take(&mut prefix_messages);

        if let Some(index) = forward_nodes.iter().position(first_node_has_plain_text) {
            let node = forward_nodes.remove(index);
            carrier.extend(node.content);
        } else if !forward_nodes.is_empty() {
            let node = forward_nodes.remove(0);
            carrier.extend(node.content);
        }

        if let Some(reply) = reply {
            carrier.insert(0, Message::Reply(reply));
        }

        if carrier.is_empty() {
            carrier.push(Message::Reply(ReplyMessage { id: 0, message_source: None }));
        }

        normalize_reply_only_carrier(&mut carrier);
        batches.push(carrier);
    }

    if !forward_nodes.is_empty() {
        batches.push(vec![Message::Forward(ForwardMessage { id: None, content: forward_nodes })]);
    }

    batches
}

fn first_node_has_plain_text(node: &ForwardNodeMessage) -> bool {
    matches!(
        node.content.as_slice(),
        [Message::PlainText(PlainTextMessage { text })] if !text.trim().is_empty()
    )
}

fn normalize_reply_only_carrier(carrier: &mut Vec<Message>) {
    if matches!(carrier.as_slice(), [Message::Reply(_)]) {
        carrier.push(Message::PlainText(PlainTextMessage { text: " ".to_string() }));
    }
}

fn attach_reply_to_first_batch(batches: &mut Vec<Vec<Message>>, reply: Option<ReplyMessage>) {
    let Some(reply) = reply else {
        return;
    };

    if let Some(first_batch) = batches
        .iter_mut()
        .find(|batch| !matches!(batch.as_slice(), [Message::Forward(_)]))
    {
        first_batch.insert(0, Message::Reply(reply));
        normalize_reply_only_carrier(first_batch);
        return;
    }

    batches.push(vec![
        Message::Reply(reply),
        Message::PlainText(PlainTextMessage { text: " ".to_string() }),
    ]);
}

pub(crate) fn ensure_space_after_at(batches: &mut [Vec<Message>]) {
    for batch in batches {
        for index in 0..batch.len().saturating_sub(1) {
            if matches!(batch[index], Message::At(_)) {
                if let Message::PlainText(text) = &mut batch[index + 1] {
                    if !text.text.starts_with(' ') {
                        text.text.insert(0, ' ');
                    }
                }
            }
        }
    }
}

pub(crate) fn flush_batch(batches: &mut Vec<Vec<Message>>, current_batch: &mut Vec<Message>) {
    if !current_batch.is_empty() {
        batches.push(std::mem::take(current_batch));
    }
}

pub(crate) fn build_forward_message_from_chunks(
    chunks: Vec<String>,
    bot_id: &str,
    bot_name: &str,
) -> Result<ForwardMessage> {
    let nodes: Vec<ForwardNodeMessage> = chunks
        .into_iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .map(|chunk| ForwardNodeMessage {
            user_id: Some(bot_id.to_string()),
            nickname: Some(bot_name.to_string()),
            id: None,
            content: vec![Message::PlainText(PlainTextMessage { text: chunk })],
        })
        .collect();

    if nodes.is_empty() {
        return Err(Error::ValidationError("forward content must not be blank".to_string()));
    }

    Ok(ForwardMessage { id: None, content: nodes })
}

pub(crate) fn split_text_by_semantic_boundaries(content: &str, max_chars: usize) -> Vec<String> {
    PunctuationSegmenter.segment(content, max_chars)
}

pub(crate) fn split_plain_text_for_forward(
    text: &str,
    max_chars: usize,
    segmenter: &dyn TextSegmenter,
) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return Vec::new();
    }

    let raw_chunks = segmenter.segment(trimmed, max_chars);
    let mut carry_prefix = String::new();
    let mut state = SplitRepairState::default();
    let mut chunks = Vec::new();

    for (index, raw_chunk) in raw_chunks.iter().enumerate() {
        let mut chunk = carry_prefix.clone();
        chunk.push_str(raw_chunk);
        let analysis = analyze_chunk_state(&chunk, state);
        let has_more = index + 1 < raw_chunks.len();
        let mut finalized = chunk.trim().to_string();
        carry_prefix.clear();

        if has_more {
            if analysis.in_code_fence {
                finalized.push_str("\n```");
                carry_prefix.push_str("```\n");
            }
            if analysis.in_cn_quote {
                finalized.push('”');
                carry_prefix.push('“');
            }
            if analysis.in_double_quote {
                finalized.push('"');
                carry_prefix.push('"');
            }
        }

        let finalized = finalized.trim().to_string();
        if !finalized.is_empty() {
            chunks.push(finalized);
        }
        state = analysis;
    }

    chunks
}

fn analyze_chunk_state(chunk: &str, mut state: SplitRepairState) -> SplitRepairState {
    let mut iter = chunk.chars().peekable();
    while let Some(ch) = iter.next() {
        if ch == '`' {
            let mut count = 1usize;
            while matches!(iter.peek(), Some('`')) {
                iter.next();
                count += 1;
            }
            if count >= 3 {
                state.in_code_fence = !state.in_code_fence;
                continue;
            }
        }

        if state.in_code_fence {
            continue;
        }

        match ch {
            '"' => state.in_double_quote = !state.in_double_quote,
            '“' => state.in_cn_quote = true,
            '”' => state.in_cn_quote = false,
            _ => {}
        }
    }
    state
}

fn parse_reply_segments(text: &str) -> Vec<ReplySegment> {
    let mut segments = Vec::new();
    let mut buffer = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '@' && is_mention_prefix_boundary(&chars, index) {
            if let Some((target, next_index)) = parse_at_segment(&chars, index) {
                push_text_segment(&mut segments, &mut buffer);
                segments.push(ReplySegment::At(target));
                index = next_index;
                continue;
            }
        }

        if chars[index] == '[' {
            if let Some((segment, next_index)) = parse_bracket_segment(&chars, index) {
                push_text_segment(&mut segments, &mut buffer);
                segments.push(segment);
                index = next_index;
                continue;
            }
        }

        buffer.push(chars[index]);
        index += 1;
    }

    push_text_segment(&mut segments, &mut buffer);
    merge_adjacent_text_segments(segments)
}

fn push_text_segment(segments: &mut Vec<ReplySegment>, buffer: &mut String) {
    if !buffer.is_empty() {
        segments.push(ReplySegment::Text(std::mem::take(buffer)));
    }
}

fn merge_adjacent_text_segments(segments: Vec<ReplySegment>) -> Vec<ReplySegment> {
    let mut merged = Vec::new();
    for segment in segments {
        match segment {
            ReplySegment::Text(text) => {
                if let Some(ReplySegment::Text(last)) = merged.last_mut() {
                    last.push_str(&text);
                } else {
                    merged.push(ReplySegment::Text(text));
                }
            }
            other => merged.push(other),
        }
    }
    merged
}

fn is_mention_prefix_boundary(chars: &[char], index: usize) -> bool {
    if index == 0 {
        return true;
    }

    !chars[index - 1].is_ascii_alphanumeric()
}

fn parse_bracket_segment(chars: &[char], start: usize) -> Option<(ReplySegment, usize)> {
    let mut end = start + 1;
    while end < chars.len() {
        if chars[end] == ']' {
            let inner: String = chars[start + 1..end].iter().collect();
            let inner = inner.trim();
            if is_no_reply_directive(inner) {
                return Some((ReplySegment::NoReply, end + 1));
            }
            if let Some(message) = parse_bracket_message(inner) {
                return Some((message, end + 1));
            }
            return None;
        }
        end += 1;
    }
    None
}

fn parse_bracket_message(inner: &str) -> Option<ReplySegment> {
    let value = inner
        .strip_prefix("Image media_id=")
        .or_else(|| inner.strip_prefix("Image: media_id="))?;
    let media_id = parse_tag_value(value)?;
    Some(ReplySegment::Image(ImageMessage::new(PersistedMedia {
        media_id,
        source: PersistedMediaSource::Upload,
        original_source: String::new(),
        rustfs_path: String::new(),
        name: None,
        description: None,
        mime_type: None,
    })))
}
