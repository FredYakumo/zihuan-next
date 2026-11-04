//! The normal brain path of one turn.
//!
//! One procedure chain (documents/procedure.md): [group-message-window] -> [memory-agent] ->
//! before-brain -> brain -> after-brain -> format-output, executed through
//! `execute_blocking_procedure_chain`, followed by the reply decision, delivery, history
//! persistence, context events, and turn archiving.

use std::sync::{Arc, Mutex};

use log::warn;

use zihuan_core::agent::tools::ToolCallingStopReason;
use zihuan_core::agent::MemoryCapability;
use zihuan_core::error::{Error, Result};
use zihuan_core::graph::tool_spec::{
    QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT, QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT,
};
use zihuan_core::graph::DataValue;
use zihuan_core::model_inference::llm::{LLMMessage, MessageRole};
use zihuan_core::qq_context::{emit_qq_context_event, emit_qq_context_stage, QqChatContextEvent};
use zihuan_core::role::procedure::{execute_blocking_procedure_chain, Procedure};
use zihuan_core::runtime::block_async;
use zihuan_core::steer::{message_with_api_style, PendingSteerEvent};
use zihuan_core::utils::string_utils::shorten_text;

use super::super::core::{history_user_message, QqChatServiceTurnResult, LOG_PREFIX};
use super::super::logging::{emit_context_turn_failure, QqChatTurnIdentity};
use super::finalization::{
    deliver_reply, rearm_jobs_if_replied, record_rethink_stage, write_back_session_state,
};
use super::{build_after_brain, build_format_output, TurnContext};
use crate::procedure::{
    qq_procedure_context, BeforeBrainContext, GroupMessageWindowContext, QqBeforeBrain, QqBrain,
    QqBrainOutput, QqFormatOutputOutput, QqGroupMessageWindow, QqReplyReviewResult,
};
use crate::qq_chat::steer::SteerCommitGuard;
use crate::storage::qq_chat_history_store::{load_history, save_history};
use crate::tools::{
    qq_memory_access_context, AgentMemoryToolResources, DEFAULT_TOOL_MEMORY_AGENT,
    QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS,
};

/// Runs the normal turn: one procedure chain plus the reply lifecycle around it.
pub(crate) fn run(turn: TurnContext<'_>) -> Result<QqChatServiceTurnResult> {
    let turn_id = turn.turn_id;
    let sender_id = turn.sender_id;
    let ctx = turn.ctx;
    let trace = turn.trace;
    let is_group = turn.is_group;

    let memory_resources = MemoryCapability::resolve(
        ctx.local_memory_store.cloned(),
        ctx.retrieval_store.cloned(),
        ctx.embedding_model.cloned(),
    )
    .map(|memory| AgentMemoryToolResources {
        memory_backend: memory.backend,
        embedding_model: memory.embedding_model,
        llm: Arc::clone(ctx.llm),
        access: {
            let mut access = qq_memory_access_context(
                sender_id,
                turn.target_id,
                is_group,
                turn.prepared_input.event.group_id,
            );
            access.agent_id = ctx.agent_id.to_string();
            access.permission_connection = ctx.rdb_pool.cloned();
            access
        },
    });
    let turn_identity = QqChatTurnIdentity {
        agent_id: ctx.agent_id.to_string(),
        sender_id: sender_id.to_string(),
        turn_id: turn_id.to_string(),
    };

    let group_message_window = if is_group
        && !turn.continues_previous
        && turn.role_config.group_message_window_count > 0
    {
        turn.event.group_id.map(|group_id| {
            QqGroupMessageWindow::new(GroupMessageWindowContext {
                trace,
                identity: turn_identity.clone(),
                llm: ctx.intent_classification_llm,
                rdb_pool: ctx.rdb_pool,
                group_id: group_id.to_string(),
                current_message_id: turn.prepared_input.event.message_id,
                user_input: turn.current_message.clone(),
                window_count: turn.role_config.group_message_window_count,
                confirmed: &turn.service.group_message_window_confirmed,
            })
        })
    } else {
        None
    };
    let memory_phase = if turn.service.is_default_tool_enabled(DEFAULT_TOOL_MEMORY_AGENT) {
        memory_resources
            .map(|resources| {
                let level = zihuan_core::storage::user_permission::memory_source_level(
                    &resources.access,
                    sender_id,
                )?;
                Ok::<_, Error>(crate::procedure::QqMemoryAgent {
                    identity: turn_identity.clone(),
                    resources,
                    input: format!(
                        "Authenticated sender QQ ID: {sender_id}; AccessLevel: {level}\n{}",
                        turn.current_message
                    ),
                    previous_context: turn
                        .history
                        .iter()
                        .filter_map(LLMMessage::content_text_owned)
                        .collect::<Vec<_>>()
                        .join("\n"),
                })
            })
            .transpose()?
    } else {
        None
    };
    let before_brain = QqBeforeBrain::new(BeforeBrainContext {
        trace,
        identity: Some(turn_identity),
        // The turn's routed model decides the API style and multimodal shape of the
        // assembled user input.
        llm: turn.turn_llm,
        input: &turn.prepared_input,
        adapter: ctx.adapter,
        bot_name: ctx.bot_name,
        is_group,
        agent_system_prompt: ctx.agent_system_prompt,
        style_prompt: ctx.resolved_language_style.as_ref().map(|item| item.style_prompt.as_str()),
        message_rate_limit_warning: turn.message_rate_limit_warning,
        session_state: Arc::clone(&turn.turn_session_state),
        emotion_dimensions: turn.emotion_dimensions.clone(),
    });

    let shared_runtime_values = Arc::new(Mutex::new(ctx.shared_runtime_values.clone()));
    {
        let mut locked = shared_runtime_values.lock().unwrap();
        locked.insert(
            QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT.to_string(),
            DataValue::MessageEvent(turn.prepared_input.event.clone()),
        );
        let adapter_handle: zihuan_core::ims_bot_adapter::BotAdapterHandle = ctx.adapter.clone();
        locked.insert(
            QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT.to_string(),
            DataValue::BotAdapterRef(adapter_handle),
        );
        locked.insert(
            QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS.to_string(),
            DataValue::Boolean(false),
        );
    }

    let consumed_steer_messages = Arc::new(Mutex::new(Vec::new()));
    // Steer events drained mid-turn but not yet covered by a sent reply. The guard
    // rolls them back into the pending store if the turn dies before replying —
    // including on `?` short-circuits and panics — so they are never lost silently.
    let in_flight_steers: Arc<Mutex<Vec<PendingSteerEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let steer_commit_guard = SteerCommitGuard::new(
        Arc::clone(ctx.pending_steer),
        sender_id,
        Arc::clone(&in_flight_steers),
    );
    let brain = QqBrain::new(
        turn.service,
        ctx,
        trace,
        turn.turn_llm,
        &turn.prepared_input,
        sender_id,
        turn.target_id,
        turn.bot_id,
        is_group,
        turn_id,
        turn.event.group_name.clone(),
        turn.history,
        &turn.history_key,
        Arc::clone(&shared_runtime_values),
        Arc::clone(&consumed_steer_messages),
        Arc::clone(&in_flight_steers),
        Arc::clone(&turn.turn_session_state),
        turn.emotion_dimensions.clone(),
    );
    let brain = match brain {
        Ok(brain) => brain,
        Err(error) => {
            emit_context_turn_failure(ctx.agent_id, sender_id, turn_id, "构建 Brain", &error);
            return Err(error);
        }
    };
    // Role turn chain (documents/procedure.md): [group-message-window] -> before-brain ->
    // brain -> after-brain -> format-output. The chain runs through one shared procedure
    // context; the window agent picks the turn's group history, the before-brain
    // procedure assembles the turn's system prompt and user input, and the brain reads
    // both from it instead of assembling them itself.
    let mut shared = qq_procedure_context(
        &turn.history_key,
        Some(turn.current_message.clone()),
        Some(&turn.inference_event),
    );
    // The chain runs inside `block_async`. A guard taken inside the call arguments stays
    // alive until the end of this statement, i.e. for the whole chain, and the preprompt
    // stage locks the same non-reentrant mutex. Clone the state first so the guard is
    // released before the chain starts.
    let after_brain_session_state = turn.turn_session_state.lock().unwrap().clone();
    let mut chain: Vec<Arc<dyn Procedure + '_>> = Vec::new();
    if let Some(group_message_window) = group_message_window {
        chain.push(Arc::new(group_message_window));
    }
    if let Some(memory_phase) = memory_phase {
        chain.push(Arc::new(memory_phase));
    }
    chain.push(Arc::new(before_brain));
    chain.push(Arc::new(brain));
    chain.push(Arc::new(build_after_brain(
        ctx,
        trace,
        is_group,
        sender_id,
        &turn.inference_event,
        after_brain_session_state,
        turn.emotion_dimensions.clone(),
    )));
    chain.push(Arc::new(build_format_output(
        ctx,
        trace,
        turn_id,
        is_group,
        sender_id,
        turn.bot_id,
        &turn.inference_event,
        Arc::clone(&shared_runtime_values),
    )));
    let turn_outputs = block_async(execute_blocking_procedure_chain(chain, &mut shared));
    let turn_outputs = match turn_outputs {
        Ok(outputs) => outputs,
        Err(error) => {
            emit_context_turn_failure(ctx.agent_id, sender_id, turn_id, "Brain 推理循环", &error);
            return Err(error);
        }
    };
    let brain_output_payload = shared
        .find_output::<QqBrainOutput>()
        .map(|value| (*value).clone())
        .ok_or_else(|| {
            Error::ValidationError("qq brain procedure produced no turn output".to_string())
        })?;

    let mut visible_assistant_history_text = None;
    let mut explicit_no_reply = false;
    // False when the turn died before producing any reply (transport error, iteration
    // limit, no content): the session loop then stops instead of chaining a follow-up,
    // and the steer commit guard rolls consumed steers back into the pending store.
    let mut turn_completed = false;
    if brain_output_payload.final_reply_text.is_none() {
        match &brain_output_payload.stop_reason {
            ToolCallingStopReason::TransportError(ref err) => {
                warn!("{LOG_PREFIX} ToolCallingEngine transport error without reply: {err}");
            }
            ToolCallingStopReason::MaxIterationsReached => {
                warn!("{LOG_PREFIX} ToolCallingEngine exceeded max tool iterations without reply");
            }
            ToolCallingStopReason::Done => {
                warn!("{LOG_PREFIX} ToolCallingEngine finished without any sendable reply content");
            }
            ToolCallingStopReason::AwaitUserInput(ref request) => {
                warn!(
                    "{LOG_PREFIX} ToolCallingEngine paused for user input without reply: {}",
                    request.question
                );
            }
            ToolCallingStopReason::ToolCallLimitReached(ref request) => {
                warn!(
                    "{LOG_PREFIX} ToolCallingEngine paused at tool-call limit without reply: {}",
                    request.question
                );
            }
        }
        let stop_reason_text = match &brain_output_payload.stop_reason {
            ToolCallingStopReason::TransportError(_) => "模型调用异常",
            ToolCallingStopReason::MaxIterationsReached => "推理迭代次数达到上限",
            ToolCallingStopReason::ToolCallLimitReached(_) => "工具调用次数达到上限",
            ToolCallingStopReason::AwaitUserInput(_) => "推理暂停在等待输入",
            ToolCallingStopReason::Done => "没有生成可发送的回复",
        };
        emit_qq_context_stage(
            ctx.agent_id,
            sender_id,
            turn_id,
            "回复决策",
            serde_json::json!({
                "stop_reason": format!("{:?}", brain_output_payload.stop_reason),
                "suppress_send": false,
                "final_message": null,
                "reason": format!("推理结束但没有可发送的回复（{stop_reason_text}）"),
            }),
        );
    } else if brain_output_payload.final_reply_text.is_some() {
        if brain_output_payload.suppress_send {
            turn_completed = true;
            explicit_no_reply = true;
            emit_qq_context_stage(
                ctx.agent_id,
                sender_id,
                turn_id,
                "回复决策",
                serde_json::json!({
                    "stop_reason": format!("{:?}", brain_output_payload.stop_reason),
                    "suppress_send": true,
                    "final_message": brain_output_payload.final_reply_text,
                    "reason": "brain 输出了 [no_reply]",
                }),
            );
        } else {
            turn_completed = true;
            let review_result = turn_outputs
                .iter()
                .rev()
                .find_map(|output| output.cloned::<QqReplyReviewResult>())
                .ok_or_else(|| {
                    Error::ValidationError(
                        "after-brain procedure produced no review result".to_string(),
                    )
                })?;
            record_rethink_stage(
                ctx,
                sender_id,
                turn_id,
                brain_output_payload.final_reply_text.as_deref(),
                &review_result,
            );

            let format_output = turn_outputs
                .iter()
                .rev()
                .find_map(|output| output.cloned::<QqFormatOutputOutput>())
                .ok_or_else(|| {
                    Error::ValidationError(
                        "format-output procedure produced no turn output".to_string(),
                    )
                })?;
            let (_sent_batches, no_reply, visible_text) = deliver_reply(
                trace,
                ctx,
                turn.event,
                sender_id,
                turn_id,
                turn.target_id,
                is_group,
                turn.bot_id,
                &format_output,
            );
            explicit_no_reply = no_reply;
            visible_assistant_history_text = visible_text;
        }
    }

    // The brain procedure compacted (and persisted) the working history if needed, so
    // reload it and append this turn's messages to what was actually kept. The stored copy
    // drops the per-turn state prefix and processing instructions; keeping the conversation
    // itself keeps the history readable and stops it growing a prefix per turn.
    let mut history = load_history(ctx.cache, &turn.history_key);
    history.push(history_user_message(brain_output_payload.current_user_message.clone()));
    // Steer messages drained mid-turn only belong in history once the turn actually
    // answered them; on a failed turn they are rolled back into the pending store
    // and the next turn re-injects them fresh.
    if turn_completed {
        history.extend(
            consumed_steer_messages
                .lock()
                .unwrap()
                .iter()
                .map(|message| history_user_message(message.clone())),
        );
    }
    // The final model call's usage, carried onto the assistant history message so the live
    // context view can report input/output tokens and cache hit rate. `usage` is not part of
    // request serialization, so this never affects inference.
    let turn_usage = brain_output_payload
        .brain_output
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::Assistant) && message.usage.is_some())
        .and_then(|message| message.usage.clone());
    // Reasoning of the final model call, shown in the context view as a collapsed block.
    let turn_reasoning = brain_output_payload
        .brain_output
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::Assistant))
        .and_then(|message| message.reasoning_content.clone())
        .filter(|text| !text.trim().is_empty());
    if let Some(ref assistant_text) = visible_assistant_history_text {
        let mut assistant_history = LLMMessage::assistant_text(assistant_text.clone());
        assistant_history.usage = turn_usage.clone();
        history.push(message_with_api_style(assistant_history, turn.turn_llm.api_style()));
    }
    save_history(ctx.cache, &turn.history_key, history);

    let generation_ms = trace.inference_elapsed_ms();
    let output_tokens_per_second = match (turn_usage.as_ref(), generation_ms) {
        (Some(usage), Some(ms)) if ms > 0 => {
            usage.completion_tokens.map(|tokens| tokens as f64 / (ms as f64 / 1000.0))
        }
        _ => None,
    };
    emit_qq_context_event(QqChatContextEvent::TurnFinished {
        agent_id: ctx.agent_id.to_string(),
        sender_id: sender_id.to_string(),
        turn_id: turn_id.to_string(),
        assistant_text: visible_assistant_history_text.clone(),
        reasoning_text: turn_reasoning,
        usage: turn_usage,
        generation_ms,
        output_tokens_per_second,
    });
    // Archive the finished turn into the agent's relational database so the conversation
    // stays browsable after the live context window is gone.
    if let Some(connection) = ctx.rdb_pool {
        if let Some(record) = block_async(zihuan_core::qq_context::qq_chat_turn_record(
            &ctx.agent_id,
            sender_id,
            turn_id,
        )) {
            crate::storage::qq_chat_context_store::persist_turn(connection, &record);
        }
    }

    rearm_jobs_if_replied(ctx, sender_id, visible_assistant_history_text.is_some());
    write_back_session_state(ctx, &turn.turn_session_state);

    let result_summary = if let Some(ref assistant_text) = visible_assistant_history_text {
        format!("已回复[{sender_id}]，内容：{}", shorten_text(assistant_text, 80))
    } else if explicit_no_reply {
        format!("已处理[{sender_id}]的消息，显式选择不回复")
    } else if matches!(brain_output_payload.stop_reason, ToolCallingStopReason::TransportError(_)) {
        format!("回复[{sender_id}]失败：模型请求异常")
    } else {
        format!("已处理[{sender_id}]的消息，但未发送回复")
    };
    trace.log_result_summary(&result_summary);

    if turn_completed {
        steer_commit_guard.commit();
    }
    Ok(QqChatServiceTurnResult { result_summary, turn_completed })
}
