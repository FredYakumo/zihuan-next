//! Outbound delivery orchestration for the QQ chat service: everything that happens after
//! the reply has been formatted into message batches (see
//! [`crate::procedure::utils::reply_plan`]) — sending
//! batches through the bot adapter with human-paced delays, plus the notification and
//! forward-content shortcuts that bypass the brain.

use std::borrow::Cow;
use std::collections::HashMap;

use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_core::ims_bot_adapter::adapter::SharedBotAdapter;
use zihuan_core::ims_bot_adapter::message_helpers::{
    get_bot_id, send_friend_batches_with_persistence_and_delays,
    send_group_batches_with_persistence_and_delays, OutboundMessagePersistence,
};
use zihuan_core::ims_bot_adapter::models::message::{AtTargetMessage, Message, PlainTextMessage};
use zihuan_core::nlp::{PunctuationSegmenter, TextSegmenter};

use crate::qq_chat::logging::QqChatTaskTrace;

use super::model::QqChatServiceReplyBatchBuilder;
use crate::procedure::utils::reply_plan::{
    build_forward_message_from_chunks, build_reply_result, ensure_space_after_at, flush_batch,
    reply_batch_delays_ms, split_plain_text_for_forward, split_text_by_semantic_boundaries,
    MAX_FORWARD_NODE_CHARS,
};

const DEFAULT_NOTIFICATION_TEXT_CHARS: usize = 250;

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

pub(crate) fn send_planned_batches(ctx: &QqChatServiceSendContext<'_>, batches: &[Vec<Message>]) {
    // Human-paced dispatch: every follow-up bubble waits for the typing time of its own
    // text, so a multi-message reply does not land as one instant burst.
    let delays_ms = reply_batch_delays_ms(batches);
    if ctx.is_group {
        send_group_batches_with_persistence_and_delays(
            ctx.adapter,
            ctx.target_id,
            batches,
            &delays_ms,
            &ctx.persistence,
        );
    } else {
        send_friend_batches_with_persistence_and_delays(
            ctx.adapter,
            ctx.target_id,
            batches,
            &delays_ms,
            &ctx.persistence,
        );
    }
}

pub(crate) fn send_notification_text(
    ctx: &QqChatServiceSendContext<'_>,
    content: &str,
) -> Result<()> {
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

pub(crate) fn send_forward_content(
    ctx: &QqChatServiceSendContext<'_>,
    content: &str,
) -> Result<()> {
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
            carrier.push(Message::At(AtTargetMessage { target: Some(target.to_string()) }));
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
        current_batch.push(Message::At(AtTargetMessage { target: Some(target.to_string()) }));
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

/// Send a direct text reply to the user, constructing reply batches via the
/// provided `reply_batch_builder` and dispatching them through the QQ adapter.
pub(crate) fn send_direct_text_reply(
    trace: &QqChatTaskTrace,
    adapter: &SharedBotAdapter,
    target_id: &str,
    rdb_pool: Option<&RelationalDbConnection>,
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
    let persistence = crate::storage::qq_chat_session_store::build_outbound_persistence(
        rdb_pool, group_name, bot_name,
    );
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
        rdb_pool.cloned(),
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

pub(crate) fn send_direct_notification_text_reply(
    trace: &QqChatTaskTrace,
    adapter: &SharedBotAdapter,
    target_id: &str,
    rdb_pool: Option<&RelationalDbConnection>,
    group_name: Option<&str>,
    bot_name: &str,
    bot_id: &str,
    text: &str,
    is_group: bool,
    mention_target_id: Option<&str>,
    max_message_length: usize,
) -> Result<()> {
    let persistence = crate::storage::qq_chat_session_store::build_outbound_persistence(
        rdb_pool, group_name, bot_name,
    );
    let send_ctx = QqChatServiceSendContext {
        adapter,
        target_id,
        is_group,
        group_name,
        bot_name,
        bot_id,
        mention_target_id,
        persistence,
        max_text_chars: max_message_length,
    };
    send_notification_text(&send_ctx, text)?;
    trace.record_reply_send(false, false, &[]);
    Ok(())
}
