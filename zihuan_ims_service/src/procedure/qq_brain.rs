use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::qq_session_state::QqChatSessionState;
use crate::role_config::QqChatEmotionDimensionConfig;
use async_trait::async_trait;
use log::{info, warn};
use zihuan_core::agent::declarative_agent::list_agent_ids;
use zihuan_core::agent::runtime_context::current_agent_resources;
use zihuan_core::agent::tools::{LongTaskContext, Tool, ToolCallingEngine, ToolCallingStopReason};
use zihuan_core::agent::{
    AgentServiceContext, MemoryCapability, LLM_KIND_MAIN, LLM_KIND_MATH_PROGRAMMING,
};
use zihuan_core::error::{Error, Result};
use zihuan_core::graph::tool_spec::QQ_AGENT_TOOL_OWNER_TYPE;
use zihuan_core::graph::DataValue;
use zihuan_core::ims_bot_adapter::tools::group_members::GetCurrentGroupMembersTool;
use zihuan_core::ims_bot_adapter::tools::qq_profile::{GetBotProfileTool, GetQqUserProfileTool};
use zihuan_core::model_inference::inference_function::compact_message::estimate_messages_tokens;
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::{LLMMessage, MessagePart};
use zihuan_core::model_inference::message_content_utils::{
    downgrade_messages_for_model, media_placeholder,
};
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};
use zihuan_core::steer::{message_with_api_style, PendingSteerEvent};
use zihuan_core::storage::LLMMessageSessionCacheRef;
use zihuan_core::tool_subgraph::{ToolResultMode, ToolSubgraphRunner};

use super::before_brain::BeforeBrainOutput;
use super::GroupMessageWindowOutput;
use crate::qq_chat::logging::{QqChatToolCallingObserver, QqChatTurnIdentity};
use crate::qq_chat::steer::{QqChatServiceSteerHook, SteerTurnContext};
use crate::qq_chat::tool_quota::wrap_brain_tool_with_quota;
use crate::qq_chat::user_input::PreparedCurrentTurnUserInput;
use crate::qq_chat::{
    compact_turn_history, split_current_user_body, QqChatAgentServiceContext,
    QqChatAgentServiceInner, QqChatTaskTrace, LOG_PREFIX,
};
use crate::tools::{
    qq_memory_access_context, EditableQqAgentTool, GetAgentPublicInfoTool, GetFunctionListTool,
    GetRecentGroupMessagesTool, GetRecentUserMessagesTool, ImageUnderstandTool, ReplyMessageTool,
    SaveImageTool, SearchQqMessagesTool, SearchSimilarImagesTool, SharedTool,
    ToolNotificationTarget, WebSearchTool, DEFAULT_TOOL_GET_AGENT_PUBLIC_INFO,
    DEFAULT_TOOL_GET_FUNCTION_LIST, DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES,
    DEFAULT_TOOL_GET_RECENT_USER_MESSAGES, DEFAULT_TOOL_IMAGE_UNDERSTAND,
    DEFAULT_TOOL_MEMORY_AGENT, DEFAULT_TOOL_SAVE_IMAGE, DEFAULT_TOOL_SEARCH_QQ_MESSAGES,
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
    /// The assembled current-turn user message, for the turn's history writeback.
    pub(crate) current_user_message: LLMMessage,
    /// Full brain output messages (media collection and tracing).
    pub(crate) brain_output: Vec<LLMMessage>,
    /// Why the tool-calling loop stopped.
    pub(crate) stop_reason: ToolCallingStopReason,
}

/// The QQ turn's brain invocation as a Procedure (documents/procedure.md).
///
/// **Design:** Created per turn by the transport adapter (`handle_claimed_turn`), which keeps
/// ownership of everything that outlives the brain run (steer buffers, session state). The
/// procedure assembles the [`ToolCallingEngine`] — observer, steer hook, quota-wrapped tools,
/// long-task context. [`QqBrain::run`] reads the turn input the before-brain procedure already
/// assembled (system prompt, user message, memory-title hint), compacts history, and then
/// executes the engine: the multi-turn tool loop, the one-shot reflection re-run when no
/// sendable text was produced, and the final reply parsing including the `[no_reply]`
/// directive.
pub(crate) struct QqBrain {
    /// Assembled [`ToolCallingEngine`] for this turn (observer, steer hook, tools, long task).
    engine: ToolCallingEngine,
    trace: QqChatTaskTrace,
    turn_llm: Arc<dyn LLMBase>,
    /// Turn identity for the Data Explorer context stages emitted while sending the input.
    agent_id: String,
    sender_id: String,
    turn_id: String,
    /// Loaded conversation history. The current-turn input was already assembled by the
    /// before-brain procedure and arrives through the shared procedure context.
    history: Vec<LLMMessage>,
    history_key: String,
    cache: Arc<LLMMessageSessionCacheRef>,
    /// Shared with the steer hook: filled at the start of `run` with the assembled turn
    /// context so steer messages built mid-loop reuse the same system prompt and memory hint.
    turn_context: Arc<Mutex<Option<SteerTurnContext>>>,
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
        turn_id: &str,
        event_group_name: Option<String>,
        history: Vec<LLMMessage>,
        history_key: &str,
        shared_runtime_values: Arc<Mutex<HashMap<String, DataValue>>>,
        consumed_steer_messages: Arc<Mutex<Vec<LLMMessage>>>,
        in_flight_steers: Arc<Mutex<Vec<PendingSteerEvent>>>,
        turn_session_state: Arc<Mutex<QqChatSessionState>>,
        emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    ) -> Result<Self> {
        let tool_quota = ctx.tool_quota.clone();
        let mut brain = ToolCallingEngine::new(Arc::clone(turn_llm));
        brain.set_observer(Arc::new(QqChatToolCallingObserver {
            trace: trace.clone(),
            identity: Some(QqChatTurnIdentity {
                agent_id: ctx.agent_id.to_string(),
                sender_id: sender_id.to_string(),
                turn_id: turn_id.to_string(),
            }),
        }));
        let turn_context = Arc::new(Mutex::new(None));
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
            in_flight_steers: Arc::clone(&in_flight_steers),
            shared_runtime_values: Arc::clone(&shared_runtime_values),
            style_prompt: ctx
                .resolved_language_style
                .as_ref()
                .map(|item| item.style_prompt.clone()),
            session_state: Arc::clone(&turn_session_state),
            emotion_dimensions: emotion_dimensions.clone(),
            turn_context: Arc::clone(&turn_context),
        }));

        let memory_access =
            qq_memory_access_context(sender_id, target_id, is_group, prepared_input.event.group_id);
        let service_context = AgentServiceContext::new()
            .with_memory(MemoryCapability::resolve(
                ctx.local_memory_store.cloned(),
                ctx.retrieval_store.cloned(),
                ctx.embedding_model.cloned(),
            ))
            .with_llm(LLM_KIND_MAIN, Arc::clone(ctx.llm))
            .with_llm(LLM_KIND_MATH_PROGRAMMING, Arc::clone(ctx.math_programming_llm))
            .with_tool(
                DEFAULT_TOOL_WEB_SEARCH,
                Arc::new(wrap_brain_tool_with_quota(
                    WebSearchTool::new(ctx.web_search_engine.clone()),
                    tool_quota.clone(),
                )),
            )
            .with_tool(
                DEFAULT_TOOL_IMAGE_UNDERSTAND,
                Arc::new(ImageUnderstandTool::new(
                    Some(prepared_input.event.clone()),
                    ctx.rdb_pool.cloned(),
                    ctx.s3_ref.cloned(),
                    ToolNotificationTarget::dashboard(),
                )),
            )
            .with_tool(
                DEFAULT_TOOL_SEARCH_QQ_MESSAGES,
                Arc::new(SearchQqMessagesTool::new(
                    ctx.rdb_pool.cloned(),
                    ToolNotificationTarget::new(None, target_id.to_string(), None, is_group, false),
                )),
            );

        // Definition-driven agent host for this turn: memory tools plus the web/image tools the research
        // agent escalates to, and the LLM handles their `llm_kind` resolves to. `main` is the
        // service's main model; the memory agent uses it, the research agent (`math_programming`)
        // uses the dedicated math/programming model.
        let memory_enabled = service_context.memory().is_some();
        let mut agent_host = service_context.build_host(memory_access);

        // The memory agent is composed and registered by `build_host`; grab it first so the
        // research agent can reference it.
        let mut added_agent_ids = HashSet::new();
        if let Some(tool) = agent_host.tool(DEFAULT_TOOL_MEMORY_AGENT) {
            if memory_enabled && service.is_default_tool_enabled(DEFAULT_TOOL_MEMORY_AGENT) {
                brain.add_tool(wrap_brain_tool_with_quota(
                    SharedTool::new(tool),
                    tool_quota.clone(),
                ));
                added_agent_ids.insert(DEFAULT_TOOL_MEMORY_AGENT.to_string());
            }
        }

        if let Some(tool) = agent_host.publish_logged("run_research_subagent") {
            brain.add_tool(wrap_brain_tool_with_quota(SharedTool::new(tool), tool_quota.clone()));
            added_agent_ids.insert("run_research_subagent".to_string());
        }
        // Sub-agents the service selects. Publish every remaining on-disk definition first so a
        // selected sub-agent may reference another one, then expose the selected ids that the
        // built-in tools above have not already added.
        if !service.sub_agent_ids.is_empty() {
            for id in list_agent_ids() {
                if agent_host.tool(&id).is_none() {
                    agent_host.publish_logged(&id);
                }
            }
            for id in &service.sub_agent_ids {
                if added_agent_ids.contains(id) {
                    continue;
                }
                let Some(tool) = agent_host.tool(id) else {
                    warn!("{LOG_PREFIX} configured sub-agent '{id}' is unavailable");
                    continue;
                };
                brain.add_tool(wrap_brain_tool_with_quota(
                    SharedTool::new(tool),
                    tool_quota.clone(),
                ));
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
                    ctx.retrieval_store.cloned(),
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
            && ctx.retrieval_store.is_some()
            && ctx.embedding_model.is_some()
        {
            brain.add_tool(wrap_brain_tool_with_quota(
                SaveImageTool::new(
                    ctx.retrieval_store.cloned(),
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


        for tool in zihuan_core::agent::tools::selected_plugin_brain_tools(&service.plugin_tool_names)
        {
            brain.add_tool(wrap_brain_tool_with_quota(tool, tool_quota.clone()));
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

        let resources = current_agent_resources()?;
        for tool_def in &service.tool_definitions {
            brain.add_tool(wrap_brain_tool_with_quota(
                EditableQqAgentTool {
                    runner: ToolSubgraphRunner {
                        node_id: service.id.clone(),
                        owner_node_type: QQ_AGENT_TOOL_OWNER_TYPE.to_string(),
                        shared_inputs: service.shared_inputs.clone(),
                        definition: tool_def.clone(),
                        shared_runtime_values: Arc::clone(&shared_runtime_values),
                        resources: Some(resources.clone()),
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
            agent_id: ctx.agent_id.to_string(),
            sender_id: sender_id.to_string(),
            turn_id: turn_id.to_string(),
            history,
            history_key: history_key.to_string(),
            cache: Arc::clone(ctx.cache),
            turn_context,
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

/// Full text content of one message as the model receives it: text parts joined, and for
/// multimodal messages one marker line per media part in its position.
fn raw_message_content(message: &LLMMessage) -> String {
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            MessagePart::Text { text } => Some(text.clone()),
            MessagePart::Image { media } => Some(media_placeholder("[image]", &media.media_id)),
            MessagePart::Video { media } => Some(media_placeholder("[video]", &media.media_id)),
        })
        .collect::<Vec<_>>()
        .join("\n")
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

    async fn run(&self, context: &ProcedureContext) -> Result<ProcedureOutput> {
        // The before-brain procedure assembled the turn's system prompt and user input; steer
        // messages built mid-loop read the same context through the shared cell.
        let assembled = context.find_output::<BeforeBrainOutput>().ok_or_else(|| {
            Error::ValidationError("before-brain procedure produced no turn input".to_string())
        })?;
        *self.turn_context.lock().unwrap() = Some(SteerTurnContext {
            system_prompt: assembled.system_prompt.clone(),
            memory_context: assembled.memory_context.clone(),
        });
        let user_msg = assembled.user_message.clone();

        // The per-sender conversation history is still compacted for the session cache, but
        // on a group turn the group-message-window procedure replaces what the brain
        // receives as history with the group messages related to this turn's input; the
        // window's reply directive rides on the assembled user message (before-brain), not
        // on this history block.
        let window_output = context.find_output::<GroupMessageWindowOutput>();
        let compact_result = compact_turn_history(
            &self.cache,
            &self.history_key,
            &self.turn_llm,
            self.history.clone(),
            &user_msg,
        );
        let history: Vec<LLMMessage> = match &window_output {
            Some(window) => window.history_message().into_iter().collect(),
            None => compact_result.messages,
        };
        self.trace
            .record_history_stats(history.len(), estimate_messages_tokens(&history));

        let mut conversation: Vec<LLMMessage> = Vec::with_capacity(history.len() + 1);
        conversation.extend(history.iter().cloned());
        conversation.push(user_msg.clone());
        let brain_conversation =
            downgrade_messages_for_model(conversation, self.turn_llm.supports_multimodal_input());
        let prompt_tokens_estimated = estimate_messages_tokens(&brain_conversation);
        self.trace.log_llm_conversation(&brain_conversation, prompt_tokens_estimated);
        // What the brain actually receives: the assembled current-turn input plus the history it
        // is sent with. The assembled system prompt and memory titles are reported by the
        // before-brain stages.
        let current_user_body =
            user_msg.content_text_owned().as_deref().and_then(split_current_user_body);
        // History user messages carry their per-turn system prefix; show only the turn's own
        // body so the list reads as the conversation instead of repeating the same prefix.
        let history_input: Vec<serde_json::Value> = brain_conversation
            .iter()
            .take(brain_conversation.len().saturating_sub(1))
            .map(|message| {
                let full = message.content_text_owned().unwrap_or_default();
                let is_user =
                    matches!(message.role, zihuan_core::model_inference::llm::MessageRole::User);
                let text = if is_user {
                    split_current_user_body(&full).unwrap_or(full.clone())
                } else {
                    full.clone()
                };
                serde_json::json!({
                    "role": format!("{:?}", message.role).to_lowercase(),
                    "chars": text.chars().count(),
                    "text": text,
                })
            })
            .collect();
        // What went over the wire: every sent message with its full content, per-turn prefixes
        // intact — the raw view behind the readable history list.
        let raw_messages: Vec<serde_json::Value> = brain_conversation
            .iter()
            .map(|message| {
                let mut entry = serde_json::Map::new();
                entry.insert(
                    "role".to_string(),
                    serde_json::json!(format!("{:?}", message.role).to_lowercase()),
                );
                entry
                    .insert("content".to_string(), serde_json::json!(raw_message_content(message)));
                if !message.tool_calls.is_empty() {
                    entry.insert("tool_calls".to_string(), serde_json::json!(message.tool_calls));
                }
                if let Some(tool_call_id) = &message.tool_call_id {
                    entry.insert("tool_call_id".to_string(), serde_json::json!(tool_call_id));
                }
                serde_json::Value::Object(entry)
            })
            .collect();
        zihuan_core::qq_context::emit_qq_context_stage(
            &self.agent_id,
            &self.sender_id,
            &self.turn_id,
            "Brain 输入",
            serde_json::json!({
                "model": self.turn_llm.get_model_name(),
                "messages_sent": brain_conversation.len(),
                "prompt_tokens_estimated": prompt_tokens_estimated,
                "tools": self.engine.tool_definitions(),
                "current_user_message": current_user_body,
                "history_source": window_output
                    .as_ref()
                    .map(|_| "group-message-window")
                    .unwrap_or("conversation_history"),
                "history": history_input,
                "raw_messages": raw_messages,
                "compacted": compact_result.did_compact,
                "tokens_before_compaction": compact_result
                    .did_compact
                    .then_some(compact_result.estimated_tokens_before),
                "tokens_after_compaction": compact_result
                    .did_compact
                    .then_some(compact_result.estimated_tokens_after),
                "removed_tool_related_messages": compact_result.removed_tool_related_messages,
            }),
        );

        self.trace.mark_llm_request_started();
        let mut brain_conversation = brain_conversation;
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
            current_user_message: user_msg,
            brain_output,
            stop_reason,
        }))
    }
}
