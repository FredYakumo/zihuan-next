use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use log::info;
use zihuan_core::agent::qq_chat::QqChatEmotionDimensionConfig;
use zihuan_core::agent::runtime_context::current_qq_chat_agent_service_config;
use zihuan_core::agent::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent::tools::{LongTaskContext, ToolCallingEngine, ToolCallingStopReason};
use zihuan_core::error::Result;
use zihuan_core::graph::tool_spec::QQ_AGENT_TOOL_OWNER_TYPE;
use zihuan_core::graph::DataValue;
use zihuan_core::ims_bot_adapter::tools::group_members::GetCurrentGroupMembersTool;
use zihuan_core::ims_bot_adapter::tools::qq_profile::{GetBotProfileTool, GetQqUserProfileTool};
use zihuan_core::memory_agent::{
    MemoryBrainAgent, MemoryBrainAgentContextTool, MemoryBrainAgentTool,
};
use zihuan_core::model_inference::inference_function::compact_message::estimate_messages_tokens;
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};
use zihuan_core::steer::message_with_api_style;
use zihuan_core::storage::AgentMemoryAccessContext;
use zihuan_core::tool_subgraph::{ToolResultMode, ToolSubgraphRunner};

use crate::qq_chat::logging::QqChatToolCallingObserver;
use crate::qq_chat::steer::QqChatServiceSteerHook;
use crate::qq_chat::tool_quota::wrap_brain_tool_with_quota;
use crate::qq_chat::user_input::PreparedCurrentTurnUserInput;
use crate::qq_chat::{
    QqChatAgentServiceContext, QqChatAgentServiceInner, QqChatTaskTrace, LOG_PREFIX,
};
use crate::tools::{
    AgentMemoryBackend, AgentMemoryToolResources, EditableQqAgentTool, GetAgentPublicInfoTool,
    GetFunctionListTool, GetRecentGroupMessagesTool, GetRecentUserMessagesTool,
    ImageUnderstandTool, ReplyMessageTool, RunResearchSubagentTool, SaveImageTool,
    SearchSimilarImagesTool, ToolNotificationTarget, WebSearchTool,
    DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO, DEFAULT_TOOL_GET_FUNCTION_LIST,
    DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES, DEFAULT_TOOL_GET_RECENT_USER_MESSAGES,
    DEFAULT_TOOL_IMAGE_UNDERSTAND, DEFAULT_TOOL_MEMORY_AGENT,
    DEFAULT_TOOL_MEMORY_AGENT_WITH_CONTEXT, DEFAULT_TOOL_SAVE_IMAGE,
    DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES, DEFAULT_TOOL_WEB_SEARCH,
};

/// Output of the QQ brain invocation for one turn.
///
/// The transport adapter consumes it for the reply decision, media collection, and history
/// writeback; the AfterBrain procedure reads `final_reply_text` as the review candidate from
/// [`ProcedureContext::procedure_outputs`].
#[derive(Clone)]
pub(crate) struct QqBrainOutput {
    /// Final sendable reply text parsed from the brain output, if any.
    pub(crate) final_reply_text: Option<String>,
    /// Whether the final reply is an explicit `[no_reply]` directive.
    pub(crate) suppress_send: bool,
    /// Full brain output messages (media collection and tracing).
    pub(crate) brain_output: Vec<LLMMessage>,
    /// Why the tool-calling loop stopped.
    pub(crate) stop_reason: ToolCallingStopReason,
}

/// The QQ turn's brain invocation as a Procedure (documents/procedure.md).
///
/// **Design:** Created per turn by the transport adapter (`handle_claimed_turn`), which keeps
/// ownership of everything that outlives the brain run (history, session state, steer
/// buffers). The procedure assembles the [`ToolCallingEngine`] — observer, steer hook,
/// quota-wrapped tools, long-task context — and [`QqBrain::run`] executes it: the multi-turn
/// tool loop, the one-shot reflection re-run when no sendable text was produced, and the final
/// reply parsing including the `[no_reply]` directive.
pub(crate) struct QqBrain {
    /// Assembled [`ToolCallingEngine`] for this turn (observer, steer hook, tools, long task).
    engine: ToolCallingEngine,
    trace: QqChatTaskTrace,
    turn_llm: Arc<dyn LLMBase>,
    brain_conversation: Vec<LLMMessage>,
    sender_id: String,
}

impl QqBrain {
    /// Assemble the turn's brain: engine, observer, steer hook, quota-wrapped tools, and the
    /// long-task context. Only the execution-time dependencies are kept on the procedure.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        service: &QqChatAgentServiceInner,
        ctx: &QqChatAgentServiceContext<'_>,
        trace: &QqChatTaskTrace,
        turn_llm: &Arc<dyn LLMBase>,
        prepared_input: &PreparedCurrentTurnUserInput,
        current_message: &str,
        sender_id: &str,
        target_id: &str,
        bot_id: &str,
        is_group: bool,
        event_group_name: Option<String>,
        brain_conversation: Vec<LLMMessage>,
        base_system_prompt: String,
        shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
        consumed_steer_messages: Arc<Mutex<Vec<LLMMessage>>>,
        turn_session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
        emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
        preprompt_context: Option<String>,
    ) -> Result<Self> {
        let tool_quota = ctx.tool_quota.clone();
        let mut brain = ToolCallingEngine::new(Arc::clone(turn_llm));
        brain.set_observer(Arc::new(QqChatToolCallingObserver { trace: trace.clone() }));
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
            style_prompt: ctx
                .resolved_language_style
                .as_ref()
                .map(|item| item.style_prompt.clone()),
            session_state: Arc::clone(&turn_session_state),
            emotion_dimensions: emotion_dimensions.clone(),
            preprompt_context: preprompt_context.clone(),
        }));

        let memory_backend =
            ctx.local_memory_store.cloned().map(AgentMemoryBackend::LocalFile).or_else(|| {
                ctx.elasticsearch_memory_ref
                    .cloned()
                    .map(AgentMemoryBackend::Elasticsearch)
                    .or_else(|| ctx.weaviate_memory_ref.cloned().map(AgentMemoryBackend::Weaviate))
            });
        if let Some(memory_backend) = &memory_backend {
            let embedding_model = ctx.embedding_model.cloned();
            if !matches!(memory_backend, AgentMemoryBackend::LocalFile(_))
                && embedding_model.is_none()
            {
                log::warn!(
                    "memory tools disabled because the configured backend has no embedding model"
                );
            } else {
                let memory_resources = AgentMemoryToolResources {
                    memory_backend: memory_backend.clone(),
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
                let memory_agent = MemoryBrainAgent::new(memory_resources);
                if service.is_default_tool_enabled(DEFAULT_TOOL_MEMORY_AGENT) {
                    brain.add_tool(wrap_brain_tool_with_quota(
                        MemoryBrainAgentTool::new(memory_agent.clone()),
                        tool_quota.clone(),
                    ));
                }
                if service.is_default_tool_enabled(DEFAULT_TOOL_MEMORY_AGENT_WITH_CONTEXT) {
                    brain.add_tool(wrap_brain_tool_with_quota(
                        MemoryBrainAgentContextTool::new(memory_agent),
                        tool_quota.clone(),
                    ));
                }
            }
        }

        if service.is_default_tool_enabled(DEFAULT_TOOL_WEB_SEARCH) {
            brain.add_tool(wrap_brain_tool_with_quota(
                WebSearchTool::new(ctx.web_search_engine.clone()),
                tool_quota.clone(),
            ));
        }

        if service.is_default_tool_enabled(DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO) {
            brain.add_tool(wrap_brain_tool_with_quota(
                GetAgentPublicInfoTool::new(current_message.to_string()),
                tool_quota.clone(),
            ));
        }

        if service.is_default_tool_enabled(DEFAULT_TOOL_GET_FUNCTION_LIST) {
            brain.add_tool(wrap_brain_tool_with_quota(GetFunctionListTool, tool_quota.clone()));
        }

        brain.add_tool(wrap_brain_tool_with_quota(
            RunResearchSubagentTool::new(
                Arc::clone(ctx.math_programming_llm),
                Arc::clone(ctx.web_search_engine),
                ctx.rdb_pool.cloned(),
                ctx.s3_ref.cloned(),
                ctx.weaviate_image_ref.cloned(),
                Some(prepared_input.event.clone()),
                ToolNotificationTarget::dashboard(),
                memory_backend.as_ref().and_then(|memory_backend| {
                    let embedding_model = ctx.embedding_model.cloned();
                    if !matches!(memory_backend, AgentMemoryBackend::LocalFile(_))
                        && embedding_model.is_none()
                    {
                        None
                    } else {
                        Some(AgentMemoryToolResources {
                            memory_backend: memory_backend.clone(),
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
                    }
                }),
                tool_quota.clone(),
            ),
            tool_quota.clone(),
        ));
        brain.add_tool(wrap_brain_tool_with_quota(
            ReplyMessageTool::new(Arc::clone(&shared_runtime_values)),
            tool_quota.clone(),
        ));

        if service.is_default_tool_enabled(DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES) {
            brain.add_tool(wrap_brain_tool_with_quota(
                GetRecentGroupMessagesTool::new(
                    ctx.rdb_pool.cloned(),
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
                ),
                tool_quota.clone(),
            ));
        }

        if service.is_default_tool_enabled(DEFAULT_TOOL_GET_RECENT_USER_MESSAGES) {
            brain.add_tool(wrap_brain_tool_with_quota(
                GetRecentUserMessagesTool::new(
                    ctx.rdb_pool.cloned(),
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
                ),
                tool_quota.clone(),
            ));
        }

        if service.is_default_tool_enabled(DEFAULT_TOOL_SEARCH_SIMILAR_IMAGES) {
            brain.add_tool(wrap_brain_tool_with_quota(
                SearchSimilarImagesTool::new(
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
                ),
                tool_quota.clone(),
            ));
        }

        if service.is_default_tool_enabled(DEFAULT_TOOL_SAVE_IMAGE)
            && ctx.s3_ref.is_some()
            && ctx.weaviate_image_ref.is_some()
            && ctx.embedding_model.is_some()
        {
            brain.add_tool(wrap_brain_tool_with_quota(
                SaveImageTool::new(
                    ctx.weaviate_image_ref.cloned(),
                    None,
                    ctx.embedding_model.cloned(),
                    ctx.s3_ref.cloned(),
                    ctx.rdb_pool.cloned(),
                ),
                tool_quota.clone(),
            ));
        }

        if service.is_default_tool_enabled(DEFAULT_TOOL_IMAGE_UNDERSTAND) {
            brain.add_tool(wrap_brain_tool_with_quota(
                ImageUnderstandTool::new(
                    Some(prepared_input.event.clone()),
                    ctx.rdb_pool.cloned(),
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
                ),
                tool_quota.clone(),
            ));
        }

        brain.add_tool(wrap_brain_tool_with_quota(
            GetBotProfileTool::new(
                ctx.adapter.clone(),
                prepared_input.event.clone(),
                ctx.s3_ref.cloned(),
            ),
            tool_quota.clone(),
        ));
        brain.add_tool(wrap_brain_tool_with_quota(
            GetQqUserProfileTool::new(
                ctx.adapter.clone(),
                prepared_input.event.clone(),
                ctx.s3_ref.cloned(),
            ),
            tool_quota.clone(),
        ));
        brain.add_tool(wrap_brain_tool_with_quota(
            GetCurrentGroupMembersTool::new(ctx.adapter.clone(), prepared_input.event.clone()),
            tool_quota.clone(),
        ));

        let qq_chat_agent = current_qq_chat_agent_service_config()?;
        for tool_def in &service.tool_definitions {
            brain.add_tool(wrap_brain_tool_with_quota(
                EditableQqAgentTool {
                    runner: ToolSubgraphRunner {
                        node_id: service.id.clone(),
                        owner_node_type: QQ_AGENT_TOOL_OWNER_TYPE.to_string(),
                        shared_inputs: service.shared_inputs.clone(),
                        definition: tool_def.clone(),
                        shared_runtime_values: Arc::clone(&shared_runtime_values),
                        qq_chat_agent: Some(qq_chat_agent.clone()),
                        result_mode: ToolResultMode::SingleString,
                        builtin_executor: Some(
                            crate::qq_tool_subgraph_hooks::image_understand_executor(),
                        ),
                        progress_notifier: Some(
                            crate::qq_tool_subgraph_hooks::qq_progress_notifier(),
                        ),
                    },
                },
                tool_quota.clone(),
            ));
        }

        if let Some(task_runtime) = ctx.task_runtime.clone() {
            brain.set_long_task_context(LongTaskContext {
                task_runtime,
                owner_id: Some(sender_id.to_string()),
                agent_id: service.id.clone(),
                agent_name: ctx.bot_name.to_string(),
                task_db_connection_id: ctx.task_db_connection_id.clone(),
                notifier: Arc::new(crate::qq_chat::model::notifier::QqLongTaskNotifier {
                    adapter: ctx.adapter.clone(),
                    target_id: target_id.to_string(),
                    sender_id: sender_id.to_string(),
                    is_group,
                    rdb_pool: ctx.rdb_pool.cloned(),
                    group_name: event_group_name,
                    bot_id: bot_id.to_string(),
                    bot_name: ctx.bot_name.to_string(),
                }),
            });
        }

        Ok(Self {
            engine: brain,
            trace: trace.clone(),
            turn_llm: Arc::clone(turn_llm),
            brain_conversation,
            sender_id: sender_id.to_string(),
        })
    }
}

/// Returns the last assistant text that carries no tool calls, skipping transport errors and
/// awaiting-user-input stops. Empty or whitespace-only text yields `None`.
fn parse_final_reply_text(
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

#[async_trait]
impl Procedure for QqBrain {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "qq_brain",
            name: "QQ Brain",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, _context: &ProcedureContext) -> Result<ProcedureOutput> {
        self.trace.mark_llm_request_started();
        let mut brain_conversation = self.brain_conversation.clone();
        let (mut brain_output, mut stop_reason) = self.engine.run(brain_conversation.clone());
        self.trace.record_llm_final_result(&stop_reason, &brain_output);
        let completion_tokens_estimated = estimate_messages_tokens(&brain_output);

        // take the usage of the last LLM call only (the final assistant message that carries
        // usage). Each iteration of the tool loop sends the full context accumulated up to that
        // point, so summing every message would count the same history input multiple times and
        // inflate total_tokens. The final request's usage is the complete context actually sent
        // for this reply.
        let exact_token_usage = brain_output
            .iter()
            .rev()
            .find(|message| {
                matches!(message.role, zihuan_core::model_inference::llm::MessageRole::Assistant)
                    && message.usage.is_some()
            })
            .and_then(|message| message.usage.clone());
        self.trace.record_token_usage(completion_tokens_estimated, exact_token_usage);

        let mut final_reply_text = parse_final_reply_text(&stop_reason, &brain_output);

        if final_reply_text.is_none() && matches!(stop_reason, ToolCallingStopReason::Done) {
            info!(
                "{LOG_PREFIX} ToolCallingEngine finished without sendable final reply text; requesting one more internal reflection for sender={}",
                self.sender_id
            );
            brain_conversation.extend(brain_output.iter().cloned());
            brain_conversation.push(message_with_api_style(
                LLMMessage::user(
                    "【系统补充提醒】你刚才还没有输出最终可发送文本。请重新完成本轮任务，并且最终 assistant 只能输出直接发给用户的自然语言文本，或者输出 `[no_reply]` 表示本轮不回复。"
                        .to_string(),
                ),
                self.turn_llm.api_style(),
            ));

            let (second_output, second_stop_reason) = self.engine.run(brain_conversation.clone());
            self.trace.record_llm_final_result(&second_stop_reason, &second_output);
            brain_output.extend(second_output.iter().cloned());
            stop_reason = second_stop_reason;
            final_reply_text = parse_final_reply_text(&stop_reason, &brain_output);
        }

        self.trace.record_llm_result_parsed(final_reply_text.as_deref());
        let suppress_send = final_reply_text
            .as_deref()
            .map(zihuan_core::agent::utils::string_utils::is_no_reply_directive);
        self.trace
            .record_final_reply_decision(final_reply_text.as_deref(), suppress_send, None);

        Ok(ProcedureOutput::of(QqBrainOutput {
            final_reply_text,
            suppress_send: suppress_send.unwrap_or(false),
            brain_output,
            stop_reason,
        }))
    }
}
