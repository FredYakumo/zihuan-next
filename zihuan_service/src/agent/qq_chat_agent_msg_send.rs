use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ims_bot_adapter::adapter::SharedBotAdapter;
use ims_bot_adapter::message_helpers::{
    get_bot_id, send_friend_batches_with_persistence, send_group_batches_with_persistence,
    OutboundMessagePersistence,
};
use ims_bot_adapter::models::message::{
    AtTargetMessage, ForwardMessage, ForwardNodeMessage, ImageMessage, Message, PersistedMedia,
    PersistedMediaSource, PlainTextMessage, ReplyMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zihuan_core::error::{Error, Result};
use zihuan_core::utils::string_utils::parse_tag_value;
use zihuan_agent::utils::string_utils::is_no_reply_directive;
use zihuan_graph_engine::data_value::DataValue;
use zihuan_graph_engine::message_restore::restore_media_by_id;
use zihuan_nlp::{PunctuationSegmenter, TextSegmenter};

use super::qq_chat_agent_core::{
    QqAgentReplyBatchBuilder, QqAgentReplyBuildRequest, QqAgentReplyBuildResult,
};

const MAX_FORWARD_NODE_CHARS: usize = 800;
const DEFAULT_NOTIFICATION_TEXT_CHARS: usize = 250;

pub(crate) const QQ_CHAT_REPLY_DIRECTIVE_RUNTIME_KEY: &str = "qq_chat_reply_directive";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum QqReplyDirective {
    Explicit { message_id: i64 },
    TriggerMessage,
}

#[derive(Clone)]
pub(crate) struct QqSendContext<'a> {
    pub adapter: &'a SharedBotAdapter,
    pub target_id: &'a str,
    pub is_group: bool,
    pub group_name: Option<&'a str>,
    pub bot_id: &'a str,
    pub bot_name: &'a str,
    pub mention_target_id: Option<&'a str>,
    pub persistence: OutboundMessagePersistence,
    pub max_text_chars: usize,
}

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
) -> QqAgentReplyBatchBuilder {
    Arc::new(move |request| {
        let plan = plan_model_reply(request, segmenter.as_ref())?;
        Ok(QqAgentReplyBuildResult {
            batches: plan.batches,
            suppress_send: plan.suppress_send,
        })
    })
}

pub(crate) fn store_reply_directive(
    shared_runtime_values: &Arc<Mutex<HashMap<String, DataValue>>>,
    directive: QqReplyDirective,
) {
    shared_runtime_values.lock().unwrap().insert(
        QQ_CHAT_REPLY_DIRECTIVE_RUNTIME_KEY.to_string(),
        DataValue::Json(serde_json::to_value(directive).unwrap_or(Value::Null)),
    );
}

pub(crate) fn take_reply_directive(
    shared_runtime_values: &Arc<Mutex<HashMap<String, DataValue>>>,
) -> Option<QqReplyDirective> {
    let value = shared_runtime_values
        .lock()
        .unwrap()
        .remove(QQ_CHAT_REPLY_DIRECTIVE_RUNTIME_KEY)?;
    match value {
        DataValue::Json(value) => serde_json::from_value(value).ok(),
        _ => None,
    }
}

pub(crate) fn plan_model_reply(
    request: &QqAgentReplyBuildRequest,
    segmenter: &dyn TextSegmenter,
) -> Result<QqOutboundPlan> {
    let normalized_text = normalize_assistant_text(request);
    let segments = parse_reply_segments(&normalized_text);
    if segments
        .iter()
        .any(|segment| matches!(segment, ReplySegment::NoReply))
    {
        return Ok(QqOutboundPlan {
            batches: Vec::new(),
            suppress_send: true,
            visible_text: None,
        });
    }

    let mut expanded_segments = Vec::new();
    let mut text_chunk_count = 0usize;
    let mut image_count = 0usize;

    for segment in segments {
        match segment {
            ReplySegment::Text(text) => {
                for chunk in
                    split_plain_text_for_forward(&text, request.max_message_length, segmenter)
                {
                    text_chunk_count += 1;
                    expanded_segments.push(PlannedSegment::Text(chunk));
                }
            }
            ReplySegment::At(target) => {
                expanded_segments.push(PlannedSegment::At(AtTargetMessage {
                    target: Some(target),
                }))
            }
            ReplySegment::Image(image) => {
                image_count += 1;
                expanded_segments.push(PlannedSegment::Image(image));
            }
            ReplySegment::NoReply => {}
        }
    }

    let reply_message =
        resolve_reply_message(request.reply_directive.as_ref(), request.trigger_message_id);
    let forced_forward = text_chunk_count >= 3 || image_count > 1;
    let mut batches = if forced_forward {
        build_forced_forward_batches(
            expanded_segments,
            reply_message,
            &request.bot_id,
            &request.bot_name,
        )
    } else {
        build_regular_batches(expanded_segments, reply_message, request.max_message_length)
    };

    ensure_space_after_at(&mut batches);
    resolve_media_references(&mut batches, &request.available_media)?;

    Ok(QqOutboundPlan {
        batches,
        suppress_send: false,
        visible_text: Some(normalized_text),
    })
}

pub(crate) fn send_planned_batches(ctx: &QqSendContext<'_>, batches: &[Vec<Message>]) {
    if ctx.is_group {
        send_group_batches_with_persistence(ctx.adapter, ctx.target_id, batches, &ctx.persistence);
    } else {
        send_friend_batches_with_persistence(ctx.adapter, ctx.target_id, batches, &ctx.persistence);
    }
}

pub(crate) fn send_notification_text(ctx: &QqSendContext<'_>, content: &str) -> Result<()> {
    let text = content.trim();
    if text.is_empty() {
        return Ok(());
    }

    let (bot_id, bot_name) = resolve_bot_identity(ctx);
    let batches = build_notification_batches(
        text,
        ctx.mention_target_id,
        ctx.max_text_chars.max(DEFAULT_NOTIFICATION_TEXT_CHARS),
        bot_id.as_ref(),
        bot_name.as_ref(),
        &PunctuationSegmenter,
    )?;
    if !batches.is_empty() {
        send_planned_batches(ctx, &batches);
    }
    Ok(())
}

pub(crate) fn send_forward_content(ctx: &QqSendContext<'_>, content: &str) -> Result<()> {
    let text = content.trim();
    if text.is_empty() {
        return Err(Error::ValidationError(
            "forward content must not be blank".to_string(),
        ));
    }

    let (bot_id, bot_name) = resolve_bot_identity(ctx);
    let forward = build_forward_message_from_chunks(
        split_text_by_semantic_boundaries(text, MAX_FORWARD_NODE_CHARS),
        bot_id.as_ref(),
        bot_name.as_ref(),
    )?;
    send_planned_batches(ctx, &[vec![Message::Forward(forward)]]);
    Ok(())
}

pub(crate) fn build_long_task_start_text(task_id: &str, call_content: &str) -> String {
    let content = call_content.trim();
    if content.is_empty() {
        format!("正在执行长时任务\n可使用 /task {task_id} 查看进度。")
    } else {
        format!("{content}\n可使用 /task {task_id} 查看进度。")
    }
}

pub(crate) fn build_long_task_complete_content(
    task_id: &str,
    task_name: &str,
    progress: &[String],
    result: &str,
) -> String {
    let result = result.trim();
    let result = if result.is_empty() {
        "没有结果"
    } else {
        result
    };
    let mut content = format!("\n任务: {task_name}({task_id})");
    if !progress.is_empty() {
        content.push_str("\n\n");
        for (index, item) in progress.iter().enumerate() {
            content.push_str(&format!("\n{}. {}", index + 1, item));
        }
    }
    content.push_str(&format!("\n\n{result}"));
    content
}

fn normalize_assistant_text(request: &QqAgentReplyBuildRequest) -> String {
    if request.is_group {
        request
            .assistant_text
            .replace("@sender", &format!("@{}", request.sender_id))
    } else {
        request.assistant_text.clone()
    }
}

fn resolve_reply_message(
    directive: Option<&QqReplyDirective>,
    trigger_message_id: Option<i64>,
) -> Option<ReplyMessage> {
    let message_id = match directive {
        Some(QqReplyDirective::Explicit { message_id }) => Some(*message_id),
        Some(QqReplyDirective::TriggerMessage) => trigger_message_id,
        None => None,
    }?;

    Some(ReplyMessage {
        id: message_id,
        message_source: None,
    })
}

fn build_regular_batches(
    segments: Vec<PlannedSegment>,
    reply: Option<ReplyMessage>,
    max_message_length: usize,
) -> Vec<Vec<Message>> {
    let mut batches = Vec::new();
    let mut current_batch = Vec::new();
    let mut current_text_chars = 0usize;

    for segment in segments {
        match segment {
            PlannedSegment::At(at) => current_batch.push(Message::At(at)),
            PlannedSegment::Text(text) => {
                let text_chars = text.chars().count();
                if current_text_chars > 0 && current_text_chars + text_chars > max_message_length {
                    flush_batch(&mut batches, &mut current_batch);
                    current_text_chars = 0;
                }
                current_text_chars += text_chars;
                current_batch.push(Message::PlainText(PlainTextMessage { text }));
            }
            PlannedSegment::Image(image) => {
                let should_append_to_current = current_batch
                    .iter()
                    .all(|message| matches!(message, Message::At(_)));
                if should_append_to_current && !current_batch.is_empty() {
                    current_batch.push(Message::Image(image));
                    flush_batch(&mut batches, &mut current_batch);
                    current_text_chars = 0;
                } else {
                    flush_batch(&mut batches, &mut current_batch);
                    current_text_chars = 0;
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
            carrier.push(Message::Reply(ReplyMessage {
                id: 0,
                message_source: None,
            }));
        }

        normalize_reply_only_carrier(&mut carrier);
        batches.push(carrier);
    }

    if !forward_nodes.is_empty() {
        batches.push(vec![Message::Forward(ForwardMessage {
            id: None,
            content: forward_nodes,
        })]);
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
        carrier.push(Message::PlainText(PlainTextMessage {
            text: " ".to_string(),
        }));
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
        Message::PlainText(PlainTextMessage {
            text: " ".to_string(),
        }),
    ]);
}

fn build_notification_batches(
    text: &str,
    mention_target_id: Option<&str>,
    max_message_length: usize,
    bot_id: &str,
    bot_name: &str,
    segmenter: &dyn TextSegmenter,
) -> Result<Vec<Vec<Message>>> {
    let chunks = split_plain_text_for_forward(text, max_message_length, segmenter);
    if chunks.is_empty() {
        return Ok(Vec::new());
    }

    if chunks.len() >= 3 {
        let mut batches = Vec::new();
        let mut carrier = Vec::new();
        if let Some(target) = mention_target_id {
            carrier.push(Message::At(AtTargetMessage {
                target: Some(target.to_string()),
            }));
        }
        if let Some(first_chunk) = chunks.first() {
            carrier.push(Message::PlainText(PlainTextMessage {
                text: first_chunk.clone(),
            }));
        }
        if !carrier.is_empty() {
            batches.push(carrier);
        }

        let forward_chunks = if mention_target_id.is_some() {
            chunks.into_iter().skip(1).collect::<Vec<_>>()
        } else {
            chunks
        };
        if !forward_chunks.is_empty() {
            batches.push(vec![Message::Forward(build_forward_message_from_chunks(
                forward_chunks,
                bot_id,
                bot_name,
            )?)]);
        }
        return Ok(batches);
    }

    let mut current_batch = Vec::new();
    if let Some(target) = mention_target_id {
        current_batch.push(Message::At(AtTargetMessage {
            target: Some(target.to_string()),
        }));
    }
    let mut batches = Vec::new();
    for chunk in chunks {
        if current_batch.is_empty() {
            current_batch.push(Message::PlainText(PlainTextMessage { text: chunk }));
        } else if current_batch
            .iter()
            .all(|message| matches!(message, Message::At(_)))
        {
            current_batch.push(Message::PlainText(PlainTextMessage { text: chunk }));
        } else {
            flush_batch(&mut batches, &mut current_batch);
            current_batch.push(Message::PlainText(PlainTextMessage { text: chunk }));
        }
    }
    flush_batch(&mut batches, &mut current_batch);
    ensure_space_after_at(&mut batches);
    Ok(batches)
}

fn resolve_bot_identity<'a>(ctx: &'a QqSendContext<'_>) -> (Cow<'a, str>, Cow<'a, str>) {
    let bot_id = if ctx.bot_id.trim().is_empty() {
        Cow::Owned(get_bot_id(ctx.adapter))
    } else {
        Cow::Borrowed(ctx.bot_id)
    };
    let bot_name = if ctx.bot_name.trim().is_empty() {
        Cow::Owned(resolve_bot_name(ctx.adapter, bot_id.as_ref()))
    } else {
        Cow::Borrowed(ctx.bot_name)
    };
    (bot_id, bot_name)
}

fn resolve_bot_name(adapter: &SharedBotAdapter, fallback: &str) -> String {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| {
            let guard = handle.block_on(adapter.lock());
            guard
                .get_bot_profile()
                .map(|profile| profile.nickname.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| fallback.to_string())
        })
    } else {
        let guard = adapter.blocking_lock();
        guard
            .get_bot_profile()
            .map(|profile| profile.nickname.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback.to_string())
    }
}

fn resolve_media_references(
    batches: &mut [Vec<Message>],
    available_media: &HashMap<String, PersistedMedia>,
) -> Result<()> {
    for batch in batches {
        for message in batch {
            resolve_message_media_reference(message, available_media)?;
        }
    }
    Ok(())
}

fn resolve_message_media_reference(
    message: &mut Message,
    available_media: &HashMap<String, PersistedMedia>,
) -> Result<()> {
    match message {
        Message::Image(image) => {
            if image.rustfs_path().is_some() || image.original_source().is_some() {
                return Ok(());
            }

            let media_id = image.media.media_id.trim();
            if media_id.is_empty() {
                return Err(Error::ValidationError(
                    "outbound image marker is missing media_id".to_string(),
                ));
            }

            if let Some(media) = available_media.get(media_id) {
                image.media = media.clone();
                return Ok(());
            }

            if let Some(media) = restore_media_by_id(media_id)? {
                image.media = media;
                return Ok(());
            }

            Err(Error::ValidationError(format!(
                "failed to resolve outbound image media_id '{}'",
                media_id
            )))
        }
        Message::Forward(forward) => {
            for node in &mut forward.content {
                for nested in &mut node.content {
                    resolve_message_media_reference(nested, available_media)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn ensure_space_after_at(batches: &mut [Vec<Message>]) {
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

fn flush_batch(batches: &mut Vec<Vec<Message>>, current_batch: &mut Vec<Message>) {
    if !current_batch.is_empty() {
        batches.push(std::mem::take(current_batch));
    }
}

fn build_forward_message_from_chunks(
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
        return Err(Error::ValidationError(
            "forward content must not be blank".to_string(),
        ));
    }

    Ok(ForwardMessage {
        id: None,
        content: nodes,
    })
}

fn split_text_by_semantic_boundaries(content: &str, max_chars: usize) -> Vec<String> {
    PunctuationSegmenter.segment(content, max_chars)
}

fn split_plain_text_for_forward(
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

fn parse_at_segment(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut end = start + 1;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }

    if end == start + 1 {
        return None;
    }

    if end < chars.len() {
        let boundary = chars[end];
        if !boundary.is_whitespace()
            && !matches!(
                boundary,
                ',' | '，' | '。' | ':' | '：' | '!' | '！' | '?' | '？' | ')' | '）' | ']' | '】'
            )
        {
            return None;
        }
    }

    Some((chars[start + 1..end].iter().collect(), end))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request(text: &str) -> QqAgentReplyBuildRequest {
        QqAgentReplyBuildRequest {
            assistant_text: text.to_string(),
            is_group: true,
            sender_id: "123456".to_string(),
            sender_nickname: "tester".to_string(),
            sender_card: String::new(),
            bot_id: "999".to_string(),
            bot_name: "bot".to_string(),
            max_message_length: 4,
            trigger_message_id: Some(42),
            available_media: HashMap::new(),
            reply_directive: None,
        }
    }

    #[test]
    fn parse_bracket_message_supports_media_id_marker() {
        let parsed = parse_bracket_message("Image media_id=media-123").expect("parse image tag");
        match parsed {
            ReplySegment::Image(image) => {
                assert_eq!(image.media.media_id, "media-123");
            }
            other => panic!("expected image segment, got {other:?}"),
        }
    }

    #[test]
    fn three_text_chunks_become_forward() {
        let plan = plan_model_reply(
            &sample_request("第一段。第二段。第三段。"),
            &PunctuationSegmenter,
        )
        .expect("plan reply");
        assert!(matches!(
            plan.batches.as_slice(),
            [batch] if matches!(batch.as_slice(), [Message::Forward(_)])
        ));
    }

    #[test]
    fn multi_image_becomes_forward() {
        let mut request = sample_request("[Image media_id=a][Image media_id=b]");
        request.available_media = HashMap::from([
            (
                "a".to_string(),
                PersistedMedia {
                    media_id: "a".to_string(),
                    source: PersistedMediaSource::Upload,
                    original_source: "https://example.com/a.png".to_string(),
                    rustfs_path: String::new(),
                    name: None,
                    description: None,
                    mime_type: None,
                },
            ),
            (
                "b".to_string(),
                PersistedMedia {
                    media_id: "b".to_string(),
                    source: PersistedMediaSource::Upload,
                    original_source: "https://example.com/b.png".to_string(),
                    rustfs_path: String::new(),
                    name: None,
                    description: None,
                    mime_type: None,
                },
            ),
        ]);

        let plan = plan_model_reply(&request, &PunctuationSegmenter).expect("plan reply");
        assert!(matches!(
            plan.batches.as_slice(),
            [batch] if matches!(batch.as_slice(), [Message::Forward(_)])
        ));
    }

    #[test]
    fn reply_directive_extracts_first_text_outside_forward() {
        let mut request = sample_request("@sender 第一段。第二段。第三段。");
        request.reply_directive = Some(QqReplyDirective::TriggerMessage);

        let plan = plan_model_reply(&request, &PunctuationSegmenter).expect("plan reply");
        assert_eq!(plan.batches.len(), 2);
        assert!(matches!(plan.batches[0].first(), Some(Message::Reply(_))));
        assert!(plan.batches[0]
            .iter()
            .any(|message| matches!(message, Message::At(_))));
        assert!(plan.batches[0]
            .iter()
            .any(|message| matches!(message, Message::PlainText(_))));
        assert!(matches!(plan.batches[1].as_slice(), [Message::Forward(_)]));
    }
}
