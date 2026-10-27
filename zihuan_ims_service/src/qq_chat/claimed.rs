use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use log::{info, warn};

use zihuan_core::agent::tools::ToolCallingStopReason;
use zihuan_core::agent::MemoryCapability;

use crate::agent::emotion::utils::{emotion_expression_prompt, has_noticeable_emotion_expression};
use crate::qq_chat::resources::current_qq_chat_role_service_config;
use crate::qq_session_state::QqChatSessionState;
use crate::role_config::QqChatEmotionDimensionConfig;
use crate::storage::message_rate_limit_store::MessageRateLimitCheckResult;
use zihuan_core::error::{Error, Result};
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::qq_context::{collect_turn_media_refs, EmbeddingTraceGuard, QqChatContextEvent};
use zihuan_core::steer::{message_with_api_style, PendingSteerEvent};

use zihuan_core::graph::tool_spec::{
    QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT, QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT,
};
use zihuan_core::graph::DataValue;

use super::super::super::tools::{
    format_public_info_message, qq_memory_access_context, AgentMemoryToolResources,
    QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS,
};

use crate::storage::qq_chat_history_store::{conversation_history_key, load_history, save_history};

use crate::classify_intent::{classify_intent_with_trace, IntentCategory};
use crate::qq_chat::command::{run_command_pipeline, CommandTurnEnd};
use crate::qq_chat::msg_send::{
    build_reply_result, send_planned_batches, take_reply_directive, QqChatServiceSendContext,
};
use crate::qq_chat::steer::SteerCommitGuard;

use super::{
    build_meta_query_system_prompt, build_meta_query_user_message,
    collect_available_media_from_brain_output, compact_turn_history, expand_messages_for_inference,
    prepare_current_turn_user_input, prepare_current_turn_user_input_from_event,
    QqChatAgentServiceContext, QqChatAgentServiceInner, QqChatServiceTurnResult, QqChatTaskTrace,
    LOG_PREFIX,
};

use crate::procedure::{
    qq_procedure_context, BeforeBrainContext, ModelIdentityContext, QqAfterBrain,
    QqAfterBrainContext, QqBeforeBrain, QqBrain, QqBrainOutput, QqMetaQueryBrain,
    QqReplyReviewRequest, QqReplyReviewResult,
};
use crate::qq_chat::logging::{emit_context_turn_failure, QqChatTurnIdentity};
use zihuan_core::role::procedure::execute_blocking_procedure_chain;
use zihuan_core::runtime::block_async;

impl QqChatAgentServiceInner {
    fn selected_turn_llm<'a>(
        &self,
        ctx: &'a QqChatAgentServiceContext<'_>,
        intent_category: IntentCategory,
    ) -> (&'a Arc<dyn zihuan_core::model_inference::llm::llm_base::LLMBase>, &'a str) {
        match intent_category {
            IntentCategory::SolveComplexProblem | IntentCategory::WriteCode => {
                (ctx.math_programming_llm, "math_programming")
            }
            _ => (ctx.llm, "main"),
        }
    }

    /// Processes a single QQ chat turn end-to-end for a claimed message.
    ///
    /// The lifecycle is:
    /// - **Hydration & extraction** — resolves reply chains and extracts the user text.
    /// - **Command interception** — runs the message through the QQ command pipeline
    ///   (`crate::qq_chat::command`), which either consumes the turn or leaves
    ///   remaining text to the brain loop.
    /// - **Intent classification** — selects the turn's chain composition and the routed LLM
    ///   (general vs math/programming).
    /// - **Short-circuit replies** — answers meta-queries (model name, tool list, etc.) directly.
    /// - **Role turn chain** — one procedure chain (documents/procedure.md): before-brain
    ///   (assembles the system prompt and user input, including memory titles and the emotion
    ///   prompt) -> brain (compacts history, runs the tool-calling loop with steer support) ->
    ///   after-brain (reply review/rewrite).
    /// - **Reply delivery** — parses the final assistant output and sends it back to the user
    ///   (group or private chat), persisting message history along the way.
    ///
    /// Returns a [`QqChatServiceTurnResult`] containing a human-readable summary of what happened.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn handle_claimed_turn(
        &self,
        trace: &QqChatTaskTrace,
        event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
        _time: &str,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        bot_id: &str,
        message_rate_limit: Option<&MessageRateLimitCheckResult>,
        // True when this turn replays queued steer messages after a previous turn in the same
        // session; the context view then keeps both inside one block.
        continues_previous: bool,
        ctx: &QqChatAgentServiceContext<'_>,
    ) -> Result<QqChatServiceTurnResult> {
        let message_rate_limit_warning =
            message_rate_limit.and_then(|result| result.warning_prompt.as_deref());
        let prepared_input =
            prepare_current_turn_user_input(event, ctx.adapter, bot_id, ctx.bot_name, ctx.s3_ref);
        let mut inference_event = prepared_input.event.clone();
        inference_event.message_list =
            expand_messages_for_inference(&prepared_input.event.message_list);
        let inference_input = prepare_current_turn_user_input_from_event(
            &inference_event,
            bot_id,
            ctx.bot_name,
            ctx.s3_ref,
        );
        let raw_user_message = prepared_input.current_text_for_prompt().to_string();
        let mut current_message = inference_input.current_text_for_prompt().to_string();
        trace.log_user_message(&raw_user_message, &current_message);

        // Identity for this turn's live context events (Data Explorer view).
        let turn_id = uuid::Uuid::new_v4().to_string();
        // Intent classification and before-brain retrieval run synchronously on this blocking
        // turn thread, so binding the turn here lets the embedding runtime report each of their
        // inferences as a live context stage without threading the turn identity through every
        // retrieval call.
        let _embedding_trace = EmbeddingTraceGuard::enter(ctx.agent_id, sender_id, &turn_id);
        let turn_media = collect_turn_media_refs(&prepared_input.event.message_list);

        let emotion_dimensions =
            current_qq_chat_role_service_config()?.resolved_emotion_dimensions();
        let now_unix_seconds =
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
        {
            let mut session_state = ctx.session_state_store.lock().unwrap();
            session_state.dissipate_expired_emotions(&emotion_dimensions, now_unix_seconds);
            session_state.record_conversation_activity(now_unix_seconds);
        }

        // Reset session-level tool quota for each new turn so that
        // tool call limits apply per single user message cycle and not
        // across the entire agent service lifetime.
        if let Some(ref quota) = ctx.tool_quota {
            quota.session_state.lock().unwrap().reset();
        }

        let history_key = conversation_history_key(sender_id);
        let history = load_history(ctx.cache, &history_key);

        // Command interception runs through the unified command state machine
        // (crate::qq_chat::command). The pipeline resolves the spec, runs
        // preconditions/body steps via the QQ channel runtime, and returns
        // effects (or a pause) for this turn.
        match run_command_pipeline(
            &self.id,
            trace,
            event,
            &inference_event,
            &raw_user_message,
            sender_id,
            target_id,
            bot_id,
            is_group,
            &emotion_dimensions,
            ctx,
        )? {
            CommandTurnEnd::Consumed(result) => return Ok(result),
            CommandTurnEnd::Passthrough(text) => current_message = text,
            CommandTurnEnd::NotACommand => {}
        }

        // Non-command messages fall through to the brain loop.

        let mut current_session_state = ctx.session_state_store.lock().unwrap().clone();
        current_session_state.sync_emotion_dimensions(&emotion_dimensions);
        let turn_session_state = Arc::new(Mutex::new(current_session_state));

        // Announce the turn before the first real step runs, so every stage and tool call that
        // follows is appended in the order it actually happened.
        zihuan_core::qq_context::emit_qq_context_event(QqChatContextEvent::TurnStarted {
            agent_id: ctx.agent_id.to_string(),
            sender_id: sender_id.to_string(),
            sender_name: zihuan_core::ims_bot_adapter::utils::sender_display_name!(
                &event.sender.nickname,
                &event.sender.card
            ),
            group_id: event.group_id.map(|value| value.to_string()),
            group_name: event.group_name.clone(),
            is_group,
            turn_id: turn_id.clone(),
            created_at: chrono::Local::now().to_rfc3339(),
            user_text: raw_user_message.clone(),
            quoted: prepared_input.quoted_message.clone(),
            media: turn_media.clone(),
            continues_previous,
        });

        // Report the rate-limit decision once per turn so the context view can show which
        // quota applied and how much of it had been consumed. Steer follow-ups share the
        // original turn's block and would otherwise repeat the same snapshot.
        if !continues_previous {
            zihuan_core::qq_context::emit_qq_context_stage(
                ctx.agent_id,
                sender_id,
                &turn_id,
                "限流",
                rate_limit_stage_detail(message_rate_limit),
            );
        }

        // Intent classification picks the turn's chain composition (meta-query vs brain) and
        // the routed brain model, so it runs before any procedure chain is assembled
        // (documents/procedure.md: which procedures a turn uses is decided per event). It is
        // the preprompt phase's model call: the "Preprompt 模型" setting, falling back to the
        // main model when unset.
        let intent_trace = classify_intent_with_trace(
            ctx.intent_classification_llm,
            ctx.embedding_model,
            &current_message,
            Some(&history),
            800,
        );
        let (turn_llm, routed_model) = self.selected_turn_llm(ctx, intent_trace.category);
        trace.record_intent_classification(&intent_trace, routed_model);
        zihuan_core::qq_context::emit_qq_context_stage(
            ctx.agent_id,
            sender_id,
            &turn_id,
            "意图识别",
            serde_json::json!({
                "category": intent_trace.category.label(),
                "path": intent_trace.path.label(),
                "used_embedding": intent_trace.used_embedding,
                "used_llm": intent_trace.used_llm,
                "embedding_duration_ms": intent_trace.embedding_duration_ms,
                "duration_ms": intent_trace.total_duration_ms,
                "raw_label": intent_trace.raw_label,
                "routed_model": routed_model,
                "model": turn_llm.get_model_name(),
            }),
        );

        let is_meta_query = matches!(
            intent_trace.category,
            IntentCategory::AskToolList
                | IntentCategory::AskSystemPrompt
                | IntentCategory::AskModelName
        );
        if is_meta_query {
            info!(
                "{LOG_PREFIX} meta-query short-circuit for sender={sender_id}, intent={}",
                intent_trace.category.label()
            );
            // Meta-query turns answer directly without the brain loop. The turn was already
            // announced, so that path closes it with its own `TurnFinished` event.
            let meta_result = self.handle_meta_query_turn(
                trace,
                event,
                &inference_event,
                sender_id,
                target_id,
                is_group,
                bot_id,
                ctx,
                &current_message,
                &history_key,
                history,
                &turn_session_state,
                &emotion_dimensions,
                &turn_id,
            );
            if let Err(ref error) = meta_result {
                emit_context_turn_failure(ctx.agent_id, sender_id, &turn_id, "元查询", error);
            }
            return meta_result;
        }

        // Normal turn: one procedure chain, before-brain -> brain -> after-brain
        // (documents/procedure.md). The before-brain procedure assembles the turn's system
        // prompt and user input; the brain reads them through the shared procedure context.
        let memory_resources = MemoryCapability::resolve(
            ctx.local_memory_store.cloned(),
            ctx.retrieval_store.cloned(),
            ctx.embedding_model.cloned(),
        )
        .map(|memory| AgentMemoryToolResources {
            memory_backend: memory.backend,
            embedding_model: memory.embedding_model,
            llm: Arc::clone(ctx.llm),
            access: qq_memory_access_context(
                sender_id,
                target_id,
                is_group,
                prepared_input.event.group_id,
            ),
        });
        let turn_identity = QqChatTurnIdentity {
            agent_id: ctx.agent_id.to_string(),
            sender_id: sender_id.to_string(),
            turn_id: turn_id.clone(),
        };
        let before_brain = QqBeforeBrain::new(BeforeBrainContext {
            trace,
            identity: Some(turn_identity),
            // The turn's routed model decides the API style and multimodal shape of the
            // assembled user input.
            llm: turn_llm,
            input: &prepared_input,
            adapter: ctx.adapter,
            bot_name: ctx.bot_name,
            is_group,
            agent_system_prompt: ctx.agent_system_prompt,
            style_prompt: ctx
                .resolved_language_style
                .as_ref()
                .map(|item| item.style_prompt.as_str()),
            message_rate_limit_warning,
            session_state: Arc::clone(&turn_session_state),
            emotion_dimensions: emotion_dimensions.clone(),
            memory_resources,
            default_tools_enabled: &self.default_tools_enabled,
        });

        let shared_runtime_values = Arc::new(Mutex::new(ctx.shared_runtime_values.clone()));
        {
            let mut locked = shared_runtime_values.lock().unwrap();
            locked.insert(
                QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT.to_string(),
                DataValue::MessageEvent(prepared_input.event.clone()),
            );
            let adapter_handle: zihuan_core::ims_bot_adapter::BotAdapterHandle =
                ctx.adapter.clone();
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
            self,
            ctx,
            trace,
            turn_llm,
            &prepared_input,
            &current_message,
            sender_id,
            target_id,
            bot_id,
            is_group,
            &turn_id,
            event.group_name.clone(),
            history,
            &history_key,
            Arc::clone(&shared_runtime_values),
            Arc::clone(&consumed_steer_messages),
            Arc::clone(&in_flight_steers),
            Arc::clone(&turn_session_state),
            emotion_dimensions.clone(),
        );
        let brain = match brain {
            Ok(brain) => brain,
            Err(error) => {
                emit_context_turn_failure(ctx.agent_id, sender_id, &turn_id, "构建 Brain", &error);
                return Err(error);
            }
        };
        // Role turn chain (documents/procedure.md): before-brain -> brain -> after-brain.
        // The chain runs through one shared procedure context; the brain reads the assembled
        // system prompt and user input from it instead of assembling them itself.
        let mut shared = qq_procedure_context(&history_key, Some(current_message.clone()));
        let turn_outputs = block_async(execute_blocking_procedure_chain(
            vec![
                Arc::new(before_brain),
                Arc::new(brain),
                Arc::new(QqAfterBrain::new(QqAfterBrainContext {
                    review_llm: ctx.intent_classification_llm,
                    rewrite_llm: ctx.natural_language_reply_llm,
                    reply_system_prompt: ctx.natural_language_reply_system_prompt,
                    request: QqReplyReviewRequest {
                        // Filled from the brain output inside the after-brain procedure.
                        candidate_message: String::new(),
                        is_group,
                        bot_name: ctx.bot_name.to_string(),
                        sender_id: sender_id.to_string(),
                        sender_nickname: inference_event.sender.nickname.clone(),
                        sender_card: inference_event.sender.card.clone(),
                        session_state: turn_session_state.lock().unwrap().clone(),
                        emotion_dimensions: emotion_dimensions.clone(),
                        model_identity_context: Some(build_model_identity_context(ctx)),
                    },
                    trace,
                })),
            ],
            &mut shared,
        ));
        let turn_outputs = match turn_outputs {
            Ok(outputs) => outputs,
            Err(error) => {
                emit_context_turn_failure(
                    ctx.agent_id,
                    sender_id,
                    &turn_id,
                    "Brain 推理循环",
                    &error,
                );
                return Err(error);
            }
        };
        let brain_output_payload =
            shared.find_output::<QqBrainOutput>().map(|value| (*value).clone()).ok_or_else(
                || Error::ValidationError("qq brain procedure produced no turn output".to_string()),
            )?;

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
                    warn!(
                        "{LOG_PREFIX} ToolCallingEngine exceeded max tool iterations without reply"
                    );
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
                    warn!("{LOG_PREFIX} ToolCallingEngine paused at tool-call limit without reply: {}", request.question);
                }
            }
            let stop_reason_text = match &brain_output_payload.stop_reason {
                ToolCallingStopReason::TransportError(_) => "模型调用异常",
                ToolCallingStopReason::MaxIterationsReached => "推理迭代次数达到上限",
                ToolCallingStopReason::ToolCallLimitReached(_) => "工具调用次数达到上限",
                ToolCallingStopReason::AwaitUserInput(_) => "推理暂停在等待输入",
                ToolCallingStopReason::Done => "没有生成可发送的回复",
            };
            zihuan_core::qq_context::emit_qq_context_stage(
                ctx.agent_id,
                sender_id,
                &turn_id,
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
                zihuan_core::qq_context::emit_qq_context_stage(
                    ctx.agent_id,
                    sender_id,
                    &turn_id,
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
                let available_media =
                    collect_available_media_from_brain_output(&brain_output_payload.brain_output);
                let review_result = turn_outputs
                    .iter()
                    .rev()
                    .find_map(|output| output.cloned::<QqReplyReviewResult>())
                    .ok_or_else(|| {
                        Error::ValidationError(
                            "after-brain procedure produced no review result".to_string(),
                        )
                    })?;
                zihuan_core::qq_context::emit_qq_context_stage(
                    ctx.agent_id,
                    sender_id,
                    &turn_id,
                    "Rethink",
                    serde_json::json!({
                        "safe": review_result.safe,
                        "rewritten": review_result.rewritten,
                        "reason": review_result.reason,
                        "candidate": brain_output_payload.final_reply_text,
                        "final_message": review_result.final_message,
                    }),
                );

                let reply_result = build_reply_result(
                    &review_result.final_message,
                    is_group,
                    sender_id,
                    &inference_event.sender.nickname,
                    inference_event.sender.card.as_str(),
                    bot_id,
                    ctx.bot_name,
                    ctx.max_message_length,
                    take_reply_directive(&shared_runtime_values),
                    Some(inference_event.message_id),
                    available_media,
                    ctx.rdb_pool.cloned(),
                    ctx.reply_batch_builder,
                );
                let reply_result = match reply_result {
                    Ok(reply_result) => reply_result,
                    Err(error) => {
                        emit_context_turn_failure(
                            ctx.agent_id,
                            sender_id,
                            &turn_id,
                            "构建回复",
                            &error,
                        );
                        return Err(error);
                    }
                };
                trace.mark_reply_send_started();
                let mut sent_batches = 0usize;
                if reply_result.suppress_send {
                    explicit_no_reply = true;
                    trace.record_reply_send(true, false, &reply_result.batches);
                } else if reply_result.batches.is_empty() {
                    trace.record_reply_send(false, false, &reply_result.batches);
                } else {
                    let send_ctx = QqChatServiceSendContext {
                        adapter: ctx.adapter,
                        target_id,
                        is_group,
                        group_name: event.group_name.as_deref(),
                        bot_id,
                        bot_name: ctx.bot_name,
                        mention_target_id: None,
                        persistence:
                            crate::storage::qq_chat_session_store::build_outbound_persistence(
                                ctx.rdb_pool,
                                event.group_name.as_deref(),
                                ctx.bot_name,
                            ),
                        max_text_chars: ctx.max_message_length,
                    };
                    send_planned_batches(&send_ctx, &reply_result.batches);
                    trace.record_reply_send(false, true, &reply_result.batches);
                    sent_batches = reply_result.batches.len();
                    visible_assistant_history_text = Some(review_result.final_message);
                }
                zihuan_core::qq_context::emit_qq_context_stage(
                    ctx.agent_id,
                    sender_id,
                    &turn_id,
                    "发送回复",
                    serde_json::json!({
                        "suppress_send": reply_result.suppress_send,
                        "batches": sent_batches,
                        "reason": if explicit_no_reply {
                            "brain 显式选择不回复"
                        } else if sent_batches == 0 {
                            "没有可发送的内容"
                        } else {
                            "已发送"
                        },
                    }),
                );
            }
        }

        // The brain procedure compacted (and persisted) the working history if needed, so
        // reload it and append this turn's messages to what was actually kept.
        let mut history = load_history(ctx.cache, &history_key);
        history.push(brain_output_payload.current_user_message.clone());
        // Steer messages drained mid-turn only belong in history once the turn actually
        // answered them; on a failed turn they are rolled back into the pending store
        // and the next turn re-injects them fresh.
        if turn_completed {
            history.extend(consumed_steer_messages.lock().unwrap().iter().cloned());
        }
        // The final model call's usage, carried onto the assistant history message so the live
        // context view can report input/output tokens and cache hit rate. `usage` is not part of
        // request serialization, so this never affects inference.
        let turn_usage = brain_output_payload
            .brain_output
            .iter()
            .rev()
            .find(|message| {
                matches!(message.role, zihuan_core::model_inference::llm::MessageRole::Assistant)
                    && message.usage.is_some()
            })
            .and_then(|message| message.usage.clone());
        // Reasoning of the final model call, shown in the context view as a collapsed block.
        let turn_reasoning = brain_output_payload
            .brain_output
            .iter()
            .rev()
            .find(|message| {
                matches!(message.role, zihuan_core::model_inference::llm::MessageRole::Assistant)
            })
            .and_then(|message| message.reasoning_content.clone())
            .filter(|text| !text.trim().is_empty());
        if let Some(ref assistant_text) = visible_assistant_history_text {
            let mut assistant_history = LLMMessage::assistant_text(assistant_text.clone());
            assistant_history.usage = turn_usage.clone();
            history.push(message_with_api_style(assistant_history, turn_llm.api_style()));
        }
        save_history(ctx.cache, &history_key, history);

        let generation_ms = trace.inference_elapsed_ms();
        let output_tokens_per_second = match (turn_usage.as_ref(), generation_ms) {
            (Some(usage), Some(ms)) if ms > 0 => {
                usage.completion_tokens.map(|tokens| tokens as f64 / (ms as f64 / 1000.0))
            }
            _ => None,
        };
        zihuan_core::qq_context::emit_qq_context_event(QqChatContextEvent::TurnFinished {
            agent_id: ctx.agent_id.to_string(),
            sender_id: sender_id.to_string(),
            turn_id: turn_id.clone(),
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
                &turn_id,
            )) {
                crate::storage::qq_chat_context_store::persist_turn(connection, &record);
            }
        }

        if visible_assistant_history_text.is_some() {
            zihuan_core::scheduler::rearm_triggered_jobs(ctx.agent_id, sender_id);
        }
        *ctx.session_state_store.lock().unwrap() = turn_session_state.lock().unwrap().clone();

        let result_summary = if let Some(ref assistant_text) = visible_assistant_history_text {
            format!(
                "已回复[{sender_id}]，内容：{}",
                zihuan_core::utils::string_utils::shorten_text(assistant_text, 80)
            )
        } else if explicit_no_reply {
            format!("已处理[{sender_id}]的消息，显式选择不回复")
        } else if matches!(
            brain_output_payload.stop_reason,
            ToolCallingStopReason::TransportError(_)
        ) {
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

    /// Handles meta-query intents (AskToolList, AskSystemPrompt) by using a dedicated
    /// LLM context with pre-fetched function list and public info, without exposing
    /// any tool specs to the LLM.
    ///
    /// The LLM receives only safe, pre-fetched data as text context and is instructed
    /// to rephrase it into natural language. The result still goes through
    /// `review_and_rewrite_reply` as a safety net.
    fn handle_meta_query_turn(
        &self,
        trace: &QqChatTaskTrace,
        event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
        inference_event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        bot_id: &str,
        ctx: &QqChatAgentServiceContext<'_>,
        current_message: &str,
        history_key: &str,
        history: Vec<LLMMessage>,
        turn_session_state: &Arc<Mutex<QqChatSessionState>>,
        emotion_dimensions: &[QqChatEmotionDimensionConfig],
        turn_id: &str,
    ) -> Result<QqChatServiceTurnResult> {
        // The caller passes the raw loaded history; compact it here with a plain user message
        // as the token-estimate reference so the persisted history stays within budget too.
        let mut history = {
            let reference = message_with_api_style(
                LLMMessage::user(current_message.to_string()),
                ctx.llm.api_style(),
            );
            compact_turn_history(ctx.cache, history_key, ctx.llm, history, &reference).messages
        };
        let function_list = zihuan_core::command::build_help_text()
            .unwrap_or_else(|| "暂无可用功能信息。".to_string());
        let public_info = format_public_info_message(current_message).to_string();

        let style_prompt =
            ctx.resolved_language_style.as_ref().map(|item| item.style_prompt.as_str());
        let (emotion_prompt, suppress_language_style) = {
            let session = turn_session_state.lock().unwrap();
            (
                emotion_expression_prompt(&session, emotion_dimensions),
                has_noticeable_emotion_expression(&session, emotion_dimensions),
            )
        };
        let style_prompt = if suppress_language_style {
            None
        } else {
            style_prompt
        };

        let meta_system_prompt =
            build_meta_query_system_prompt(ctx.bot_name, style_prompt, &emotion_prompt);
        let meta_user_message =
            build_meta_query_user_message(current_message, &function_list, &public_info);

        let meta_messages =
            vec![LLMMessage::system(meta_system_prompt), LLMMessage::user(meta_user_message)];
        zihuan_core::qq_context::emit_qq_context_stage(
            ctx.agent_id,
            sender_id,
            turn_id,
            zihuan_core::qq_context::QQ_CONTEXT_STAGE_SYSTEM_PROMPT,
            serde_json::json!({ "prompt": meta_messages[0].content_text_owned() }),
        );

        // Role turn chain (documents/procedure.md): the meta-query brain (a single direct
        // inference) -> AfterBrain review.
        let mut shared = qq_procedure_context(history_key, None);
        let meta_outputs = block_async(execute_blocking_procedure_chain(
            vec![
                Arc::new(QqMetaQueryBrain::new(ctx.llm, trace, meta_messages)),
                Arc::new(QqAfterBrain::new(QqAfterBrainContext {
                    review_llm: ctx.intent_classification_llm,
                    rewrite_llm: ctx.natural_language_reply_llm,
                    reply_system_prompt: ctx.natural_language_reply_system_prompt,
                    request: QqReplyReviewRequest {
                        // Filled from the brain output inside the after-brain procedure.
                        candidate_message: String::new(),
                        is_group,
                        bot_name: ctx.bot_name.to_string(),
                        sender_id: sender_id.to_string(),
                        sender_nickname: inference_event.sender.nickname.clone(),
                        sender_card: inference_event.sender.card.clone(),
                        session_state: turn_session_state.lock().unwrap().clone(),
                        emotion_dimensions: emotion_dimensions.to_vec(),
                        model_identity_context: Some(build_model_identity_context(ctx)),
                    },
                    trace,
                })),
            ],
            &mut shared,
        ))?;
        let meta_payload = shared
            .find_output::<QqBrainOutput>()
            .map(|value| (*value).clone())
            .ok_or_else(|| {
                Error::ValidationError(
                    "qq meta-query brain procedure produced no turn output".to_string(),
                )
            })?;
        let Some(candidate_message) = meta_payload.final_reply_text.as_ref() else {
            emit_meta_query_turn_finished(ctx, sender_id, turn_id, &meta_payload, None);
            return Ok(QqChatServiceTurnResult {
                result_summary: format!("元查询[{sender_id}]：LLM未返回有效回复"),
                turn_completed: true,
            });
        };
        zihuan_core::qq_context::emit_qq_context_stage(
            ctx.agent_id,
            sender_id,
            turn_id,
            "元查询直答",
            serde_json::json!({
                "model": ctx.llm.get_model_name(),
                "tools": 0,
                "answer": candidate_message,
            }),
        );

        if meta_payload.suppress_send {
            history.push(message_with_api_style(
                LLMMessage::user(current_message.to_string()),
                ctx.llm.api_style(),
            ));
            save_history(ctx.cache, history_key, history);
            *ctx.session_state_store.lock().unwrap() = turn_session_state.lock().unwrap().clone();
            trace.record_final_reply_decision(Some(candidate_message), Some(true), None);
            emit_meta_query_turn_finished(ctx, sender_id, turn_id, &meta_payload, None);
            return Ok(QqChatServiceTurnResult {
                result_summary: format!("已处理[{sender_id}]的元查询，显式选择不回复"),
                turn_completed: true,
            });
        }

        let review_result = meta_outputs
            .iter()
            .rev()
            .find_map(|output| output.cloned::<QqReplyReviewResult>())
            .ok_or_else(|| {
                Error::ValidationError(
                    "after-brain procedure produced no review result".to_string(),
                )
            })?;
        zihuan_core::qq_context::emit_qq_context_stage(
            ctx.agent_id,
            sender_id,
            turn_id,
            "Rethink",
            serde_json::json!({
                "safe": review_result.safe,
                "rewritten": review_result.rewritten,
                "reason": review_result.reason,
                "candidate": candidate_message,
                "final_message": review_result.final_message,
            }),
        );
        let reply_result = build_reply_result(
            &review_result.final_message,
            is_group,
            sender_id,
            &inference_event.sender.nickname,
            inference_event.sender.card.as_str(),
            bot_id,
            ctx.bot_name,
            ctx.max_message_length,
            None,
            Some(inference_event.message_id),
            HashMap::new(),
            ctx.rdb_pool.cloned(),
            ctx.reply_batch_builder,
        )?;

        let mut visible_assistant_history_text = None;
        let mut sent_batches = 0usize;
        trace.mark_reply_send_started();
        if reply_result.suppress_send {
            trace.record_reply_send(true, false, &reply_result.batches);
        } else if reply_result.batches.is_empty() {
            trace.record_reply_send(false, false, &reply_result.batches);
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
            send_planned_batches(&send_ctx, &reply_result.batches);
            trace.record_reply_send(false, true, &reply_result.batches);
            sent_batches = reply_result.batches.len();
            visible_assistant_history_text = Some(review_result.final_message);
        }
        zihuan_core::qq_context::emit_qq_context_stage(
            ctx.agent_id,
            sender_id,
            turn_id,
            "发送回复",
            serde_json::json!({
                "suppress_send": reply_result.suppress_send,
                "batches": sent_batches,
            }),
        );

        history.push(message_with_api_style(
            LLMMessage::user(current_message.to_string()),
            ctx.llm.api_style(),
        ));
        if let Some(ref assistant_text) = visible_assistant_history_text {
            history.push(message_with_api_style(
                LLMMessage::assistant_text(assistant_text.clone()),
                ctx.llm.api_style(),
            ));
        }
        save_history(ctx.cache, history_key, history);
        // Same re-arm rule as the main reply path: only an actual reply to the sender
        // starts their scheduled-job timers.
        if visible_assistant_history_text.is_some() {
            zihuan_core::scheduler::rearm_triggered_jobs(ctx.agent_id, sender_id);
        }
        *ctx.session_state_store.lock().unwrap() = turn_session_state.lock().unwrap().clone();

        let result_summary = if let Some(ref assistant_text) = visible_assistant_history_text {
            format!(
                "已回复[{sender_id}]的元查询，内容：{}",
                zihuan_core::utils::string_utils::shorten_text(assistant_text, 80)
            )
        } else {
            format!("已处理[{sender_id}]的元查询，但未发送回复")
        };
        trace.log_result_summary(&result_summary);
        emit_meta_query_turn_finished(
            ctx,
            sender_id,
            turn_id,
            &meta_payload,
            visible_assistant_history_text.as_deref(),
        );

        Ok(QqChatServiceTurnResult { result_summary, turn_completed: true })
    }
}

/// The rate-limit snapshot the context view shows for one turn: which rule applied
/// (user/group/default), its window and quota, and how much of it this message used.
fn rate_limit_stage_detail(result: Option<&MessageRateLimitCheckResult>) -> serde_json::Value {
    let Some(result) = result else {
        // No relational database: the runtime skips rate limiting entirely for this turn.
        return serde_json::json!({ "configured": false, "unlimited": true });
    };
    let rule = result.resolved_limit.as_ref().map(|resolved| &resolved.rule);
    serde_json::json!({
        "configured": rule.is_some(),
        "unlimited": rule.is_none_or(|rule| rule.is_effectively_unlimited()),
        "scope_type": result.resolved_limit.as_ref().map(|resolved| resolved.scope_type.as_str()),
        "window_unit": rule.and_then(|rule| rule.window_unit).map(|unit| unit.as_str()),
        "window_size": rule.map(|rule| rule.window_size),
        "max_calls": result.max_calls,
        "used_calls": result.used_calls_after_increment,
        "warning_level": result.warning_level,
    })
}

/// Close the context-view turn for a meta-query path, which skips the brain loop.
fn emit_meta_query_turn_finished(
    ctx: &QqChatAgentServiceContext<'_>,
    sender_id: &str,
    turn_id: &str,
    payload: &QqBrainOutput,
    assistant_text: Option<&str>,
) {
    let last_assistant = payload.brain_output.iter().rev().find(|message| {
        matches!(message.role, zihuan_core::model_inference::llm::MessageRole::Assistant)
    });
    let usage = last_assistant.and_then(|message| message.usage.clone());
    let reasoning_text = last_assistant
        .and_then(|message| message.reasoning_content.clone())
        .filter(|text| !text.trim().is_empty());
    zihuan_core::qq_context::emit_qq_context_event(QqChatContextEvent::TurnFinished {
        agent_id: ctx.agent_id.to_string(),
        sender_id: sender_id.to_string(),
        turn_id: turn_id.to_string(),
        assistant_text: assistant_text.map(ToOwned::to_owned),
        reasoning_text,
        usage,
        generation_ms: None,
        output_tokens_per_second: None,
    });
}

fn build_service_model_list(ctx: &QqChatAgentServiceContext<'_>) -> Vec<(String, String)> {
    ctx.llm_roles()
        .into_iter()
        .map(|(role, llm)| (role.to_string(), llm.get_model_name().to_string()))
        .collect()
}

fn build_model_identity_context(ctx: &QqChatAgentServiceContext<'_>) -> ModelIdentityContext {
    ModelIdentityContext {
        framework_name: "紫幻next".to_string(),
        model_list: build_service_model_list(ctx),
    }
}
