use std::sync::{Arc, Mutex};

use log::{info, warn};

use ims_bot_adapter::message_helpers::get_bot_id;

use model_inference::inference_function::compact_message::{
    compact_message_history, estimate_messages_tokens,
};
use model_inference::message_content_utils::{
    downgrade_messages_for_model, sanitize_messages_for_inference,
};

use zihuan_agent::brain::{Brain, BrainStopReason, LongTaskContext};

use zihuan_core::command::{CommandChannel, CommandContext, DispatchResult};
use zihuan_core::agent_config::current_qq_chat_agent_config;
use zihuan_core::error::Result;
use zihuan_core::llm::{OpenAIMessage, TokenUsage};

use zihuan_graph_engine::brain_tool_spec::{
    QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT, QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT,
    QQ_AGENT_TOOL_OWNER_TYPE,
};
use zihuan_graph_engine::DataValue;

use super::super::classify_intent::{classify_intent_with_trace, IntentCategory};
use super::super::qq_chat_agent_logging::QqChatBrainObserver;
use super::super::qq_chat_agent_msg_send::{
    send_planned_batches, take_reply_directive, QqSendContext,
};
use super::super::tools::{
    EditableQqAgentTool, GetAgentPublicInfoBrainTool, GetFunctionListBrainTool,
    GetRecentGroupMessagesBrainTool, GetRecentUserMessagesBrainTool, ImageUnderstandBrainTool,
    ReplyMessageBrainTool, SearchSimilarImagesBrainTool, ToolNotificationTarget, WebSearchBrainTool,
    DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO, DEFAULT_TOOL_GET_FUNCTION_LIST,
    DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES, DEFAULT_TOOL_GET_RECENT_USER_MESSAGES,
    DEFAULT_TOOL_IMAGE_UNDERSTAND, DEFAULT_TOOL_REPLY_MESSAGE, DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES,
    DEFAULT_TOOL_WEB_SEARCH,
    QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS,
};

use crate::nodes::tool_subgraph::{ToolResultMode, ToolSubgraphRunner};
use crate::storage::qq_chat_history_store::{
    conversation_history_key, load_history, save_history,
};
use crate::storage::qq_chat_session_store::build_outbound_persistence;

use super::{
    build_group_system_prompt, build_merged_follow_up_event, build_model_name_reply,
    build_output_contract_priming_message, build_private_system_prompt, build_reply_result,
    build_user_message, collect_available_media_from_brain_output,
    expand_event_for_inference, extract_user_message_text, hydrate_missing_reply_sources,
    message_with_api_style, sender_display_name, send_direct_text_reply,
    summarize_task_text, truncate_for_log, DIRECT_REPLY_NO_SYSTEM_PROMPT, LOG_PREFIX,
    LOG_TEXT_PREVIEW_CHARS, QqChatAgent, QqChatAgentContext, QqChatHandleReport,
    QqLongTaskNotifier, QqChatSteerHook, QqChatTaskTrace, QqChatTurnResult,
    QqCommandSideEffectContext,
};

impl QqChatAgent {
    pub(crate) fn build_command_context(
        &self,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        group_id: Option<i64>,
    ) -> CommandContext {
        CommandContext {
            agent_type: "qq_chat".to_string(),
            agent_id: self.id.clone(),
            caller_id: sender_id.to_string(),
            channel: CommandChannel::QqChat {
                sender_id: sender_id.to_string(),
                is_group,
                group_id,
                target_id: target_id.to_string(),
            },
        }
    }

    pub(crate) fn execute_command_dispatch(
        &self,
        trace: &QqChatTaskTrace,
        cmd_ctx: &CommandContext,
        dispatch_result: DispatchResult,
        hydrated_event: &ims_bot_adapter::models::MessageEvent,
        inference_event: &ims_bot_adapter::models::MessageEvent,
        sender_id: &str,
        target_id: &str,
        bot_id: &str,
        history: &mut Vec<OpenAIMessage>,
        ctx: &QqChatAgentContext<'_>,
    ) -> Result<Option<String>> {
        let DispatchResult {
            result,
            passthrough_text,
        } = dispatch_result;
        let side_effect_ctx = QqCommandSideEffectContext {
            command_context: cmd_ctx,
            cache: ctx.cache,
            adapter: ctx.adapter,
            bot_id,
            bot_name: ctx.bot_name,
            target_id,
            is_group: matches!(cmd_ctx.channel, CommandChannel::QqChat { is_group: true, .. }),
            group_name: hydrated_event.group_name.as_deref(),
            rdb_pool: ctx.rdb_pool,
            mysql_ref: ctx.mysql_ref,
        };

        for effect in &result.side_effects {
            effect.execute(&side_effect_ctx)?;
        }

        if let Some(ref echo) = result.echo_message {
            let is_group = matches!(cmd_ctx.channel, CommandChannel::QqChat { is_group: true, .. });
            let _ = send_direct_text_reply(
                trace,
                ctx.adapter,
                target_id,
                ctx.rdb_pool,
                ctx.mysql_ref,
                hydrated_event.group_name.as_deref(),
                ctx.bot_name,
                bot_id,
                echo,
                is_group,
                sender_id,
                &inference_event.sender.nickname,
                inference_event.sender.card.as_str(),
                ctx.max_message_length,
                ctx.reply_batch_builder,
            )?;
        }

        let has_passthrough = passthrough_text.is_some();
        if result.inject_to_llm {
            let user_msg_for_cmd = message_with_api_style(
                build_user_message(
                    hydrated_event,
                    bot_id,
                    ctx.bot_name,
                    ctx.llm.supports_multimodal_input(),
                    ctx.s3_ref,
                ),
                ctx.llm.api_style(),
            );
            history.push(user_msg_for_cmd);
            history.push(message_with_api_style(
                OpenAIMessage::assistant_text(result.reply),
                ctx.llm.api_style(),
            ));
        }

        if result.inject_to_llm && !has_passthrough {
            let history_key = conversation_history_key(
                bot_id,
                sender_id,
                matches!(cmd_ctx.channel, CommandChannel::QqChat { is_group: true, .. }),
                inference_event.group_id,
            );
            save_history(ctx.cache, &history_key, history.clone());
        }

        Ok(passthrough_text)
    }

    /// Processes a claimed QQ chat message, potentially across multiple turns due to steering.
    ///
    /// Repeatedly calls [`handle_claimed_turn`] and drains any pending steer messages after each
    /// turn. When steer messages exist, they are merged into a follow-up event that becomes the
    /// input for the next iteration. The loop ends once no more steer messages remain.
    ///
    /// Returns a [`QqChatHandleReport`] with a summary of the final turn.
    pub(crate) fn handle_claimed(
        &self,
        trace: &QqChatTaskTrace,
        event: &ims_bot_adapter::models::MessageEvent,
        time: &str,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        ctx: &QqChatAgentContext<'_>,
    ) -> Result<QqChatHandleReport> {
        (|| -> Result<QqChatHandleReport> {
            let bot_id = get_bot_id(ctx.adapter);
            let mut current_event = event.clone();
            let mut current_time = time.to_string();
            let result_summary = loop {
                let turn_result = self.handle_claimed_turn(
                    trace,
                    &current_event,
                    &current_time,
                    sender_id,
                    target_id,
                    is_group,
                    &bot_id,
                    ctx,
                )?;

                let (pending, remaining_queue_len, accepted_steer_count) =
                    ctx.pending_steer.drain_all(sender_id);
                if pending.is_empty() {
                    break turn_result.result_summary;
                }

                let steer_count = pending.len();
                let next_event = build_merged_follow_up_event(&pending);
                let next_inference_event = expand_event_for_inference(&next_event);
                let next_message =
                    extract_user_message_text(&next_inference_event, &bot_id, ctx.bot_name);
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
                    truncate_for_log(&next_message, LOG_TEXT_PREVIEW_CHARS)
                );
                current_event = next_event;
                current_time = pending
                    .last()
                    .map(|event| event.time.clone())
                    .unwrap_or_else(|| current_time.clone());
            };

            Ok(QqChatHandleReport { result_summary })
        })()
    }

    /// Processes a single QQ chat turn end-to-end for a claimed message.
    ///
    /// The lifecycle is:
    /// - **Hydration & extraction** — resolves reply chains and extracts the user text.
    /// - **Command interception** — dispatches slash commands, executes side effects, and
    ///   optionally passes remaining text to the brain loop.
    /// - **Intent classification** — selects the appropriate LLM (general vs math/programming).
    /// - **Short-circuit replies** — answers meta-queries (model name, tool list, etc.) directly.
    /// - **History compaction** — compresses conversation context when it exceeds budget.
    /// - **Brain loop** — builds system prompt + conversation messages, attaches tools, and
    ///   runs the LLM inference loop with steer support.
    /// - **Reply delivery** — parses the final assistant output and sends it back to the user
    ///   (group or private chat), persisting message history along the way.
    ///
    /// Returns a [`QqChatTurnResult`] containing a human-readable summary of what happened.
    fn handle_claimed_turn(
        &self,
        trace: &QqChatTaskTrace,
        event: &ims_bot_adapter::models::MessageEvent,
        time: &str,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        bot_id: &str,
        ctx: &QqChatAgentContext<'_>,
    ) -> Result<QqChatTurnResult> {
        let hydrated_event = hydrate_missing_reply_sources(event, ctx.adapter);
        let inference_event = expand_event_for_inference(&hydrated_event);
        let raw_user_message = extract_user_message_text(&hydrated_event, bot_id, ctx.bot_name);
        let mut current_message = extract_user_message_text(&inference_event, bot_id, ctx.bot_name);
        trace.log_user_message(&raw_user_message, &current_message);

        let history_key =
            conversation_history_key(bot_id, sender_id, is_group, inference_event.group_id);
        let legacy_history_key = sender_id.to_string();
        let mut history = load_history(ctx.cache, &history_key, &legacy_history_key);

        // Intercept command-style messages (e.g. slash commands) before the brain loop.
        // Commands are dispatched synchronously; if `passthrough_text` is present it
        // replaces `current_message` and the brain loop runs with the leftover text.
        if let Some(command_registry) = crate::command::global_command_registry() {
            let cmd_ctx =
                self.build_command_context(sender_id, target_id, is_group, inference_event.group_id);
            if let Some(DispatchResult { result, passthrough_text }) =
                command_registry.dispatch(&cmd_ctx, &raw_user_message)
            {
                if let Some(passthrough) = self.execute_command_dispatch(
                    trace,
                    &cmd_ctx,
                    DispatchResult {
                        result,
                        passthrough_text,
                    },
                    &hydrated_event,
                    &inference_event,
                    sender_id,
                    target_id,
                    bot_id,
                    &mut history,
                    ctx,
                )? {
                    current_message = passthrough;
                } else {
                    return Ok(QqChatTurnResult {
                        result_summary: "已处理命令".to_string(),
                    });
                }
            }
        }

        let intent_trace = classify_intent_with_trace(
            ctx.intent_llm,
            ctx.embedding_model,
            &current_message,
            Some(&history),
            ctx.compact_context_length,
        );
        let intent = intent_trace.category;
        trace.record_intent(intent_trace);

        let selected_llm = match intent {
            IntentCategory::SolveComplexProblem | IntentCategory::WriteCode => {
                ctx.math_programming_llm
            }
            _ => ctx.llm,
        };
        let user_msg = message_with_api_style(
            build_user_message(
                &hydrated_event,
                bot_id,
                ctx.bot_name,
                selected_llm.supports_multimodal_input(),
                ctx.s3_ref,
            ),
            selected_llm.api_style(),
        );

        let mut history = sanitize_messages_for_inference(history);

        let direct_reply = match intent {
            IntentCategory::AskSystemPrompt => Some(DIRECT_REPLY_NO_SYSTEM_PROMPT.to_string()),
            IntentCategory::AskModelName => {
                Some(build_model_name_reply(ctx.model_display_names))
            }
            IntentCategory::AskToolList => crate::command::build_help_text(),
            _ => None,
        };

        if let Some(content) = direct_reply {
            trace.record_history_stats(history.len(), estimate_messages_tokens(&history));
            let visible_assistant_history_text = send_direct_text_reply(
                trace,
                ctx.adapter,
                target_id,
                ctx.rdb_pool,
                ctx.mysql_ref,
                event.group_name.as_deref(),
                ctx.bot_name,
                bot_id,
                &content,
                is_group,
                sender_id,
                &inference_event.sender.nickname,
                inference_event.sender.card.as_str(),
                ctx.max_message_length,
                ctx.reply_batch_builder,
            )?;
            history.push(user_msg);
            if let Some(assistant_text) = visible_assistant_history_text {
                history.push(message_with_api_style(
                    OpenAIMessage::assistant_text(assistant_text),
                    selected_llm.api_style(),
                ));
            }
            save_history(ctx.cache, &history_key, history);
            let result_summary = format!(
                "已直接回复[{sender_id}]，内容：{}",
                summarize_task_text(&content, 80)
            );
            trace.log_result_summary(&result_summary);
            return Ok(QqChatTurnResult { result_summary });
        }

        let compact_result = compact_message_history(
            selected_llm,
            history.clone(),
            ctx.compact_context_length,
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

        let system_prompt = if is_group {
            let group_name = inference_event.group_name.as_deref().unwrap_or("未知");
            build_group_system_prompt(
                ctx.bot_name,
                bot_id,
                time,
                sender_id,
                &sender_display_name(
                    &inference_event.sender.nickname,
                    &inference_event.sender.card,
                ),
                group_name,
                target_id,
                ctx.agent_system_prompt,
            )
        } else {
            build_private_system_prompt(
                ctx.bot_name,
                bot_id,
                time,
                sender_id,
                &sender_display_name(
                    &inference_event.sender.nickname,
                    &inference_event.sender.card,
                ),
                ctx.agent_system_prompt,
            )
        };
        let system_msg = OpenAIMessage::system(system_prompt);
        let priming_msg = build_output_contract_priming_message();

        let shared_runtime_values = Arc::new(Mutex::new(ctx.shared_runtime_values.clone()));
        {
            let mut locked = shared_runtime_values.lock().unwrap();
            locked.insert(
                QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT.to_string(),
                DataValue::MessageEvent(hydrated_event.clone()),
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

        let mut conversation: Vec<OpenAIMessage> = Vec::with_capacity(history.len() + 3);
        conversation.push(system_msg);
        conversation.push(priming_msg);
        conversation.extend(history.iter().cloned());
        conversation.push(user_msg.clone());
        let conversation =
            downgrade_messages_for_model(conversation, selected_llm.supports_multimodal_input());
        let prompt_tokens_estimated = estimate_messages_tokens(&conversation);
        trace.log_llm_conversation(&conversation, prompt_tokens_estimated);

        let consumed_steer_messages = Arc::new(Mutex::new(Vec::new()));
        let mut brain = Brain::new(selected_llm.clone());
        brain.set_observer(Arc::new(QqChatBrainObserver {
            trace: trace.clone(),
        }));
        brain.set_iteration_hook(Arc::new(QqChatSteerHook {
            pending_steer: Arc::clone(ctx.pending_steer),
            sender_id: sender_id.to_string(),
            bot_id: bot_id.to_string(),
            bot_name: ctx.bot_name.to_string(),
            max_steer_count: ctx.max_steer_count,
            llm_supports_multimodal_input: selected_llm.supports_multimodal_input(),
            llm_api_style: selected_llm.api_style().map(ToOwned::to_owned),
            s3_ref: ctx.s3_ref.cloned(),
            trace: trace.clone(),
            consumed_messages: Arc::clone(&consumed_steer_messages),
            shared_runtime_values: Arc::clone(&shared_runtime_values),
        }));

        if self.is_default_tool_enabled(DEFAULT_TOOL_WEB_SEARCH) {
            brain = brain.with_tool(WebSearchBrainTool::new(
                ctx.web_search_engine.clone(),
                ToolNotificationTarget::new(
                    Some(ctx.adapter.clone()),
                    target_id.to_string(),
                    if is_group {
                        Some(sender_id.to_string())
                    } else {
                        None
                    },
                    is_group,
                    false,
                ),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO) {
            brain = brain.with_tool(GetAgentPublicInfoBrainTool::new(current_message.clone()));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_GET_FUNCTION_LIST) {
            brain = brain.with_tool(GetFunctionListBrainTool);
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES) {
            brain = brain.with_tool(GetRecentGroupMessagesBrainTool::new(
                ctx.mysql_ref.cloned(),
                ToolNotificationTarget::new(
                    Some(ctx.adapter.clone()),
                    target_id.to_string(),
                    if is_group {
                        Some(sender_id.to_string())
                    } else {
                        None
                    },
                    is_group,
                    false,
                ),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_GET_RECENT_USER_MESSAGES) {
            brain = brain.with_tool(GetRecentUserMessagesBrainTool::new(
                ctx.mysql_ref.cloned(),
                ToolNotificationTarget::new(
                    Some(ctx.adapter.clone()),
                    target_id.to_string(),
                    if is_group {
                        Some(sender_id.to_string())
                    } else {
                        None
                    },
                    is_group,
                    false,
                ),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES) {
            brain = brain.with_tool(SearchSimilarImagesBrainTool::new(
                ctx.weaviate_image_ref.cloned(),
                ctx.embedding_model.cloned(),
                ctx.web_search_engine.clone(),
                ctx.s3_ref.cloned(),
                ToolNotificationTarget::new(
                    Some(ctx.adapter.clone()),
                    target_id.to_string(),
                    if is_group {
                        Some(sender_id.to_string())
                    } else {
                        None
                    },
                    is_group,
                    false,
                ),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_IMAGE_UNDERSTAND) {
            brain = brain.with_tool(ImageUnderstandBrainTool::new(
                Some(hydrated_event.clone()),
                ctx.mysql_ref.cloned(),
                ctx.s3_ref.cloned(),
                ToolNotificationTarget::new(
                    Some(ctx.adapter.clone()),
                    target_id.to_string(),
                    if is_group {
                        Some(sender_id.to_string())
                    } else {
                        None
                    },
                    is_group,
                    false,
                ),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_REPLY_MESSAGE) {
            brain = brain.with_tool(ReplyMessageBrainTool::new(Arc::clone(
                &shared_runtime_values,
            )));
        }

        let qq_chat_agent_config = current_qq_chat_agent_config()?;
        for tool_def in &self.tool_definitions {
            brain.add_tool(EditableQqAgentTool {
                runner: ToolSubgraphRunner {
                    node_id: self.id.clone(),
                    owner_node_type: QQ_AGENT_TOOL_OWNER_TYPE.to_string(),
                    shared_inputs: self.shared_inputs.clone(),
                    definition: tool_def.clone(),
                    shared_runtime_values: Arc::clone(&shared_runtime_values),
                    qq_chat_agent_config: Some(qq_chat_agent_config.clone()),
                    result_mode: ToolResultMode::SingleString,
                },
            });
        }

        trace.mark_llm_request_started();
        if let Some(task_runtime) = ctx.task_runtime.clone() {
            brain.set_long_task_context(LongTaskContext {
                task_runtime,
                owner_id: Some(sender_id.to_string()),
                agent_id: self.id.clone(),
                agent_name: ctx.bot_name.to_string(),
                task_db_connection_id: ctx.task_db_connection_id.clone(),
                notifier: Arc::new(QqLongTaskNotifier {
                    adapter: ctx.adapter.clone(),
                    target_id: target_id.to_string(),
                    sender_id: sender_id.to_string(),
                    is_group,
                    rdb_pool: ctx.rdb_pool.cloned(),
                    mysql_ref: ctx.mysql_ref.cloned(),
                    group_name: event.group_name.clone(),
                    bot_id: bot_id.to_string(),
                    bot_name: ctx.bot_name.to_string(),
                }),
            });
        }
        let (brain_output, stop_reason) = brain.run(conversation);
        trace.record_llm_final_result(&stop_reason, &brain_output);
        let completion_tokens_estimated = estimate_messages_tokens(&brain_output);
        let exact_token_usage = {
            let mut prompt_tokens = 0usize;
            let mut completion_tokens = 0usize;
            let mut total_tokens = 0usize;
            let mut has_usage = false;
            let mut total_tokens_seen = false;

            for message in &brain_output {
                if let Some(usage) = message.usage.as_ref() {
                    if let Some(value) = usage.prompt_tokens {
                        prompt_tokens = prompt_tokens.saturating_add(value);
                    }
                    if let Some(value) = usage.completion_tokens {
                        completion_tokens = completion_tokens.saturating_add(value);
                    }
                    if let Some(value) = usage.total_tokens {
                        total_tokens = total_tokens.saturating_add(value);
                        total_tokens_seen = true;
                    }
                    has_usage = true;
                }
            }

            if has_usage {
                Some(TokenUsage {
                    prompt_tokens: Some(prompt_tokens),
                    completion_tokens: Some(completion_tokens),
                    total_tokens: if total_tokens_seen {
                        Some(total_tokens)
                    } else {
                        None
                    },
                })
            } else {
                None
            }
        };
        trace.record_token_usage(completion_tokens_estimated, exact_token_usage);

        let last_assistant = brain_output.iter().rev().find(|message| {
            matches!(message.role, zihuan_core::llm::MessageRole::Assistant)
                && message.tool_calls.is_empty()
        });
        let final_assistant_text = last_assistant
            .and_then(|message| message.content_text())
            .map(str::trim)
            .filter(|content| !content.is_empty())
            .map(ToOwned::to_owned);
        let final_assistant_text = match stop_reason {
            BrainStopReason::TransportError(_) => None,
            _ => final_assistant_text,
        };
        trace.record_llm_result_parsed(final_assistant_text.as_deref());

        let available_media = collect_available_media_from_brain_output(&brain_output);
        let reply_directive = take_reply_directive(&shared_runtime_values);
        let mut visible_assistant_history_text = None;

        if let Some(content) = final_assistant_text {
            let reply_result = build_reply_result(
                &content,
                is_group,
                sender_id,
                &inference_event.sender.nickname,
                inference_event.sender.card.as_str(),
                bot_id,
                ctx.bot_name,
                ctx.max_message_length,
                reply_directive,
                Some(inference_event.message_id),
                available_media,
                ctx.reply_batch_builder,
            )?;

            trace.mark_reply_send_started();
            if reply_result.suppress_send {
                trace.record_reply_send(true, false, &reply_result.batches);
            } else if !reply_result.batches.is_empty() {
                let send_ctx = QqSendContext {
                    adapter: ctx.adapter,
                    target_id,
                    is_group,
                    group_name: event.group_name.as_deref(),
                    bot_id,
                    bot_name: ctx.bot_name,
                    mention_target_id: if is_group { Some(sender_id) } else { None },
                    persistence: build_outbound_persistence(
                        ctx.rdb_pool,
                        ctx.mysql_ref,
                        event.group_name.as_deref(),
                        ctx.bot_name,
                    ),
                    max_text_chars: ctx.max_message_length,
                };
                send_planned_batches(&send_ctx, &reply_result.batches);
                trace.record_reply_send(false, true, &reply_result.batches);
                visible_assistant_history_text = Some(if is_group {
                    content.replace("@sender", &format!("@{}", sender_id))
                } else {
                    content
                });
            } else {
                trace.record_reply_send(false, false, &reply_result.batches);
                warn!("{LOG_PREFIX} Brain finished with empty sendable reply content");
            }
        } else {
            match stop_reason {
                BrainStopReason::TransportError(ref err) => {
                    warn!("{LOG_PREFIX} Brain transport error without reply: {err}");
                }
                BrainStopReason::MaxIterationsReached => {
                    warn!("{LOG_PREFIX} Brain exceeded max tool iterations without reply");
                }
                BrainStopReason::Done => {
                    warn!("{LOG_PREFIX} Brain finished without any sendable reply content");
                }
            }
        }

        history.push(user_msg);
        history.extend(consumed_steer_messages.lock().unwrap().iter().cloned());
        if let Some(ref assistant_text) = visible_assistant_history_text {
            history.push(message_with_api_style(
                OpenAIMessage::assistant_text(assistant_text.clone()),
                selected_llm.api_style(),
            ));
        }
        save_history(ctx.cache, &history_key, history);

        let result_summary = if let Some(ref assistant_text) = visible_assistant_history_text {
            format!(
                "已回复[{sender_id}]，内容：{}",
                summarize_task_text(assistant_text, 80)
            )
        } else if matches!(stop_reason, BrainStopReason::TransportError(_)) {
            format!("回复[{sender_id}]失败：模型请求异常")
        } else {
            format!("已处理[{sender_id}]的消息，但未发送回复")
        };
        trace.log_result_summary(&result_summary);

        Ok(QqChatTurnResult { result_summary })
    }
}
