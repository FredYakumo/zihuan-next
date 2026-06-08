use std::sync::{Arc, Mutex};

use log::{info, warn};

use model_inference::inference_function::compact_message::{compact_message_history, estimate_messages_tokens};
use model_inference::message_content_utils::{downgrade_messages_for_model, sanitize_messages_for_inference};

use zihuan_agent::brain::{Brain, BrainStopReason, LongTaskContext};

use zihuan_core::agent_config::current_qq_chat_agent_config;
use zihuan_core::command::{CommandChannel, CommandContext, DispatchResult};
use zihuan_core::error::Result;
use zihuan_core::llm::{LLMMessage, TokenUsage};
use zihuan_core::steer::message_with_api_style;

use zihuan_graph_engine::brain_tool_spec::{
    QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT, QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT, QQ_AGENT_TOOL_OWNER_TYPE,
};
use zihuan_graph_engine::DataValue;

use super::super::qq_chat_agent_logging::QqChatBrainObserver;
use ims_bot_adapter::tools::qq_profile::{GetBotProfileBrainTool, GetQqUserProfileBrainTool};

use super::super::tools::{
    take_last_reply_result, AgentMemoryToolResources, CurrentTimeBrainTool, EditableQqAgentTool,
    GetAgentPublicInfoBrainTool, GetFunctionListBrainTool, GetRecentGroupMessagesBrainTool,
    GetRecentUserMessagesBrainTool, ImageUnderstandBrainTool, ListAvailableMemoryKeysBrainTool,
    RememberContentBrainTool, RunResearchSubagentBrainTool, SearchMemoryContentBrainTool, SearchSimilarImagesBrainTool,
    SendNaturalLanguageReplyBrainTool, ToolNotificationTarget, UpdateAgentStateBrainTool, WebSearchBrainTool,
    DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO, DEFAULT_TOOL_GET_FUNCTION_LIST, DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES,
    DEFAULT_TOOL_GET_RECENT_USER_MESSAGES, DEFAULT_TOOL_IMAGE_UNDERSTAND, DEFAULT_TOOL_LIST_AVAILABLE_MEMORY_KEYS,
    DEFAULT_TOOL_REMEMBER_CONTENT, DEFAULT_TOOL_SEARCH_MEMORY_CONTENT, DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES,
    DEFAULT_TOOL_WEB_SEARCH, QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS,
};
use storage_handler::AgentMemoryAccessContext;

use crate::nodes::tool_subgraph::{ToolResultMode, ToolSubgraphRunner};
use crate::storage::qq_chat_history_store::{conversation_history_key, load_history, save_history};

use crate::agent::qq_chat_agent_msg_send::send_direct_text_reply;

use super::{
    build_group_system_prompt, build_private_system_prompt, build_user_message, expand_messages_for_inference,
    prepare_current_turn_user_input, prepare_current_turn_user_input_from_event, QqChatAgent, QqChatAgentContext,
    QqChatTaskTrace, QqChatTurnResult, QqCommandSideEffectContext, QqLongTaskNotifier, LOG_PREFIX,
    LOG_TEXT_PREVIEW_CHARS,
};
use zihuan_core::utils::string_utils::shorten_text;

use super::super::qq_chat_agent_steer::QqChatSteerHook;

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
        history: &mut Vec<LLMMessage>,
        ctx: &QqChatAgentContext<'_>,
    ) -> Result<Option<String>> {
        let DispatchResult { result, passthrough_text } = dispatch_result;
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
            let is_group = matches!(cmd_ctx.channel, CommandChannel::QqChat { is_group: true, .. });
            let cmd_system_prompt = if is_group {
                build_group_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
            } else {
                build_private_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
            };
            let cmd_session_state = ctx.session_state_store.lock().unwrap().clone();
            let cmd_emotion_dimensions = current_qq_chat_agent_config()?.resolved_emotion_dimensions();

            let user_msg_for_cmd = message_with_api_style(
                build_user_message(
                    &prepare_current_turn_user_input_from_event(hydrated_event, bot_id, ctx.bot_name, ctx.s3_ref),
                    ctx.bot_name,
                    ctx.llm.supports_multimodal_input(),
                    &cmd_system_prompt,
                    &cmd_session_state,
                    &cmd_emotion_dimensions,
                ),
                ctx.llm.api_style(),
            );
            history.push(user_msg_for_cmd);
            history.push(message_with_api_style(
                LLMMessage::assistant_text(result.reply),
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
    pub(crate) fn handle_claimed_turn(
        &self,
        trace: &QqChatTaskTrace,
        event: &ims_bot_adapter::models::MessageEvent,
        _time: &str,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        bot_id: &str,
        ctx: &QqChatAgentContext<'_>,
    ) -> Result<QqChatTurnResult> {
        let prepared_input = prepare_current_turn_user_input(event, ctx.adapter, bot_id, ctx.bot_name, ctx.s3_ref);
        let mut inference_event = prepared_input.event.clone();
        inference_event.message_list = expand_messages_for_inference(&prepared_input.event.message_list);
        let inference_input =
            prepare_current_turn_user_input_from_event(&inference_event, bot_id, ctx.bot_name, ctx.s3_ref);
        let raw_user_message = prepared_input.current_text_for_prompt().to_string();
        let mut current_message = inference_input.current_text_for_prompt().to_string();
        trace.log_user_message(&raw_user_message, &current_message);

        let history_key = conversation_history_key(bot_id, sender_id, is_group, inference_event.group_id);
        let legacy_history_key = sender_id.to_string();
        let mut history = load_history(ctx.cache, &history_key, &legacy_history_key);

        // Intercept command-style messages (e.g. slash commands) before the brain loop.
        // Commands are dispatched synchronously; if `passthrough_text` is present it
        // replaces `current_message` and the brain loop runs with the leftover text.
        if let Some(command_registry) = crate::command::global_command_registry() {
            let cmd_ctx = self.build_command_context(sender_id, target_id, is_group, inference_event.group_id);
            if let Some(DispatchResult { result, passthrough_text }) =
                command_registry.dispatch(&cmd_ctx, &raw_user_message)
            {
                if let Some(passthrough) = self.execute_command_dispatch(
                    trace,
                    &cmd_ctx,
                    DispatchResult { result, passthrough_text },
                    &prepared_input.event,
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

        let current_session_state = { ctx.session_state_store.lock().unwrap().clone() };
        let emotion_dimensions = current_qq_chat_agent_config()?.resolved_emotion_dimensions();
        let mut current_session_state = current_session_state;
        current_session_state.sync_emotion_dimensions(&emotion_dimensions);
        let turn_session_state = Arc::new(Mutex::new(current_session_state));

        let system_prompt = if is_group {
            build_group_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
        } else {
            build_private_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
        };

        let user_msg = message_with_api_style(
            build_user_message(
                &prepared_input,
                ctx.bot_name,
                ctx.llm.supports_multimodal_input(),
                &system_prompt,
                &turn_session_state.lock().unwrap(),
                &emotion_dimensions,
            ),
            ctx.llm.api_style(),
        );

        let mut history = sanitize_messages_for_inference(history);
        let compact_result = compact_message_history(ctx.llm, history.clone(), ctx.compact_context_length, &user_msg);
        if compact_result.did_compact {
            info!(
                "{LOG_PREFIX} history compacted for {history_key}: tokens {} -> {}",
                compact_result.estimated_tokens_before, compact_result.estimated_tokens_after
            );
            history = compact_result.messages;
            save_history(ctx.cache, &history_key, history.clone());
        }
        trace.record_history_stats(history.len(), estimate_messages_tokens(&history));

        let shared_runtime_values = Arc::new(Mutex::new(ctx.shared_runtime_values.clone()));
        {
            let mut locked = shared_runtime_values.lock().unwrap();
            locked.insert(
                QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT.to_string(),
                DataValue::MessageEvent(prepared_input.event.clone()),
            );
            let adapter_handle: zihuan_core::ims_bot_adapter::BotAdapterHandle = ctx.adapter.clone();
            locked.insert(
                QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT.to_string(),
                DataValue::BotAdapterRef(adapter_handle),
            );
            locked.insert(QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS.to_string(), DataValue::Boolean(false));
        }

        let mut conversation: Vec<LLMMessage> = Vec::with_capacity(history.len() + 1);
        conversation.extend(history.iter().cloned());
        conversation.push(user_msg.clone());
        let mut brain_conversation = downgrade_messages_for_model(conversation, ctx.llm.supports_multimodal_input());
        let prompt_tokens_estimated = estimate_messages_tokens(&brain_conversation);
        trace.log_llm_conversation(&brain_conversation, prompt_tokens_estimated);

        let consumed_steer_messages = Arc::new(Mutex::new(Vec::new()));
        let mut brain = Brain::new(Arc::clone(ctx.llm));
        brain.add_tool(CurrentTimeBrainTool);
        brain.set_observer(Arc::new(QqChatBrainObserver { trace: trace.clone() }));
        brain.set_iteration_hook(Arc::new(QqChatSteerHook {
            pending_steer: Arc::clone(ctx.pending_steer),
            sender_id: sender_id.to_string(),
            bot_id: bot_id.to_string(),
            bot_name: ctx.bot_name.to_string(),
            max_steer_count: ctx.max_steer_count,
            llm_supports_multimodal_input: ctx.llm.supports_multimodal_input(),
            llm_api_style: ctx.llm.api_style().map(ToOwned::to_owned),
            s3_ref: ctx.s3_ref.cloned(),
            trace: trace.clone(),
            consumed_messages: Arc::clone(&consumed_steer_messages),
            shared_runtime_values: Arc::clone(&shared_runtime_values),
            system_prompt: system_prompt.clone(),
            session_state: Arc::clone(&turn_session_state),
            emotion_dimensions: emotion_dimensions.clone(),
        }));

        if let (Some(memory_ref), Some(embedding_model)) =
            (ctx.weaviate_memory_ref.cloned(), ctx.embedding_model.cloned())
        {
            let memory_resources = AgentMemoryToolResources {
                memory_ref,
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
            };
            if self.is_default_tool_enabled(DEFAULT_TOOL_LIST_AVAILABLE_MEMORY_KEYS) {
                brain = brain.with_tool(ListAvailableMemoryKeysBrainTool::new(memory_resources.clone()));
            }
            if self.is_default_tool_enabled(DEFAULT_TOOL_SEARCH_MEMORY_CONTENT) {
                brain = brain.with_tool(SearchMemoryContentBrainTool::new(memory_resources.clone()));
            }
            if self.is_default_tool_enabled(DEFAULT_TOOL_REMEMBER_CONTENT) {
                brain = brain.with_tool(RememberContentBrainTool::new(memory_resources));
            }
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_WEB_SEARCH) {
            brain = brain.with_tool(WebSearchBrainTool::new(
                ctx.web_search_engine.clone(),
                ToolNotificationTarget::new(
                    Some(ctx.adapter.clone()),
                    target_id.to_string(),
                    if is_group { Some(sender_id.to_string()) } else { None },
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

        brain = brain.with_tool(UpdateAgentStateBrainTool::new(
            Arc::clone(&turn_session_state),
            emotion_dimensions.clone(),
        ));
        brain = brain.with_tool(RunResearchSubagentBrainTool::new(
            Arc::clone(ctx.math_programming_llm),
            Arc::clone(ctx.web_search_engine),
            ctx.mysql_ref.cloned(),
            ctx.s3_ref.cloned(),
            Some(prepared_input.event.clone()),
            ToolNotificationTarget::dashboard(),
            if let (Some(memory_ref), Some(embedding_model)) =
                (ctx.weaviate_memory_ref.cloned(), ctx.embedding_model.cloned())
            {
                Some(AgentMemoryToolResources {
                    memory_ref,
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
            } else {
                None
            },
        ));
        brain = brain.with_tool(SendNaturalLanguageReplyBrainTool::new(
            ctx.adapter.clone(),
            target_id.to_string(),
            is_group,
            event.group_name.clone(),
            bot_id.to_string(),
            ctx.bot_name.to_string(),
            sender_id.to_string(),
            inference_event.sender.nickname.clone(),
            inference_event.sender.card.clone(),
            Arc::clone(ctx.natural_language_reply_llm),
            ctx.natural_language_reply_system_prompt.map(ToOwned::to_owned),
            Arc::clone(&turn_session_state),
            emotion_dimensions.clone(),
            Arc::clone(&shared_runtime_values),
            ctx.reply_batch_builder.cloned(),
            ctx.max_message_length,
            Some(inference_event.message_id),
            ctx.rdb_pool.cloned(),
            ctx.mysql_ref.cloned(),
            trace.clone(),
        ));

        if self.is_default_tool_enabled(DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES) {
            brain = brain.with_tool(GetRecentGroupMessagesBrainTool::new(
                ctx.mysql_ref.cloned(),
                ToolNotificationTarget::new(
                    Some(ctx.adapter.clone()),
                    target_id.to_string(),
                    if is_group { Some(sender_id.to_string()) } else { None },
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
                    if is_group { Some(sender_id.to_string()) } else { None },
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
                    if is_group { Some(sender_id.to_string()) } else { None },
                    is_group,
                    false,
                ),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_IMAGE_UNDERSTAND) {
            brain = brain.with_tool(ImageUnderstandBrainTool::new(
                Some(prepared_input.event.clone()),
                ctx.mysql_ref.cloned(),
                ctx.s3_ref.cloned(),
                ToolNotificationTarget::new(
                    Some(ctx.adapter.clone()),
                    target_id.to_string(),
                    if is_group { Some(sender_id.to_string()) } else { None },
                    is_group,
                    false,
                ),
            ));
        }

        brain = brain.with_tool(GetBotProfileBrainTool::new(
            ctx.adapter.clone(),
            prepared_input.event.clone(),
            ctx.s3_ref.cloned(),
        ));
        brain = brain.with_tool(GetQqUserProfileBrainTool::new(
            ctx.adapter.clone(),
            prepared_input.event.clone(),
            ctx.s3_ref.cloned(),
        ));

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
        let mut brain_output;
        let mut stop_reason;
        (brain_output, stop_reason) = brain.run(brain_conversation.clone());
        trace.record_llm_final_result(&stop_reason, &brain_output);
        let completion_tokens_estimated = estimate_messages_tokens(&brain_output);
        let exact_token_usage = {
            let mut prompt_tokens = 0usize;
            let mut cached_prompt_tokens = 0usize;
            let mut prompt_cache_miss_tokens = 0usize;
            let mut completion_tokens = 0usize;
            let mut total_tokens = 0usize;
            let mut has_usage = false;
            let mut cached_prompt_tokens_seen = false;
            let mut prompt_cache_miss_tokens_seen = false;
            let mut total_tokens_seen = false;

            for message in &brain_output {
                if let Some(usage) = message.usage.as_ref() {
                    if let Some(value) = usage.prompt_tokens {
                        prompt_tokens = prompt_tokens.saturating_add(value);
                    }
                    if let Some(value) = usage.cached_prompt_tokens {
                        cached_prompt_tokens = cached_prompt_tokens.saturating_add(value);
                        cached_prompt_tokens_seen = true;
                    }
                    if let Some(value) = usage.prompt_cache_miss_tokens {
                        prompt_cache_miss_tokens = prompt_cache_miss_tokens.saturating_add(value);
                        prompt_cache_miss_tokens_seen = true;
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
                    cached_prompt_tokens: if cached_prompt_tokens_seen {
                        Some(cached_prompt_tokens)
                    } else {
                        None
                    },
                    prompt_cache_miss_tokens: if prompt_cache_miss_tokens_seen {
                        Some(prompt_cache_miss_tokens)
                    } else {
                        None
                    },
                    completion_tokens: Some(completion_tokens),
                    total_tokens: if total_tokens_seen { Some(total_tokens) } else { None },
                })
            } else {
                None
            }
        };
        trace.record_token_usage(completion_tokens_estimated, exact_token_usage);

        let mut last_assistant = brain_output.iter().rev().find(|message| {
            matches!(message.role, zihuan_core::llm::MessageRole::Assistant) && message.tool_calls.is_empty()
        });
        let mut final_assistant_text = last_assistant
            .and_then(|message| message.content_text())
            .map(str::trim)
            .filter(|content| !content.is_empty())
            .map(ToOwned::to_owned);
        final_assistant_text = match stop_reason {
            BrainStopReason::TransportError(_) => None,
            _ => final_assistant_text,
        };

        let mut visible_assistant_history_text = take_last_reply_result(&shared_runtime_values)
            .and_then(|result| result.visible_reply_text)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());

        if visible_assistant_history_text.is_none() && matches!(stop_reason, BrainStopReason::Done) {
            info!(
                "{LOG_PREFIX} Brain finished without reply tool output; requesting one more internal reflection for sender={sender_id}"
            );
            brain_conversation.extend(brain_output.iter().cloned());
            brain_conversation.push(message_with_api_style(
                LLMMessage::user(
                    "【系统补充提醒】你刚才还没有真正回复用户。请再次思考：是否需要调用工具来完成对用户的最终回复，尤其是 `send_natural_language_reply`。如果你判断这条消息确实不需要回复，可以选择仍然不调用任何回复工具。"
                        .to_string(),
                ),
                ctx.llm.api_style(),
            ));

            let (second_output, second_stop_reason) = brain.run(brain_conversation.clone());
            trace.record_llm_final_result(&second_stop_reason, &second_output);
            brain_output.extend(second_output.iter().cloned());
            stop_reason = second_stop_reason;
            last_assistant = second_output.iter().rev().find(|message| {
                matches!(message.role, zihuan_core::llm::MessageRole::Assistant) && message.tool_calls.is_empty()
            });
            final_assistant_text = last_assistant
                .and_then(|message| message.content_text())
                .map(str::trim)
                .filter(|content| !content.is_empty())
                .map(ToOwned::to_owned);
            final_assistant_text = match stop_reason {
                BrainStopReason::TransportError(_) => None,
                _ => final_assistant_text,
            };
            visible_assistant_history_text = take_last_reply_result(&shared_runtime_values)
                .and_then(|result| result.visible_reply_text)
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty());
        }

        trace.record_llm_result_parsed(final_assistant_text.as_deref());

        if visible_assistant_history_text.is_none() {
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
                LLMMessage::assistant_text(assistant_text.clone()),
                ctx.llm.api_style(),
            ));
        }
        save_history(ctx.cache, &history_key, history);
        *ctx.session_state_store.lock().unwrap() = turn_session_state.lock().unwrap().clone();

        let result_summary = if let Some(ref assistant_text) = visible_assistant_history_text {
            format!(
                "已回复[{sender_id}]，内容：{}",
                zihuan_core::utils::string_utils::shorten_text(assistant_text, 80)
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
