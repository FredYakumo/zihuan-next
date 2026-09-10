use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use log::{info, warn};

use zihuan_core::model_inference::inference_function::compact_message::{
    compact_message_history, compaction_threshold, estimate_messages_tokens,
};
use zihuan_core::model_inference::message_content_utils::{
    downgrade_messages_for_model, sanitize_messages_for_inference,
};
use zihuan_core::system_config::current_context_compaction_percent;

use zihuan_core::agent::tools::ToolCallingStopReason;

use crate::agent::emotion::utils::{
    emotion_expression_prompt, has_noticeable_emotion_expression,
};
use crate::qq_chat::resources::current_qq_chat_role_service_config;
use crate::qq_session_state::QqChatSessionState;
use crate::role_config::QqChatEmotionDimensionConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::steer::message_with_api_style;

use zihuan_core::graph::tool_spec::{
    QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT, QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT,
};
use zihuan_core::graph::DataValue;

use super::super::super::tools::{
    format_public_info_message, AgentMemoryBackend, AgentMemoryToolResources, ModelIdentityContext,
    QqReplyReviewRequest, QqReplyReviewResult, QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS,
};
use zihuan_core::storage::AgentMemoryAccessContext;

use crate::storage::qq_chat_history_store::{
    chat_preprompt_history_key, conversation_history_key, load_history, save_history,
};

use crate::classify_intent::{classify_intent_with_trace, IntentCategory};
use crate::qq_chat::command::{run_command_pipeline, CommandTurnEnd};
use crate::qq_chat::msg_send::{
    build_reply_result, send_planned_batches, take_reply_directive, QqChatServiceSendContext,
};

use super::{
    build_group_system_prompt, build_meta_query_system_prompt, build_meta_query_user_message,
    build_private_system_prompt, build_user_message, collect_available_media_from_brain_output,
    expand_messages_for_inference, prepare_current_turn_user_input,
    prepare_current_turn_user_input_from_event, QqChatAgentServiceContext, QqChatAgentServiceInner,
    QqChatServiceTurnResult, QqChatTaskTrace, LOG_PREFIX,
};

use crate::agent::before_brain_agent::PrepromptContext;
use crate::procedure::{
    qq_procedure_context, run_before_brain, QqAfterBrain, QqAfterBrainContext, QqBrain,
    QqBrainOutput, QqMetaQueryBrain,
};
use zihuan_core::role::procedure::execute_blocking_procedure_chain;
use zihuan_core::runtime::block_async;

impl QqChatAgentServiceInner {
    /// Returns the last assistant text that carries no tool calls, skipping transport
    /// errors and awaiting-user-input stops. Empty or whitespace-only text yields `None`.
    fn parse_final_reply_text(
        &self,
        stop_reason: &ToolCallingStopReason,
        brain_output: &[LLMMessage],
    ) -> Option<String> {
        if matches!(
            stop_reason,
            ToolCallingStopReason::TransportError(_)
                | ToolCallingStopReason::AwaitUserInput(_)
                | ToolCallingStopReason::ToolCallLimitReached(_)
        ) {
            return None;
        }

        brain_output
            .iter()
            .rev()
            .find(|message| {
                matches!(message.role, zihuan_core::model_inference::llm::MessageRole::Assistant)
                    && message.tool_calls.is_empty()
            })
            .and_then(|message| message.content_text())
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
    }

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
    /// - **Intent classification** — selects the appropriate LLM (general vs math/programming).
    /// - **Short-circuit replies** — answers meta-queries (model name, tool list, etc.) directly.
    /// - **History compaction** — compresses conversation context when it exceeds budget.
    /// - **ToolCallingEngine loop** — builds system prompt + conversation messages, attaches tools, and
    ///   runs the LLM inference loop with steer support.
    /// - **Reply delivery** — parses the final assistant output and sends it back to the user
    ///   (group or private chat), persisting message history along the way.
    ///
    /// Returns a [`QqChatServiceTurnResult`] containing a human-readable summary of what happened.
    pub(crate) fn handle_claimed_turn(
        &self,
        trace: &QqChatTaskTrace,
        event: &zihuan_core::ims_bot_adapter::models::MessageEvent,
        _time: &str,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        bot_id: &str,
        message_rate_limit_warning: Option<&str>,
        ctx: &QqChatAgentServiceContext<'_>,
    ) -> Result<QqChatServiceTurnResult> {
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

        let chat_preprompt_history_key = chat_preprompt_history_key(sender_id);
        let preprompt_memory_backend =
            ctx.local_memory_store.cloned().map(AgentMemoryBackend::LocalFile).or_else(|| {
                ctx.elasticsearch_memory_ref
                    .cloned()
                    .map(AgentMemoryBackend::Elasticsearch)
                    .or_else(|| ctx.weaviate_memory_ref.cloned().map(AgentMemoryBackend::Weaviate))
            });
        let preprompt_memory_resources = preprompt_memory_backend.and_then(|memory_backend| {
            let embedding_model = ctx.embedding_model.cloned();
            if !matches!(memory_backend, AgentMemoryBackend::LocalFile(_))
                && embedding_model.is_none()
            {
                return None;
            }
            Some(AgentMemoryToolResources {
                memory_backend,
                embedding_model,
                llm: Arc::clone(ctx.llm),
                access: AgentMemoryAccessContext {
                    sender_id: Some(sender_id.to_string()),
                    group_id: if is_group {
                        Some(target_id.to_string())
                    } else {
                        prepared_input.event.group_id.map(|value| value.to_string())
                    },
                    is_group,
                    admin: false,
                    skip_expiry_extend: false,
                },
            })
        });
        let preprompt_context = run_before_brain(
            &mut qq_procedure_context(&chat_preprompt_history_key, None),
            PrepromptContext {
                trace,
                llm: ctx.natural_language_reply_llm,
                cache: ctx.cache,
                history_key: &chat_preprompt_history_key,
                input: &prepared_input,
                bot_name: ctx.bot_name,
                bot_id,
                agent_id: ctx.agent_id,
                sender_id,
                target_id,
                is_group,
                session_state: Arc::clone(&turn_session_state),
                emotion_dimensions: emotion_dimensions.clone(),
                memory_resources: preprompt_memory_resources,
                rdb_pool: ctx.rdb_pool.cloned(),
                default_tools_enabled: &self.default_tools_enabled,
            },
        )?;
        trace.record_graph_phase(
            "Preprompt 阶段",
            serde_json::json!({
                "context": preprompt_context,
                "includes": ["名词处理", "情绪维度处理", "情绪提示词生成", "最近消息与记忆查询"],
            }),
        );

        let base_system_prompt = if is_group {
            build_group_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
        } else {
            build_private_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
        };
        trace.record_graph_phase(
            "情绪提示词生成",
            serde_json::json!({
                "emotion_dimensions": emotion_dimensions,
                "preprompt_context": preprompt_context,
            }),
        );

        let intent_trace = classify_intent_with_trace(
            ctx.intent_classification_llm,
            ctx.embedding_model,
            &current_message,
            Some(&history),
            800,
        );
        let (turn_llm, routed_model) = self.selected_turn_llm(ctx, intent_trace.category);
        trace.record_intent_classification(&intent_trace, routed_model);

        let user_msg = {
            let mut session_state = turn_session_state.lock().unwrap();
            message_with_api_style(
                build_user_message(
                    &prepared_input,
                    ctx.bot_name,
                    ctx.adapter,
                    turn_llm.supports_multimodal_input(),
                    &base_system_prompt,
                    ctx.resolved_language_style.as_ref().map(|item| item.style_prompt.as_str()),
                    message_rate_limit_warning,
                    &mut session_state,
                    &emotion_dimensions,
                    preprompt_context.as_deref(),
                ),
                turn_llm.api_style(),
            )
        };

        let mut history = sanitize_messages_for_inference(history);
        let compact_result = compact_message_history(
            turn_llm,
            history.clone(),
            compaction_threshold(turn_llm.context_length(), current_context_compaction_percent()),
            &user_msg,
        );
        if compact_result.did_compact {
            info!(
                "{LOG_PREFIX} history compacted for {history_key}: tokens {} -> {}",
                compact_result.estimated_tokens_before, compact_result.estimated_tokens_after
            );
            history = compact_result.messages;
            save_history(ctx.cache, &history_key, history.clone());
        }
        trace.record_history_stats(history.len(), estimate_messages_tokens(&history));

        if matches!(
            intent_trace.category,
            IntentCategory::AskToolList
                | IntentCategory::AskSystemPrompt
                | IntentCategory::AskModelName
        ) {
            info!(
                "{LOG_PREFIX} meta-query short-circuit for sender={sender_id}, intent={}",
                intent_trace.category.label()
            );
            return self.handle_meta_query_turn(
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
            );
        }

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

        let mut conversation: Vec<LLMMessage> = Vec::with_capacity(history.len() + 1);
        conversation.extend(history.iter().cloned());
        conversation.push(user_msg.clone());
        let brain_conversation =
            downgrade_messages_for_model(conversation, turn_llm.supports_multimodal_input());
        let prompt_tokens_estimated = estimate_messages_tokens(&brain_conversation);
        trace.log_llm_conversation(&brain_conversation, prompt_tokens_estimated);

        let consumed_steer_messages = Arc::new(Mutex::new(Vec::new()));
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
            event.group_name.clone(),
            brain_conversation,
            base_system_prompt,
            Arc::clone(&shared_runtime_values),
            Arc::clone(&consumed_steer_messages),
            Arc::clone(&turn_session_state),
            emotion_dimensions.clone(),
            preprompt_context.clone(),
        )?;
        // Role turn chain (documents/procedure.md): brain -> AfterBrain procedures. The
        // before-brain procedure (preprompt) ran earlier above because its output feeds the
        // turn's user-message construction.
        let mut shared = qq_procedure_context(&history_key, Some(current_message.clone()));
        let turn_outputs = block_async(execute_blocking_procedure_chain(
            vec![
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
        ))?;
        let brain_output_payload =
            shared.find_output::<QqBrainOutput>().map(|value| (*value).clone()).ok_or_else(
                || Error::ValidationError("qq brain procedure produced no turn output".to_string()),
            )?;

        let mut visible_assistant_history_text = None;
        let mut explicit_no_reply = false;
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
        } else if brain_output_payload.final_reply_text.is_some() {
            if brain_output_payload.suppress_send {
                explicit_no_reply = true;
            } else {
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
                )?;

                trace.mark_reply_send_started();
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
                    visible_assistant_history_text = Some(review_result.final_message);
                }
            }
        }

        history.push(user_msg);
        history.extend(consumed_steer_messages.lock().unwrap().iter().cloned());
        if let Some(ref assistant_text) = visible_assistant_history_text {
            history.push(message_with_api_style(
                LLMMessage::assistant_text(assistant_text.clone()),
                turn_llm.api_style(),
            ));
        }
        save_history(ctx.cache, &history_key, history);
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

        Ok(QqChatServiceTurnResult { result_summary })
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
        mut history: Vec<LLMMessage>,
        turn_session_state: &Arc<Mutex<QqChatSessionState>>,
        emotion_dimensions: &[QqChatEmotionDimensionConfig],
    ) -> Result<QqChatServiceTurnResult> {
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
            return Ok(QqChatServiceTurnResult {
                result_summary: format!("元查询[{sender_id}]：LLM未返回有效回复"),
            });
        };

        if meta_payload.suppress_send {
            history.push(message_with_api_style(
                LLMMessage::user(current_message.to_string()),
                ctx.llm.api_style(),
            ));
            save_history(ctx.cache, history_key, history);
            *ctx.session_state_store.lock().unwrap() = turn_session_state.lock().unwrap().clone();
            trace.record_final_reply_decision(Some(candidate_message), Some(true), None);
            return Ok(QqChatServiceTurnResult {
                result_summary: format!("已处理[{sender_id}]的元查询，显式选择不回复"),
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
            visible_assistant_history_text = Some(review_result.final_message);
        }

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

        Ok(QqChatServiceTurnResult { result_summary })
    }
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
