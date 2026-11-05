//! Inbound event processing pipeline.
//!
//! One inbound [`MessageEvent`] flows through [`process_event`]: message persistence →
//! ignore filter → group @-mention filter → rate limit → session claim. A claimed session
//! runs as a turn group (glossary: `turnGroup`) in [`run_turn_group`] — the turn itself
//! plus any steer follow-ups — and each individual turn is executed by
//! [`super::turn_workflow`], which drives the procedure chain. A busy session is handed to
//! [`super::steer::try_handle_busy_session_steer`] instead.

use std::sync::Arc;

use chrono::Local;
use log::{error, info, warn};

use zihuan_core::error::Result;
use zihuan_core::ims_bot_adapter::message_helpers::get_bot_id;
use zihuan_core::ims_bot_adapter::models::event_model::MessageType;
use zihuan_core::ims_bot_adapter::models::message::MessageProp;
use zihuan_core::ims_bot_adapter::models::MessageEvent;
use zihuan_core::runtime::block_async;
use zihuan_core::steer::build_merged_follow_up_event;
use zihuan_core::storage::message::record_write::persist_message_event;
use zihuan_core::task_context::{
    scope_task_id, scope_task_runtime, AgentTaskRequest, AgentTaskResult, AgentTaskStatus,
};
use zihuan_core::utils::string_utils::shorten_text;

use super::core::{
    expand_messages_for_inference, prepare_current_turn_user_input_from_event, QqChatAgentService,
    QqChatAgentServiceContext, QqChatServiceHandleReport, LOG_PREFIX, LOG_TEXT_PREVIEW_CHARS,
};
use super::ignore_store::should_ignore_message_blocking;
use super::logging::QqChatTaskTrace;
use super::msg_send::send_direct_notification_text_reply;
use super::steer::try_handle_busy_session_steer;
use super::turn_workflow::{run_turn, TurnInput};
use crate::qq_chat::resources::current_qq_chat_role_service_config;
use crate::storage::message_rate_limit_store::{
    consume_message_rate_limit_blocking, MessageRateLimitBlockAction, MessageRateLimitCheckResult,
    MESSAGE_RATE_LIMIT_BLOCKED_REPLY,
};
use crate::storage::qq_chat_session_store::{release_session, try_claim_session};

/// Entry point for handling a single inbound QQ message event.
///
/// The flow is:
/// - **Validation** — persists the message and checks ignore rules.
/// - **Group mention filter** — silently drops group messages that do not `@` the bot.
/// - **Session claim** — tries to acquire a per-sender session lock. If the session is busy,
///   the message is enqueued as a steer event instead.
/// - **Task tracking** — starts a runtime task (if available) and builds a [`QqChatTaskTrace`].
/// - **Delegation** — forwards to [`run_turn_group`] for the actual brain loop and reply.
/// - **Cleanup** — releases the session lock, finalizes steer state, and marks the task
///   as completed or failed.
pub(crate) fn process_event(
    service: &QqChatAgentService,
    event: &MessageEvent,
    time: &str,
    user_ip: Option<String>,
    ctx: &QqChatAgentServiceContext<'_>,
) -> Result<()> {
    let agent_id = service.config.agent_id.as_str();
    let session = &service.config.session;
    let is_group = event.message_type == MessageType::Group;
    let sender_id = event.sender.user_id.to_string();
    let target_id = if is_group {
        event
            .group_id
            .ok_or_else(|| service.wrap_err("group_id missing on group message"))?
            .to_string()
    } else {
        sender_id.clone()
    };

    info!(
        "{LOG_PREFIX} Handling {} message: message_id={} sender={} target={}",
        if is_group { "group" } else { "private" },
        event.message_id,
        sender_id,
        target_id
    );

    if let Err(err) = persist_message_event(event, ctx.rdb_pool, None) {
        warn!("{LOG_PREFIX} Message persistence failed: {err}");
    }

    if let Some(rdb_pool) = ctx.rdb_pool {
        let group_id_text = event.group_id.map(|value| value.to_string());
        if should_ignore_message_blocking(rdb_pool, agent_id, &sender_id, group_id_text.as_deref())?
        {
            info!(
                "{LOG_PREFIX} Ignored inbound message: message_id={} sender={} group={:?}",
                event.message_id, sender_id, event.group_id
            );
            return Ok(());
        }
    }

    if is_group {
        let bot_id = get_bot_id(ctx.adapter);
        let msg_prop = MessageProp::from_messages_with_bot_name(
            &event.message_list,
            Some(&bot_id),
            Some(ctx.bot_name),
        );
        if !msg_prop.is_at_me {
            return Ok(());
        }
    }

    let mut message_rate_limit = None;
    if let Some(rdb_pool) = ctx.rdb_pool {
        let group_id_text = event.group_id.map(|value| value.to_string());
        let config = current_qq_chat_role_service_config()?;
        let rate_limit_result = consume_message_rate_limit_blocking(
            rdb_pool,
            agent_id,
            &sender_id,
            group_id_text.as_deref(),
            &config,
        )?;
        if !rate_limit_result.allowed {
            match rate_limit_result.block_action {
                MessageRateLimitBlockAction::ReplyOnce => {
                    let bot_id = get_bot_id(ctx.adapter);
                    let blocked_reply = rate_limit_result
                        .blocked_reply
                        .unwrap_or_else(|| MESSAGE_RATE_LIMIT_BLOCKED_REPLY.to_string());
                    let mention_target_id = (is_group && rate_limit_result.mention_sender_on_block)
                        .then_some(sender_id.as_str());
                    send_direct_notification_text_reply(
                        &QqChatTaskTrace::new(Local::now()),
                        ctx.adapter,
                        &target_id,
                        ctx.rdb_pool,
                        event.group_name.as_deref(),
                        ctx.bot_name,
                        &bot_id,
                        &blocked_reply,
                        is_group,
                        mention_target_id,
                        ctx.max_message_length,
                    )?;
                    info!(
                        "{LOG_PREFIX} Message rate-limited with reply: message_id={} sender={} group={:?}",
                        event.message_id, sender_id, event.group_id
                    );
                }
                MessageRateLimitBlockAction::Silent => {
                    info!(
                        "{LOG_PREFIX} Message rate-limited silently: message_id={} sender={} group={:?}",
                        event.message_id, sender_id, event.group_id
                    );
                }
                MessageRateLimitBlockAction::None => {
                    info!(
                        "{LOG_PREFIX} Message rate-limited without block action: message_id={} sender={} group={:?}",
                        event.message_id, sender_id, event.group_id
                    );
                }
            }
            return Ok(());
        }
        message_rate_limit = Some(rate_limit_result);
    }

    let (claimed, claim_token) = try_claim_session(session, &sender_id);
    if !claimed {
        return try_handle_busy_session_steer(
            service, event, ctx, &sender_id, &target_id, is_group, time,
        );
    }

    ctx.pending_steer.ensure_session_entry(&sender_id);

    let task_created_at = Local::now();
    let task_handle = ctx.task_runtime.as_ref().map(|runtime| {
        runtime.start_task(AgentTaskRequest {
            task_name: format!("回复[{sender_id}]的消息"),
            agent_id: agent_id.to_string(),
            agent_name: ctx.bot_name.to_string(),
            user_ip,
            owner_id: Some(sender_id.to_string()),
            task_db_connection_id: ctx.task_db_connection_id.clone(),
        })
    });
    let trace = QqChatTaskTrace::new(task_created_at);
    // catch_unwind so a panicking turn thread cannot skip the session release below:
    // a leaked claim would turn every later message from this sender into steers
    // that nothing ever drains.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let run = || {
            run_turn_group(
                service,
                &trace,
                event,
                &sender_id,
                &target_id,
                is_group,
                message_rate_limit.as_ref(),
                ctx,
            )
        };
        match (task_handle.as_ref(), ctx.task_runtime.as_ref()) {
            (Some(task_handle), Some(task_runtime)) => {
                scope_task_runtime(Arc::clone(task_runtime), || {
                    scope_task_id(task_handle.task_id.clone(), run)
                })
            }
            (Some(task_handle), None) => scope_task_id(task_handle.task_id.clone(), run),
            (None, _) => run(),
        }
    }));
    let result = match result {
        Ok(result) => result,
        Err(payload) => {
            let panic_message = panic_payload_message(payload);
            error!("{LOG_PREFIX} turn task panicked for sender {sender_id}: {panic_message}");
            Err(zihuan_core::string_error!("turn task panicked: {panic_message}"))
        }
    };
    trace.finish_with_summary();

    if let Some(task_handle) = task_handle.as_ref() {
        let graph = trace.graph_definition(&task_handle.task_id);
        let graph_value = serde_json::to_value(&graph).unwrap_or(serde_json::Value::Null);
        let object_path = ctx.s3_ref.as_ref().and_then(|s3_ref| {
            let key = format!(
                "qq-chat-task-graphs/{}/{}.json",
                Local::now().format("%Y/%m/%d"),
                task_handle.task_id
            );
            let bytes = serde_json::to_vec(&graph).ok()?;
            match block_async(s3_ref.put_object(&key, "application/json", &bytes)) {
                Ok(_) => Some(format!("rustfs://{key}")),
                Err(err) => {
                    warn!(
                        "[QqChatAgentService] failed to persist task graph {}: {err}",
                        task_handle.task_id
                    );
                    None
                }
            }
        });
        if let Some(runtime) = ctx.task_runtime.as_ref() {
            runtime.set_task_graph(&task_handle.task_id, graph_value, object_path);
        }
    }

    release_session(session, &sender_id, claim_token);
    ctx.pending_steer.finish_session(&sender_id);
    if let Some(task_handle) = task_handle {
        match &result {
            Ok(report) => task_handle.finish(AgentTaskResult {
                status: Some(AgentTaskStatus::Success),
                result_summary: Some(report.result_summary.clone()),
                error_message: None,
            }),
            Err(err) => task_handle.finish(AgentTaskResult {
                status: Some(AgentTaskStatus::Failed),
                result_summary: Some(format!("回复[{sender_id}]失败: {err}")),
                error_message: Some(err.to_string()),
            }),
        }
    }
    result.map(|_| ())
}

/// Runs one claimed session's turn group (glossary: `turnGroup`).
///
/// The first pass handles the user's message; every later pass replays queued steer
/// messages and is marked as a continuation in the context view. Each pass is one turn
/// executed by [`super::turn_workflow::run_turn`]. Steers are aggregated per sender, so
/// a pass may replay messages from either conversation of that sender.
fn run_turn_group(
    service: &QqChatAgentService,
    trace: &QqChatTaskTrace,
    event: &MessageEvent,
    sender_id: &str,
    target_id: &str,
    is_group: bool,
    message_rate_limit: Option<&MessageRateLimitCheckResult>,
    ctx: &QqChatAgentServiceContext<'_>,
) -> Result<QqChatServiceHandleReport> {
    (|| -> Result<QqChatServiceHandleReport> {
        let bot_id = get_bot_id(ctx.adapter);
        let mut current_event = event.clone();
        // The first pass handles the user's message; every later pass replays queued steer
        // messages and is marked as a continuation in the context view.
        let mut is_steer_follow_up = false;
        // The steer events that were merged into the current pass's message. When that
        // pass fails they must go back to the pending store, or they would be lost.
        let mut current_pass_steers = Vec::new();
        let result_summary = loop {
            let turn_input = TurnInput {
                trace,
                event: &current_event,
                sender_id,
                target_id,
                is_group,
                bot_id: &bot_id,
                message_rate_limit,
                continues_previous: is_steer_follow_up,
                ctx,
            };
            let turn_result = run_turn(service, &turn_input)?;

            if !turn_result.turn_completed {
                // The turn failed before sending any reply. Steers the hook consumed
                // mid-turn were already rolled back by the commit guard; put this
                // pass's own merged steer events back too, then stop — chaining
                // another turn onto a persistent failure would just loop.
                ctx.pending_steer.requeue_front(sender_id, current_pass_steers);
                break turn_result.result_summary;
            }

            let (pending, remaining_queue_len, accepted_steer_count) =
                ctx.pending_steer.drain_all(sender_id);
            if pending.is_empty() {
                break turn_result.result_summary;
            }

            let steer_count = pending.len();
            let next_event = build_merged_follow_up_event(&pending);
            let mut next_inference_event = next_event.clone();
            next_inference_event.message_list =
                expand_messages_for_inference(&next_event.message_list);
            let next_message = prepare_current_turn_user_input_from_event(
                &next_inference_event,
                &bot_id,
                ctx.bot_name,
                ctx.s3_ref,
            )
            .current_text_for_prompt()
            .to_string();
            trace.record_steer_follow_up(
                next_event.message_id,
                steer_count,
                accepted_steer_count,
                ctx.max_steer_count,
                &next_message,
            );
            info!(
                "{LOG_PREFIX} steer follow-up picked for sender={} message_id={} steer_count={} remaining_queue_len={} accepted_steer_count={}/{} message={}",
                sender_id,
                next_event.message_id,
                steer_count,
                remaining_queue_len,
                accepted_steer_count,
                ctx.max_steer_count,
                shorten_text(&next_message, LOG_TEXT_PREVIEW_CHARS)
            );
            current_event = next_event;
            current_pass_steers = pending;
            is_steer_follow_up = true;
        };

        Ok(QqChatServiceHandleReport { result_summary })
    })()
}

/// Stringify a panic payload for logging; payloads are usually `&str` or `String`.
fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "unknown panic payload".to_string()
    }
}
