use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use log::{info, warn};

use model_inference::inference_function::compact_message::{compact_message_history, estimate_messages_tokens};
use model_inference::message_content_utils::{downgrade_messages_for_model, sanitize_messages_for_inference};

use zihuan_agent::brain::{Brain, BrainStopReason, LongTaskContext};

use zihuan_agent::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::qq_chat::current_qq_chat_agent_service_config;
use zihuan_core::agent_config::qq_chat::QqChatEmotionDimensionConfig;
use zihuan_core::command::{CommandChannel, CommandContext, DispatchResult};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::{InferenceParam, LLMMessage, TokenUsage};
use zihuan_core::steer::message_with_api_style;
use zihuan_core::task_context::AgentTaskRequest;

use zihuan_graph_engine::brain_tool_spec::{
    QQ_AGENT_TOOL_FIXED_BOT_ADAPTER_INPUT, QQ_AGENT_TOOL_FIXED_MESSAGE_EVENT_INPUT, QQ_AGENT_TOOL_OWNER_TYPE,
};
use zihuan_graph_engine::DataValue;

use super::super::logging::QqChatBrainObserver;
use ims_bot_adapter::tools::group_members::GetCurrentGroupMembersBrainTool;
use ims_bot_adapter::tools::qq_profile::{GetBotProfileBrainTool, GetQqUserProfileBrainTool};

use super::super::super::tools::{
    format_public_info_message, review_and_rewrite_reply, AgentMemoryToolResources, EditableQqAgentTool,
    GetAgentPublicInfoBrainTool, GetFunctionListBrainTool, GetRecentGroupMessagesBrainTool,
    GetRecentUserMessagesBrainTool, ImageUnderstandBrainTool, ListAvailableMemoryKeysBrainTool, ModelIdentityContext,
    QqReplyReviewRequest, RememberContentBrainTool, ReplyMessageBrainTool, RunResearchSubagentBrainTool,
    SaveImageBrainTool, SearchMemoryContentBrainTool, SearchSimilarImagesBrainTool, ToolNotificationTarget,
    WebSearchBrainTool, DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO, DEFAULT_TOOL_GET_FUNCTION_LIST,
    DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES, DEFAULT_TOOL_GET_RECENT_USER_MESSAGES, DEFAULT_TOOL_IMAGE_UNDERSTAND,
    DEFAULT_TOOL_LIST_AVAILABLE_MEMORY_KEYS, DEFAULT_TOOL_REMEMBER_CONTENT, DEFAULT_TOOL_SAVE_IMAGE,
    DEFAULT_TOOL_SEARCH_MEMORY_CONTENT, DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES, DEFAULT_TOOL_WEB_SEARCH,
    QQ_CHAT_EMIT_TOOL_PROGRESS_NOTIFICATIONS,
};
use storage_handler::AgentMemoryAccessContext;

use crate::nodes::tool_subgraph::{ToolResultMode, ToolSubgraphRunner};
use crate::storage::qq_chat_history_store::{
    conversation_history_key, emotion_history_key, load_history, save_history,
};

use crate::agent::classify_intent::{classify_intent_with_trace, IntentCategory};
use crate::agent::qq_chat::msg_send::{
    build_reply_result, send_direct_text_reply, send_planned_batches, take_reply_directive, QqChatServiceSendContext,
};

use super::{
    build_group_system_prompt, build_meta_query_system_prompt, build_meta_query_user_message,
    build_private_system_prompt, build_user_message, collect_available_media_from_brain_output,
    expand_messages_for_inference, prepare_current_turn_user_input, prepare_current_turn_user_input_from_event,
    QqChatAgentServiceContext, QqChatAgentServiceInner, QqChatServiceTurnResult, QqChatTaskTrace,
    QqCommandSideEffectContext, QqLongTaskNotifier, LOG_PREFIX, LOG_TEXT_PREVIEW_CHARS,
};

use super::super::emotion::run_emotion_agent;

use super::super::steer::QqChatServiceSteerHook;
use super::super::tool_quota::wrap_brain_tool_with_quota;
use crate::agent::qq_chat::language_style_store::LanguageStyleScope;
use crate::agent::qq_chat::privilege_gate::{
    enqueue_pending_privileged_command, handle_auth_command, parse_privileged_command, AuthCommandOutcome,
    PrivilegeGateOutcome, QqPrivilegedCommand,
};
use crate::agent::qq_chat::style_learner::{
    execute_style_learning_task, OwnedStyleLearningTaskContext, StyleLearningResumeInput,
};

impl QqChatAgentServiceInner {
    fn run_style_learning_task(
        &self,
        trace: &QqChatTaskTrace,
        ctx: &QqChatAgentServiceContext<'_>,
        event: &ims_bot_adapter::models::MessageEvent,
        inference_event: &ims_bot_adapter::models::MessageEvent,
        sender_id: &str,
        target_id: &str,
        bot_id: &str,
        is_group: bool,
        scope: LanguageStyleScope,
        task_handle: Arc<zihuan_core::task_context::AgentTaskHandle>,
        task_runtime: Arc<dyn zihuan_core::task_context::AgentTaskRuntime>,
    ) -> Result<()> {
        let Some(connection) = ctx.rdb_pool else {
            return Err(Error::ValidationError(
                "当前未配置关系数据库，无法执行语言风格学习。".to_string(),
            ));
        };

        let owned = OwnedStyleLearningTaskContext {
            adapter: ctx.adapter.clone(),
            bot_name: ctx.bot_name.to_string(),
            natural_language_reply_llm: Arc::clone(ctx.natural_language_reply_llm),
            intent_classification_llm: Arc::clone(ctx.intent_classification_llm),
            rdb_pool: connection.clone(),
            max_message_length: ctx.max_message_length,
            reply_batch_builder: ctx.reply_batch_builder.cloned(),
            resolved_language_style_prompt: ctx.resolved_language_style.as_ref().map(|item| item.style_prompt.clone()),
        };
        let input = StyleLearningResumeInput {
            event: event.clone(),
            inference_event: inference_event.clone(),
            sender_id: sender_id.to_string(),
            target_id: target_id.to_string(),
            bot_id: bot_id.to_string(),
            is_group,
            scope,
        };
        let trace_owned = trace.clone();
        std::thread::spawn(move || {
            execute_style_learning_task(owned, input, trace_owned, task_handle, task_runtime);
        });
        Ok(())
    }

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
        message_rate_limit_warning: Option<&str>,
        ctx: &QqChatAgentServiceContext<'_>,
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
            let mut cmd_session_state = ctx.session_state_store.lock().unwrap().clone();
            let cmd_emotion_dimensions = current_qq_chat_agent_service_config()?.resolved_emotion_dimensions();

            let user_msg_for_cmd = message_with_api_style(
                build_user_message(
                    &prepare_current_turn_user_input_from_event(hydrated_event, bot_id, ctx.bot_name, ctx.s3_ref),
                    ctx.bot_name,
                    ctx.adapter,
                    ctx.llm.supports_multimodal_input(),
                    &cmd_system_prompt,
                    ctx.resolved_language_style.as_ref().map(|item| item.style_prompt.as_str()),
                    message_rate_limit_warning,
                    &mut cmd_session_state,
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

    /// Returns the last assistant text that carries no tool calls, skipping transport
    /// errors and awaiting-user-input stops. Empty or whitespace-only text yields `None`.
    fn parse_final_reply_text(&self, stop_reason: &BrainStopReason, brain_output: &[LLMMessage]) -> Option<String> {
        if matches!(
            stop_reason,
            BrainStopReason::TransportError(_) | BrainStopReason::AwaitUserInput(_)
        ) {
            return None;
        }

        brain_output
            .iter()
            .rev()
            .find(|message| {
                matches!(message.role, zihuan_core::llm::MessageRole::Assistant) && message.tool_calls.is_empty()
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
    ) -> (&'a Arc<dyn zihuan_core::llm::llm_base::LLMBase>, &'a str) {
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
    /// Returns a [`QqChatServiceTurnResult`] containing a human-readable summary of what happened.
    pub(crate) fn handle_claimed_turn(
        &self,
        trace: &QqChatTaskTrace,
        event: &ims_bot_adapter::models::MessageEvent,
        _time: &str,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        bot_id: &str,
        message_rate_limit_warning: Option<&str>,
        ctx: &QqChatAgentServiceContext<'_>,
    ) -> Result<QqChatServiceTurnResult> {
        let prepared_input = prepare_current_turn_user_input(event, ctx.adapter, bot_id, ctx.bot_name, ctx.s3_ref);
        let mut inference_event = prepared_input.event.clone();
        inference_event.message_list = expand_messages_for_inference(&prepared_input.event.message_list);
        let inference_input =
            prepare_current_turn_user_input_from_event(&inference_event, bot_id, ctx.bot_name, ctx.s3_ref);
        let raw_user_message = prepared_input.current_text_for_prompt().to_string();
        let mut current_message = inference_input.current_text_for_prompt().to_string();
        trace.log_user_message(&raw_user_message, &current_message);

        let emotion_dimensions = current_qq_chat_agent_service_config()?.resolved_emotion_dimensions();
        let now_unix_seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
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

        let history_key = conversation_history_key(bot_id, sender_id, is_group, inference_event.group_id);
        let legacy_history_key = sender_id.to_string();
        let mut history = load_history(ctx.cache, &history_key, &legacy_history_key);

        if let Some((command_name, args)) = parse_privileged_command(&raw_user_message) {
            match command_name.as_str() {
                "auth" => {
                    let Some(connection) = ctx.rdb_pool else {
                        return Ok(QqChatServiceTurnResult {
                            result_summary: "未配置关系数据库，无法完成授权".to_string(),
                        });
                    };
                    let auth_key = args.first().map(String::as_str).unwrap_or("");
                    match handle_auth_command(connection, &self.id, sender_id, auth_key)? {
                        AuthCommandOutcome::Reply(reply) => {
                            let _ = send_direct_text_reply(
                                trace,
                                ctx.adapter,
                                target_id,
                                ctx.rdb_pool,
                                event.group_name.as_deref(),
                                ctx.bot_name,
                                bot_id,
                                &reply,
                                is_group,
                                sender_id,
                                &inference_event.sender.nickname,
                                inference_event.sender.card.as_str(),
                                ctx.max_message_length,
                                ctx.reply_batch_builder,
                            )?;
                            return Ok(QqChatServiceTurnResult {
                                result_summary: "已处理授权命令".to_string(),
                            });
                        }
                        AuthCommandOutcome::Resume { message, pending } => {
                            let _ = send_direct_text_reply(
                                trace,
                                ctx.adapter,
                                target_id,
                                ctx.rdb_pool,
                                event.group_name.as_deref(),
                                ctx.bot_name,
                                bot_id,
                                &message,
                                is_group,
                                sender_id,
                                &inference_event.sender.nickname,
                                inference_event.sender.card.as_str(),
                                ctx.max_message_length,
                                ctx.reply_batch_builder,
                            )?;
                            let resume_is_group = pending.pending_is_group;
                            let resume_target_id =
                                pending.pending_target_id.clone().unwrap_or_else(|| target_id.to_string());
                            let resume_group_id = pending.pending_group_id;
                            if matches!(pending.command, QqPrivilegedCommand::LearnGroupStyle) && !resume_is_group {
                                let reply = "当前不是群聊，无法学习群聊语言风格。".to_string();
                                let _ = send_direct_text_reply(
                                    trace,
                                    ctx.adapter,
                                    target_id,
                                    ctx.rdb_pool,
                                    event.group_name.as_deref(),
                                    ctx.bot_name,
                                    bot_id,
                                    &reply,
                                    is_group,
                                    sender_id,
                                    &inference_event.sender.nickname,
                                    inference_event.sender.card.as_str(),
                                    ctx.max_message_length,
                                    ctx.reply_batch_builder,
                                )?;
                                return Ok(QqChatServiceTurnResult {
                                    result_summary: "群聊风格学习命令在私聊中被拒绝".to_string(),
                                });
                            }
                            let scope = match pending.command {
                                QqPrivilegedCommand::LearnGlobalStyle => LanguageStyleScope::Global,
                                QqPrivilegedCommand::LearnGroupStyle => LanguageStyleScope::Group {
                                    group_id: resume_group_id
                                        .map(|value| value.to_string())
                                        .unwrap_or_else(|| resume_target_id.clone()),
                                },
                            };
                            let Some(task_runtime) = ctx.task_runtime.clone() else {
                                return Err(Error::ValidationError("task runtime is not available".to_string()));
                            };
                            let Some(task_id) = pending.pending_task_id.as_deref() else {
                                return Err(Error::ValidationError("pending task id is missing".to_string()));
                            };
                            let Some(rdb_pool) = ctx.rdb_pool.cloned() else {
                                return Err(Error::ValidationError("pending task missing rdb pool".to_string()));
                            };
                            let owned = OwnedStyleLearningTaskContext {
                                adapter: ctx.adapter.clone(),
                                bot_name: ctx.bot_name.to_string(),
                                natural_language_reply_llm: Arc::clone(ctx.natural_language_reply_llm),
                                intent_classification_llm: Arc::clone(ctx.intent_classification_llm),
                                rdb_pool,
                                max_message_length: ctx.max_message_length,
                                reply_batch_builder: ctx.reply_batch_builder.cloned(),
                                resolved_language_style_prompt: ctx
                                    .resolved_language_style
                                    .as_ref()
                                    .map(|item| item.style_prompt.clone()),
                            };
                            let input = StyleLearningResumeInput {
                                event: event.clone(),
                                inference_event: inference_event.clone(),
                                sender_id: sender_id.to_string(),
                                target_id: resume_target_id.clone(),
                                bot_id: bot_id.to_string(),
                                is_group: resume_is_group,
                                scope,
                            };
                            let trace_clone = trace.clone();
                            let task_runtime_for_runner = Arc::clone(&task_runtime);
                            let resumed = task_runtime.resume_waiting_auth_task(
                                task_id,
                                Box::new(move |task_handle| {
                                    execute_style_learning_task(
                                        owned,
                                        input,
                                        trace_clone,
                                        task_handle,
                                        task_runtime_for_runner,
                                    );
                                }),
                            );
                            if !resumed {
                                return Err(Error::ValidationError(
                                    "pending waiting-auth task could not be resumed".to_string(),
                                ));
                            }
                            return Ok(QqChatServiceTurnResult {
                                result_summary: "已恢复等待授权的任务".to_string(),
                            });
                        }
                    }
                }
                "learn_global_style" | "learn_group_style" => {
                    let Some(command_registry) = crate::command::global_command_registry() else {
                        return Err(Error::ValidationError("command registry not initialized".to_string()));
                    };
                    let permission_check = command_registry.check_permission(
                        &self.build_command_context(sender_id, target_id, is_group, inference_event.group_id),
                        &raw_user_message,
                    );
                    if !permission_check.matched || !permission_check.allowed {
                        let reply = "你没有权限使用此命令。".to_string();
                        let _ = send_direct_text_reply(
                            trace,
                            ctx.adapter,
                            target_id,
                            ctx.rdb_pool,
                            event.group_name.as_deref(),
                            ctx.bot_name,
                            bot_id,
                            &reply,
                            is_group,
                            sender_id,
                            &inference_event.sender.nickname,
                            inference_event.sender.card.as_str(),
                            ctx.max_message_length,
                            ctx.reply_batch_builder,
                        )?;
                        return Ok(QqChatServiceTurnResult {
                            result_summary: "命令权限拒绝".to_string(),
                        });
                    }

                    let Some(connection) = ctx.rdb_pool else {
                        let reply = "当前未配置关系数据库，无法执行语言风格学习。".to_string();
                        let _ = send_direct_text_reply(
                            trace,
                            ctx.adapter,
                            target_id,
                            ctx.rdb_pool,
                            event.group_name.as_deref(),
                            ctx.bot_name,
                            bot_id,
                            &reply,
                            is_group,
                            sender_id,
                            &inference_event.sender.nickname,
                            inference_event.sender.card.as_str(),
                            ctx.max_message_length,
                            ctx.reply_batch_builder,
                        )?;
                        return Ok(QqChatServiceTurnResult {
                            result_summary: "缺少关系数据库".to_string(),
                        });
                    };

                    let privileged_command = if command_name == "learn_group_style" {
                        QqPrivilegedCommand::LearnGroupStyle
                    } else {
                        QqPrivilegedCommand::LearnGlobalStyle
                    };
                    let Some(task_runtime) = ctx.task_runtime.clone() else {
                        return Err(Error::ValidationError("task runtime is not available".to_string()));
                    };
                    let task_name = if command_name == "learn_group_style" {
                        "学习群聊语言风格"
                    } else {
                        "学习全局语言风格"
                    }
                    .to_string();
                    let waiting_task = task_runtime.start_waiting_auth_task(AgentTaskRequest {
                        task_name: task_name.clone(),
                        agent_id: self.id.clone(),
                        agent_name: ctx.bot_name.to_string(),
                        user_ip: None,
                        owner_id: Some(sender_id.to_string()),
                        task_db_connection_id: ctx.task_db_connection_id.clone(),
                    });

                    let gate_outcome = enqueue_pending_privileged_command(
                        &command_registry,
                        &self.build_command_context(sender_id, target_id, is_group, inference_event.group_id),
                        connection,
                        privileged_command,
                        Some(waiting_task.task_id.as_str()),
                    )?;
                    if let PrivilegeGateOutcome::Denied(reply) = gate_outcome {
                        let _ = send_direct_text_reply(
                            trace,
                            ctx.adapter,
                            target_id,
                            ctx.rdb_pool,
                            event.group_name.as_deref(),
                            ctx.bot_name,
                            bot_id,
                            &reply,
                            is_group,
                            sender_id,
                            &inference_event.sender.nickname,
                            inference_event.sender.card.as_str(),
                            ctx.max_message_length,
                            ctx.reply_batch_builder,
                        )?;
                        return Ok(QqChatServiceTurnResult {
                            result_summary: format!("{command_name} 已进入等待授权状态"),
                        });
                    }

                    if command_name == "learn_group_style" && !is_group {
                        let reply = "当前不是群聊，无法学习群聊语言风格。".to_string();
                        let _ = send_direct_text_reply(
                            trace,
                            ctx.adapter,
                            target_id,
                            ctx.rdb_pool,
                            event.group_name.as_deref(),
                            ctx.bot_name,
                            bot_id,
                            &reply,
                            is_group,
                            sender_id,
                            &inference_event.sender.nickname,
                            inference_event.sender.card.as_str(),
                            ctx.max_message_length,
                            ctx.reply_batch_builder,
                        )?;
                        return Ok(QqChatServiceTurnResult {
                            result_summary: "群聊风格学习命令在私聊中被拒绝".to_string(),
                        });
                    }

                    let scope = if command_name == "learn_group_style" {
                        LanguageStyleScope::Group {
                            group_id: target_id.to_string(),
                        }
                    } else {
                        LanguageStyleScope::Global
                    };
                    return self
                        .run_style_learning_task(
                            trace,
                            ctx,
                            event,
                            &inference_event,
                            sender_id,
                            target_id,
                            bot_id,
                            is_group,
                            scope,
                            waiting_task,
                            task_runtime,
                        )
                        .map(|_| QqChatServiceTurnResult {
                            result_summary: format!("已创建 {task_name} 任务"),
                        });
                }
                _ => {}
            }
        }

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
                    message_rate_limit_warning,
                    ctx,
                )? {
                    current_message = passthrough;
                } else {
                    return Ok(QqChatServiceTurnResult {
                        result_summary: "已处理命令".to_string(),
                    });
                }
            }
        }

        let current_session_state = { ctx.session_state_store.lock().unwrap().clone() };
        let mut current_session_state = current_session_state;
        current_session_state.sync_emotion_dimensions(&emotion_dimensions);
        let turn_session_state = Arc::new(Mutex::new(current_session_state));

        let emotion_history_key = emotion_history_key(bot_id, sender_id, is_group, inference_event.group_id);
        run_emotion_agent(
            ctx.natural_language_reply_llm,
            ctx.cache,
            &emotion_history_key,
            &prepared_input,
            ctx.bot_name,
            Arc::clone(&turn_session_state),
            emotion_dimensions.clone(),
            ctx.compact_context_length,
        );

        let base_system_prompt = if is_group {
            build_group_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
        } else {
            build_private_system_prompt(ctx.bot_name, ctx.agent_system_prompt)
        };

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
                ),
                turn_llm.api_style(),
            )
        };

        let mut history = sanitize_messages_for_inference(history);
        let compact_result = compact_message_history(turn_llm, history.clone(), ctx.compact_context_length, &user_msg);
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
            IntentCategory::AskToolList | IntentCategory::AskSystemPrompt | IntentCategory::AskModelName
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
        let mut brain_conversation = downgrade_messages_for_model(conversation, turn_llm.supports_multimodal_input());
        let prompt_tokens_estimated = estimate_messages_tokens(&brain_conversation);
        trace.log_llm_conversation(&brain_conversation, prompt_tokens_estimated);

        let consumed_steer_messages = Arc::new(Mutex::new(Vec::new()));
        let tool_quota = ctx.tool_quota.clone();
        let mut brain = Brain::new(Arc::clone(turn_llm));
        brain.set_observer(Arc::new(QqChatBrainObserver { trace: trace.clone() }));
        brain.set_iteration_hook(Arc::new(QqChatServiceSteerHook {
            pending_steer: Arc::clone(ctx.pending_steer),
            sender_id: sender_id.to_string(),
            bot_id: bot_id.to_string(),
            bot_name: ctx.bot_name.to_string(),
            adapter: ctx.adapter.clone(),
            max_steer_count: ctx.max_steer_count,
            llm_supports_multimodal_input: turn_llm.supports_multimodal_input(),
            llm_api_style: turn_llm.api_style().map(ToOwned::to_owned),
            s3_ref: ctx.s3_ref.cloned(),
            trace: trace.clone(),
            consumed_messages: Arc::clone(&consumed_steer_messages),
            shared_runtime_values: Arc::clone(&shared_runtime_values),
            system_prompt: base_system_prompt.clone(),
            style_prompt: ctx.resolved_language_style.as_ref().map(|item| item.style_prompt.clone()),
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
                brain.add_tool(wrap_brain_tool_with_quota(
                    ListAvailableMemoryKeysBrainTool::new(memory_resources.clone()),
                    tool_quota.clone(),
                ));
            }
            if self.is_default_tool_enabled(DEFAULT_TOOL_SEARCH_MEMORY_CONTENT) {
                brain.add_tool(wrap_brain_tool_with_quota(
                    SearchMemoryContentBrainTool::new(memory_resources.clone()),
                    tool_quota.clone(),
                ));
            }
            if self.is_default_tool_enabled(DEFAULT_TOOL_REMEMBER_CONTENT) {
                brain.add_tool(wrap_brain_tool_with_quota(
                    RememberContentBrainTool::new(memory_resources),
                    tool_quota.clone(),
                ));
            }
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_WEB_SEARCH) {
            brain.add_tool(wrap_brain_tool_with_quota(
                WebSearchBrainTool::new(
                    ctx.web_search_engine.clone(),
                    ToolNotificationTarget::new(
                        Some(ctx.adapter.clone()),
                        target_id.to_string(),
                        if is_group { Some(sender_id.to_string()) } else { None },
                        is_group,
                        false,
                    ),
                ),
                tool_quota.clone(),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO) {
            brain.add_tool(wrap_brain_tool_with_quota(
                GetAgentPublicInfoBrainTool::new(current_message.clone(), build_service_model_list(ctx)),
                tool_quota.clone(),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_GET_FUNCTION_LIST) {
            brain.add_tool(wrap_brain_tool_with_quota(GetFunctionListBrainTool, tool_quota.clone()));
        }

        brain.add_tool(wrap_brain_tool_with_quota(
            RunResearchSubagentBrainTool::new(
                Arc::clone(ctx.math_programming_llm),
                Arc::clone(ctx.web_search_engine),
                ctx.rdb_pool.cloned(),
                ctx.s3_ref.cloned(),
                ctx.weaviate_image_ref.cloned(),
                Some(prepared_input.event.clone()),
                ToolNotificationTarget::dashboard(),
                if let (Some(memory_ref), Some(embedding_model)) =
                    (ctx.weaviate_memory_ref.cloned(), ctx.embedding_model.cloned())
                {
                    Some(AgentMemoryToolResources {
                        memory_ref,
                        embedding_model,
                        llm: Arc::clone(turn_llm),
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
                tool_quota.clone(),
            ),
            tool_quota.clone(),
        ));
        brain.add_tool(wrap_brain_tool_with_quota(
            ReplyMessageBrainTool::new(Arc::clone(&shared_runtime_values)),
            tool_quota.clone(),
        ));

        if self.is_default_tool_enabled(DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES) {
            brain.add_tool(wrap_brain_tool_with_quota(
                GetRecentGroupMessagesBrainTool::new(
                    ctx.rdb_pool.cloned(),
                    ToolNotificationTarget::new(
                        Some(ctx.adapter.clone()),
                        target_id.to_string(),
                        if is_group { Some(sender_id.to_string()) } else { None },
                        is_group,
                        false,
                    ),
                ),
                tool_quota.clone(),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_GET_RECENT_USER_MESSAGES) {
            brain.add_tool(wrap_brain_tool_with_quota(
                GetRecentUserMessagesBrainTool::new(
                    ctx.rdb_pool.cloned(),
                    ToolNotificationTarget::new(
                        Some(ctx.adapter.clone()),
                        target_id.to_string(),
                        if is_group { Some(sender_id.to_string()) } else { None },
                        is_group,
                        false,
                    ),
                ),
                tool_quota.clone(),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES) {
            brain.add_tool(wrap_brain_tool_with_quota(
                SearchSimilarImagesBrainTool::new(
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
                ),
                tool_quota.clone(),
            ));
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_SAVE_IMAGE) {
            if ctx.s3_ref.is_some() && ctx.weaviate_image_ref.is_some() && ctx.embedding_model.is_some() {
                brain.add_tool(wrap_brain_tool_with_quota(
                    SaveImageBrainTool::new(
                        ctx.weaviate_image_ref.cloned(),
                        ctx.embedding_model.cloned(),
                        ctx.s3_ref.cloned(),
                        ctx.rdb_pool.cloned(),
                    ),
                    tool_quota.clone(),
                ));
            }
        }

        if self.is_default_tool_enabled(DEFAULT_TOOL_IMAGE_UNDERSTAND) {
            brain.add_tool(wrap_brain_tool_with_quota(
                ImageUnderstandBrainTool::new(
                    Some(prepared_input.event.clone()),
                    ctx.rdb_pool.cloned(),
                    ctx.s3_ref.cloned(),
                    ToolNotificationTarget::new(
                        Some(ctx.adapter.clone()),
                        target_id.to_string(),
                        if is_group { Some(sender_id.to_string()) } else { None },
                        is_group,
                        false,
                    ),
                ),
                tool_quota.clone(),
            ));
        }

        brain.add_tool(wrap_brain_tool_with_quota(
            GetBotProfileBrainTool::new(ctx.adapter.clone(), prepared_input.event.clone(), ctx.s3_ref.cloned()),
            tool_quota.clone(),
        ));
        brain.add_tool(wrap_brain_tool_with_quota(
            GetQqUserProfileBrainTool::new(ctx.adapter.clone(), prepared_input.event.clone(), ctx.s3_ref.cloned()),
            tool_quota.clone(),
        ));
        brain.add_tool(wrap_brain_tool_with_quota(
            GetCurrentGroupMembersBrainTool::new(ctx.adapter.clone(), prepared_input.event.clone()),
            tool_quota.clone(),
        ));

        let qq_chat_agent_config = current_qq_chat_agent_service_config()?;
        for tool_def in &self.tool_definitions {
            brain.add_tool(wrap_brain_tool_with_quota(
                EditableQqAgentTool {
                    runner: ToolSubgraphRunner {
                        node_id: self.id.clone(),
                        owner_node_type: QQ_AGENT_TOOL_OWNER_TYPE.to_string(),
                        shared_inputs: self.shared_inputs.clone(),
                        definition: tool_def.clone(),
                        shared_runtime_values: Arc::clone(&shared_runtime_values),
                        qq_chat_agent_config: Some(qq_chat_agent_config.clone()),
                        result_mode: ToolResultMode::SingleString,
                    },
                },
                tool_quota.clone(),
            ));
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

        let mut final_reply_text = self.parse_final_reply_text(&stop_reason, &brain_output);

        if final_reply_text.is_none() && matches!(stop_reason, BrainStopReason::Done) {
            info!(
                "{LOG_PREFIX} Brain finished without sendable final reply text; requesting one more internal reflection for sender={sender_id}"
            );
            brain_conversation.extend(brain_output.iter().cloned());
            brain_conversation.push(message_with_api_style(
                LLMMessage::user(
                    "【系统补充提醒】你刚才还没有输出最终可发送文本。请重新完成本轮任务，并且最终 assistant 只能输出直接发给用户的自然语言文本，或者输出 `[no_reply]` 表示本轮不回复。"
                        .to_string(),
                ),
                turn_llm.api_style(),
            ));

            let (second_output, second_stop_reason) = brain.run(brain_conversation.clone());
            trace.record_llm_final_result(&second_stop_reason, &second_output);
            brain_output.extend(second_output.iter().cloned());
            stop_reason = second_stop_reason;
            final_reply_text = self.parse_final_reply_text(&stop_reason, &brain_output);
        }

        trace.record_llm_result_parsed(final_reply_text.as_deref());
        let suppress_send = final_reply_text
            .as_deref()
            .map(zihuan_agent::utils::string_utils::is_no_reply_directive);
        trace.record_final_reply_decision(final_reply_text.as_deref(), suppress_send, None);

        let mut visible_assistant_history_text = None;
        let mut explicit_no_reply = false;
        if final_reply_text.is_none() {
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
                BrainStopReason::AwaitUserInput(ref request) => {
                    warn!("{LOG_PREFIX} Brain paused for user input without reply: {}", request.question);
                }
            }
        } else if let Some(candidate_message) = final_reply_text.as_ref() {
            if zihuan_agent::utils::string_utils::is_no_reply_directive(candidate_message) {
                explicit_no_reply = true;
            } else {
                let available_media = collect_available_media_from_brain_output(&brain_output);
                let review_result = review_and_rewrite_reply(
                    ctx.intent_classification_llm,
                    ctx.natural_language_reply_llm,
                    ctx.natural_language_reply_system_prompt,
                    &QqReplyReviewRequest {
                        candidate_message: candidate_message.clone(),
                        is_group,
                        bot_name: ctx.bot_name.to_string(),
                        sender_id: sender_id.to_string(),
                        sender_nickname: inference_event.sender.nickname.clone(),
                        sender_card: inference_event.sender.card.clone(),
                        session_state: turn_session_state.lock().unwrap().clone(),
                        emotion_dimensions: emotion_dimensions.clone(),
                        available_media_ids: available_media.keys().cloned().collect(),
                        model_identity_context: Some(build_model_identity_context(ctx)),
                    },
                    trace,
                )?;

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
        } else if matches!(stop_reason, BrainStopReason::TransportError(_)) {
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
        event: &ims_bot_adapter::models::MessageEvent,
        inference_event: &ims_bot_adapter::models::MessageEvent,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
        bot_id: &str,
        ctx: &QqChatAgentServiceContext<'_>,
        current_message: &str,
        history_key: &str,
        mut history: Vec<LLMMessage>,
        turn_session_state: &Arc<Mutex<QqChatAgentServiceSessionState>>,
        emotion_dimensions: &[QqChatEmotionDimensionConfig],
    ) -> Result<QqChatServiceTurnResult> {
        let function_list = crate::command::build_help_text().unwrap_or_else(|| "暂无可用功能信息。".to_string());
        let model_list = build_service_model_list(ctx);
        let public_info = format_public_info_message(current_message, &model_list).to_string();

        let style_prompt = ctx.resolved_language_style.as_ref().map(|item| item.style_prompt.as_str());
        let (emotion_prompt, suppress_language_style) = {
            let session = turn_session_state.lock().unwrap();
            (
                zihuan_agent::emotion::utils::emotion_expression_prompt(&session, emotion_dimensions),
                zihuan_agent::emotion::utils::has_noticeable_emotion_expression(&session, emotion_dimensions),
            )
        };
        let style_prompt = if suppress_language_style { None } else { style_prompt };

        let meta_system_prompt = build_meta_query_system_prompt(ctx.bot_name, style_prompt, &emotion_prompt);
        let meta_user_message = build_meta_query_user_message(current_message, &function_list, &public_info);

        let meta_messages = vec![
            LLMMessage::system(meta_system_prompt),
            LLMMessage::user(meta_user_message),
        ];

        trace.mark_llm_request_started();
        let response = ctx.llm.inference(&InferenceParam {
            messages: &meta_messages,
            tools: None,
        });
        let candidate_message = response.content_text_owned().unwrap_or_default();
        let candidate_message = candidate_message.trim();
        if candidate_message.is_empty() {
            return Ok(QqChatServiceTurnResult {
                result_summary: format!("元查询[{sender_id}]：LLM未返回有效回复"),
            });
        }

        trace.record_llm_result_parsed(Some(candidate_message));

        if zihuan_agent::utils::string_utils::is_no_reply_directive(candidate_message) {
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

        let review_result = review_and_rewrite_reply(
            ctx.intent_classification_llm,
            ctx.natural_language_reply_llm,
            ctx.natural_language_reply_system_prompt,
            &QqReplyReviewRequest {
                candidate_message: candidate_message.to_string(),
                is_group,
                bot_name: ctx.bot_name.to_string(),
                sender_id: sender_id.to_string(),
                sender_nickname: inference_event.sender.nickname.clone(),
                sender_card: inference_event.sender.card.clone(),
                session_state: turn_session_state.lock().unwrap().clone(),
                emotion_dimensions: emotion_dimensions.to_vec(),
                available_media_ids: Vec::new(),
                model_identity_context: Some(build_model_identity_context(ctx)),
            },
            trace,
        )?;

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
