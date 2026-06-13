use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zihuan_core::data_refs::{MySqlConfig, RelationalDbConnection};

use ims_bot_adapter::adapter::SharedBotAdapter;
use ims_bot_adapter::message_helpers::{
    get_bot_id, send_friend_batches_with_persistence, send_group_batches_with_persistence, OutboundMessagePersistence,
};
use ims_bot_adapter::models::message::{
    AtTargetMessage, ForwardMessage, ForwardNodeMessage, ImageMessage, Message, PersistedMedia, PersistedMediaSource,
    PlainTextMessage, ReplyMessage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zihuan_agent::utils::string_utils::is_no_reply_directive;
use zihuan_core::error::{Error, Result};
use zihuan_core::utils::string_utils::{parse_at_segment, parse_tag_value};
use zihuan_graph_engine::data_value::DataValue;
use zihuan_nlp::{PunctuationSegmenter, TextSegmenter};

pub(crate) use super::qq_chat_agent_service_core::{
    QqChatServiceReplyBatchBuilder, QqChatServiceReplyBuildRequest, QqChatServiceReplyBuildResult,
};
use crate::agent::qq_chat_agent_service_logging::QqChatTaskTrace;
use crate::storage::media::resolve_media_references;

const MAX_FORWARD_NODE_CHARS: usize = 800;
const DEFAULT_NOTIFICATION_TEXT_CHARS: usize = 250;

pub(crate) const QQ_CHAT_REPLY_DIRECTIVE_RUNTIME_KEY: &str = "qq_chat_reply_directive";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum QqChatServiceReplyDirective {
    Explicit { message_id: i64 },
    TriggerMessage,
}

#[derive(Clone)]
pub(crate) struct QqChatServiceSendContext<'a> {
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

pub(crate) fn build_reply_batch_builder(segmenter: Arc<dyn TextSegmenter>) -> QqChatServiceReplyBatchBuilder {
    Arc::new(move |request| {
        let plan = plan_model_reply(request, segmenter.as_ref())?;
        Ok(QqChatServiceReplyBuildResult {
            batches: plan.batches,
            suppress_send: plan.suppress_send,
        })
    })
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

pub(crate) fn plan_model_reply(
    request: &QqChatServiceReplyBuildRequest,
    segmenter: &dyn TextSegmenter,
) -> Result<QqOutboundPlan> {
    let normalized_text = normalize_assistant_text(request);
    let segments = parse_reply_segments(&normalized_text);
    if segments.iter().any(|segment| matches!(segment, ReplySegment::NoReply)) {
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
                for chunk in split_plain_text_for_forward(&text, request.max_message_length, segmenter) {
                    text_chunk_count += 1;
                    expanded_segments.push(PlannedSegment::Text(chunk));
                }
            }
            ReplySegment::At(target) => {
                expanded_segments.push(PlannedSegment::At(AtTargetMessage { target: Some(target) }))
            }
            ReplySegment::Image(image) => {
                image_count += 1;
                expanded_segments.push(PlannedSegment::Image(image));
            }
            ReplySegment::NoReply => {}
        }
    }

    let reply_message = resolve_reply_message(request.reply_directive.as_ref(), request.trigger_message_id);
    let forced_forward = text_chunk_count >= 3 || image_count > 1;
    let mut batches = if forced_forward {
        build_forced_forward_batches(expanded_segments, reply_message, &request.bot_id, &request.bot_name)
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

pub(crate) fn send_planned_batches(ctx: &QqChatServiceSendContext<'_>, batches: &[Vec<Message>]) {
    if ctx.is_group {
        send_group_batches_with_persistence(ctx.adapter, ctx.target_id, batches, &ctx.persistence);
    } else {
        send_friend_batches_with_persistence(ctx.adapter, ctx.target_id, batches, &ctx.persistence);
    }
}

pub(crate) fn send_notification_text(ctx: &QqChatServiceSendContext<'_>, content: &str) -> Result<()> {
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

pub(crate) fn send_forward_content(ctx: &QqChatServiceSendContext<'_>, content: &str) -> Result<()> {
    let text = content.trim();
    if text.is_empty() {
        return Err(Error::ValidationError("forward content must not be blank".to_string()));
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
    let result = if result.is_empty() { "没有结果" } else { result };
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
                let should_append_to_current = current_batch.iter().all(|message| matches!(message, Message::At(_)));
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
            carrier.push(Message::Reply(ReplyMessage { id: 0, message_source: None }));
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
            carrier.push(Message::PlainText(PlainTextMessage { text: first_chunk.clone() }));
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
        } else if current_batch.iter().all(|message| matches!(message, Message::At(_))) {
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

fn resolve_bot_identity<'a>(ctx: &'a QqChatServiceSendContext<'_>) -> (Cow<'a, str>, Cow<'a, str>) {
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

fn build_forward_message_from_chunks(chunks: Vec<String>, bot_id: &str, bot_name: &str) -> Result<ForwardMessage> {
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

fn split_text_by_semantic_boundaries(content: &str, max_chars: usize) -> Vec<String> {
    PunctuationSegmenter.segment(content, max_chars)
}

fn split_plain_text_for_forward(text: &str, max_chars: usize, segmenter: &dyn TextSegmenter) -> Vec<String> {
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
    };

    if let Some(builder) = reply_batch_builder {
        builder(&request)
    } else {
        Err(Error::ValidationError(
            "no reply_batch_builder available for build_reply_result".to_string(),
        ))
    }
}

/// Send a direct text reply to the user, constructing reply batches via the
/// provided `reply_batch_builder` and dispatching them through the QQ adapter.
pub(crate) fn send_direct_text_reply(
    trace: &QqChatTaskTrace,
    adapter: &SharedBotAdapter,
    target_id: &str,
    rdb_pool: Option<&RelationalDbConnection>,
    mysql_ref: Option<&Arc<MySqlConfig>>,
    group_name: Option<&str>,
    bot_name: &str,
    bot_id: &str,
    text: &str,
    is_group: bool,
    sender_id: &str,
    sender_name: &str,
    sender_card: &str,
    max_message_length: usize,
    reply_batch_builder: Option<&QqChatServiceReplyBatchBuilder>,
) -> Result<()> {
    let persistence =
        crate::storage::qq_chat_session_store::build_outbound_persistence(rdb_pool, mysql_ref, group_name, bot_name);

    let reply_result = build_reply_result(
        text,
        is_group,
        sender_id,
        sender_name,
        sender_card,
        bot_id,
        bot_name,
        max_message_length,
        None,
        None,
        HashMap::new(),
        reply_batch_builder,
    )?;

    if reply_result.suppress_send || reply_result.batches.is_empty() {
        trace.record_reply_send(reply_result.suppress_send, true, &reply_result.batches);
        return Ok(());
    }

    trace.record_reply_send(false, false, &reply_result.batches);
    let send_ctx = QqChatServiceSendContext {
        adapter,
        target_id,
        is_group,
        group_name,
        bot_name,
        bot_id,
        mention_target_id: None,
        persistence,
        max_text_chars: max_message_length,
    };
    send_planned_batches(&send_ctx, &reply_result.batches);
    Ok(())
}
