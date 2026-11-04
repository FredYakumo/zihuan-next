//! Turn-lifecycle steps shared by the normal and meta-query reply paths: reply delivery,
//! the context-view stages around it, session-state write-back, and scheduler re-arm.
//! History construction and the `TurnFinished` event differ per path and stay there.

use std::sync::{Arc, Mutex};

use zihuan_core::ims_bot_adapter::models::MessageEvent;

use super::super::core::QqChatAgentServiceContext;
use super::super::logging::QqChatTaskTrace;
use crate::procedure::{QqFormatOutputOutput, QqReplyReviewResult};
use crate::qq_chat::msg_send::{send_planned_batches, QqChatServiceSendContext};
use crate::qq_session_state::QqChatSessionState;

/// The Rethink context-view stage, emitted identically by both reply paths.
pub(crate) fn record_rethink_stage(
    ctx: &QqChatAgentServiceContext<'_>,
    sender_id: &str,
    turn_id: &str,
    candidate: Option<&str>,
    review_result: &QqReplyReviewResult,
) {
    zihuan_core::qq_context::emit_qq_context_stage(
        ctx.agent_id,
        sender_id,
        turn_id,
        "Rethink",
        serde_json::json!({
            "safe": review_result.safe,
            "rewritten": review_result.rewritten,
            "reason": review_result.reason,
            "candidate": candidate,
            "final_message": review_result.final_message,
        }),
    );
}

/// Sends the format-output batches (unless suppressed or empty) and emits the 发送回复
/// context-view stage.
///
/// Returns `(sent_batches, explicit_no_reply, visible_assistant_history_text)`:
/// `explicit_no_reply` is true when the format-output procedure chose `[no_reply]`, and the
/// visible text is what the reply path appends to the persisted history.
#[allow(clippy::too_many_arguments)]
pub(crate) fn deliver_reply(
    trace: &QqChatTaskTrace,
    ctx: &QqChatAgentServiceContext<'_>,
    event: &MessageEvent,
    sender_id: &str,
    turn_id: &str,
    target_id: &str,
    is_group: bool,
    bot_id: &str,
    format_output: &QqFormatOutputOutput,
) -> (usize, bool, Option<String>) {
    trace.mark_reply_send_started();
    let mut sent_batches = 0usize;
    let mut explicit_no_reply = false;
    let mut visible_assistant_history_text = None;
    if format_output.suppress_send {
        explicit_no_reply = true;
        trace.record_reply_send(true, false, &format_output.batches);
    } else if format_output.batches.is_empty() {
        trace.record_reply_send(false, false, &format_output.batches);
    } else {
        let send_ctx = QqChatServiceSendContext {
            adapter: ctx.adapter,
            target_id,
            is_group,
            group_name: event.group_name.as_deref(),
            bot_id,
            bot_name: ctx.bot_name,
            mention_target_id: None,
            persistence: crate::storage::qq_chat_session_store::build_outbound_persistence(
                ctx.rdb_pool,
                event.group_name.as_deref(),
                ctx.bot_name,
            ),
            max_text_chars: ctx.max_message_length,
        };
        send_planned_batches(&send_ctx, &format_output.batches);
        trace.record_reply_send(false, true, &format_output.batches);
        sent_batches = format_output.batches.len();
        visible_assistant_history_text = format_output.final_message.clone();
    }
    let reason = if explicit_no_reply {
        "brain 显式选择不回复"
    } else if sent_batches == 0 {
        "没有可发送的内容"
    } else {
        "已发送"
    };
    zihuan_core::qq_context::emit_qq_context_stage(
        ctx.agent_id,
        sender_id,
        turn_id,
        "发送回复",
        serde_json::json!({
            "suppress_send": format_output.suppress_send,
            "batches": sent_batches,
            "messages": crate::procedure::utils::reply_plan::message_stage_summaries(
                &format_output.batches,
                &format_output.batch_delays_ms,
            ),
            "reason": reason,
        }),
    );
    (sent_batches, explicit_no_reply, visible_assistant_history_text)
}

/// Commits the turn's session state snapshot back to the shared store.
pub(crate) fn write_back_session_state(
    ctx: &QqChatAgentServiceContext<'_>,
    turn_session_state: &Arc<Mutex<QqChatSessionState>>,
) {
    *ctx.session_state_store.lock().unwrap() = turn_session_state.lock().unwrap().clone();
}

/// Same re-arm rule for both reply paths: only an actual reply to the sender
/// starts their scheduled-job timers.
pub(crate) fn rearm_jobs_if_replied(
    ctx: &QqChatAgentServiceContext<'_>,
    sender_id: &str,
    replied: bool,
) {
    if replied {
        zihuan_core::scheduler::rearm_triggered_jobs(ctx.agent_id, sender_id);
    }
}
