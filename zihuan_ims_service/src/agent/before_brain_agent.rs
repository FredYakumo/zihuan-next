use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use log::{info, warn};
use zihuan_core::agent::qq_chat::QqChatEmotionDimensionConfig;
use zihuan_core::agent::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent::tools::{ToolCallingEngine, ToolCallingStopReason};
use zihuan_core::agent::{Agent, AgentContext, AgentDescriptor};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::graph::data_value::LLMMessageSessionCacheRef;
use zihuan_core::memory_agent::{MemoryBrainAgent, MemoryBrainAgentContextTool};
use zihuan_core::model_inference::inference_function::compact_message::{
    compact_message_history, compaction_threshold,
};
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::{LLMMessage, MessageRole};
use zihuan_core::runtime::block_async;
use zihuan_core::steer::message_with_api_style;
use zihuan_core::system_config::current_context_compaction_percent;

use crate::agent::emotion::utils::emotion_dimensions_text;
use crate::qq_chat::logging::{QqChatTaskTrace, QqChatToolCallingObserver};
use crate::qq_chat::PreparedCurrentTurnUserInput;
use crate::storage::qq_chat_history_store::{load_history, save_history};
use crate::tools::{
    AgentMemoryToolResources, GetRecentGroupMessagesTool, GetRecentUserMessagesTool,
    ToolNotificationTarget, UpdateAgentStateTool, DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES,
    DEFAULT_TOOL_GET_RECENT_USER_MESSAGES, DEFAULT_TOOL_MEMORY_AGENT_WITH_CONTEXT,
};

const LOG_PREFIX: &str = "[QqBeforeBrainAgent]";

// Prompt engineering

fn build_chat_preprompt_agent_system_prompt(
    bot_name: &str,
    emotion_dimensions_text: &str,
) -> String {
    format!(
        "You are the chat-preprompt agent for the QQ bot `{bot_name}`. You run before the main reply agent every turn and prepare a context block that anchors its reply. You have two responsibilities.\n\
         \n[Responsibility 1: Emotion management]\n\
         Current emotion state:\n{emotion_dimensions_text}\n\
         Based on the current event and the independent emotion history, decide whether the emotion should be adjusted. Call `update_agent_state` only when a change is truly warranted; do not call any tool when no change is needed. When an adjustment is needed, specify an emotion dimension and `increase` or `decrease`. You may adjust multiple dimensions in the same event if each is genuinely necessary.\n\
         \n[Responsibility 2: Recall & consistency preprompt]\n\
         - Extract the key nouns / entities / proper nouns from the user's current message.\n\
         - For each, call `memory_agent_with_context` with the complete current chat context and `search_memory` to check whether you have related memory or an existing stance.\n\
         - When memory contains your prior stance on a topic, surface it so the main agent stays consistent and does not flip its likes/dislikes or opinions across turns.\n\
         - If a [Candidate Dream Memory] block is present, judge whether it is relevant to the current event. Only include relevant durable facts or continuity in the final context; omit unrelated Dream content completely.\n\
         - For nouns that have no related memory and that you do not already know, include in the final context block a line exactly like: 「xxx」这些名词没有相关内容，可能需要联网查询？\n\
         - When the user's question references something you said before, or tests consistency of your preferences, call `get_recent_user_messages` with your own id to recall your own previous replies.\n\
         \n[Output contract]\n\
         Your FINAL assistant message MUST be a concise context block, and nothing else. Use sections `[Recalled Memory]`, `[Dream Memory]`, `[Missing Knowledge]`, `[Recent Self Statements]`, and `[Emotion Note]` when applicable. If nothing relevant was found and no emotion changed, output a single line: [Preprompt] no recall needed. This block is injected into the main reply prompt; it is NOT a reply to the user. Never claim to have sent a message."
    )
}

// =======================

fn build_chat_preprompt_agent_user_message(
    input: &PreparedCurrentTurnUserInput,
    bot_name: &str,
    bot_id: &str,
    sender_id: &str,
    dream_memory: Option<&str>,
) -> String {
    let sender_name = zihuan_core::ims_bot_adapter::utils::sender_display_name!(
        &input.event.sender.nickname,
        &input.event.sender.card
    );
    let dream_candidate = dream_memory
        .filter(|content| !content.trim().is_empty())
        .map(|content| format!("\n\n[Candidate Dream Memory]\n{content}"))
        .unwrap_or_default();
    format!(
        "[Current QQ Event]\n`{sender_name}` sent a message to you (`{bot_name}`):\n{}\n\nYour own id is `{bot_id}`; the current sender's id is `{sender_id}`. Evaluate whether this event should change your emotion state, and prepare the preprompt context block per the output contract.{dream_candidate}",
        input.current_text_for_prompt(),
    )
}

pub(crate) struct PrepromptContext<'a> {
    pub(crate) trace: &'a QqChatTaskTrace,
    pub(crate) llm: &'a Arc<dyn LLMBase>,
    pub(crate) cache: &'a Arc<LLMMessageSessionCacheRef>,
    pub(crate) history_key: &'a str,
    pub(crate) input: &'a PreparedCurrentTurnUserInput,
    pub(crate) bot_name: &'a str,
    pub(crate) bot_id: &'a str,
    pub(crate) agent_id: &'a str,
    pub(crate) sender_id: &'a str,
    pub(crate) target_id: &'a str,
    pub(crate) is_group: bool,
    pub(crate) session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
    pub(crate) emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    pub(crate) memory_resources: Option<AgentMemoryToolResources>,
    pub(crate) rdb_pool: Option<RelationalDbConnection>,
    pub(crate) default_tools_enabled: &'a HashMap<String, bool>,
}

/// QQ turn sub-agent that prepares continuity and emotion context for the reply agent.
pub(crate) struct BeforeBrainAgent<'a> {
    context: PrepromptContext<'a>,
}

impl<'a> BeforeBrainAgent<'a> {
    pub(crate) fn new(context: PrepromptContext<'a>) -> Self {
        Self { context }
    }

    pub(crate) fn execute(&self) -> Option<String> {
        run_preprompt(&self.context)
    }
}

#[async_trait::async_trait]
impl Agent for BeforeBrainAgent<'_> {
    type Input = ();
    type Output = Option<String>;

    fn descriptor(&self) -> AgentDescriptor {
        AgentDescriptor::new(
            "qq_preprompt",
            "QQ Preprompt Agent",
            vec!["emotion", "memory_recall", "conversation_continuity"],
        )
    }

    async fn run(
        &self,
        _context: AgentContext,
        _input: Self::Input,
    ) -> zihuan_core::error::Result<Self::Output> {
        Ok(self.execute())
    }
}

fn run_preprompt(ctx: &PrepromptContext<'_>) -> Option<String> {
    let emotion_dimensions_text =
        emotion_dimensions_text(&ctx.session_state.lock().unwrap(), &ctx.emotion_dimensions);

    // Load the latest candidate dream memory for the current sender, if available.
    let dream_memory = ctx.rdb_pool.as_ref().and_then(|connection| {
        match block_async(zihuan_core::scheduled_task::latest_dream_memory(
            connection,
            ctx.agent_id,
            ctx.sender_id,
        )) {
            Ok(memory) => memory,
            Err(err) => {
                warn!(
                    "{LOG_PREFIX} failed to load Dream memory for sender={}: {err}",
                    ctx.sender_id
                );
                None
            }
        }
    });

    // Build the current turn and compact persisted history to fit the context budget.
    let user_message = message_with_api_style(
        LLMMessage::user(build_chat_preprompt_agent_user_message(
            ctx.input,
            ctx.bot_name,
            ctx.bot_id,
            ctx.sender_id,
            dream_memory.as_deref(),
        )),
        ctx.llm.api_style(),
    );
    let compact_result = compact_message_history(
        ctx.llm,
        load_history(ctx.cache, ctx.history_key),
        compaction_threshold(ctx.llm.context_length(), current_context_compaction_percent()),
        &user_message,
    );
    let mut history = compact_result.messages;
    if compact_result.did_compact {
        info!("{LOG_PREFIX} history compacted for {}", ctx.history_key);
    }

    // assemble the 【system prompt, prior turns, and the current user event.]
    let mut conversation = Vec::with_capacity(history.len() + 2);
    conversation.push(message_with_api_style(
        LLMMessage::system(build_chat_preprompt_agent_system_prompt(
            ctx.bot_name,
            &emotion_dimensions_text,
        )),
        ctx.llm.api_style(),
    ));
    conversation.extend(history.iter().cloned());
    conversation.push(user_message.clone());

    let mut brain = ToolCallingEngine::new(Arc::clone(ctx.llm));
    brain.set_observer(Arc::new(QqChatToolCallingObserver { trace: ctx.trace.clone() }));

    ctx.trace
        .record_graph_phase("情绪维度处理", serde_json::json!({"status": "preprompt"}));
    brain.add_tool(UpdateAgentStateTool::new(
        Arc::clone(&ctx.session_state),
        ctx.emotion_dimensions.clone(),
        Arc::clone(ctx.llm),
        ctx.input.current_text_for_prompt().to_string(),
    ));

    let is_enabled = |name: &str| *ctx.default_tools_enabled.get(name).unwrap_or(&true);
    if let Some(resources) = ctx
        .memory_resources
        .clone()
        .filter(|_| is_enabled(DEFAULT_TOOL_MEMORY_AGENT_WITH_CONTEXT))
    {
        ctx.trace
            .record_graph_phase("名词处理", serde_json::json!({"status": "preprompt"}));
        brain.add_tool(MemoryBrainAgentContextTool::new(MemoryBrainAgent::new(resources)));
    }

    let notification_target =
        ToolNotificationTarget::new(None, ctx.target_id.to_string(), None, ctx.is_group, false);
    let recent_message_tools_enabled = ctx.rdb_pool.is_some()
        && (is_enabled(DEFAULT_TOOL_GET_RECENT_USER_MESSAGES)
            || (ctx.is_group && is_enabled(DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES)));
    if recent_message_tools_enabled {
        ctx.trace.record_graph_phase(
            "获取最近消息",
            serde_json::json!({"status": "preprompt tool when needed"}),
        );
    }

    if ctx.rdb_pool.is_some() && is_enabled(DEFAULT_TOOL_GET_RECENT_USER_MESSAGES) {
        brain.add_tool(GetRecentUserMessagesTool::new(
            ctx.rdb_pool.clone(),
            notification_target.clone(),
        ));
    }
    if ctx.is_group && ctx.rdb_pool.is_some() && is_enabled(DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES)
    {
        brain.add_tool(GetRecentGroupMessagesTool::new(ctx.rdb_pool.clone(), notification_target));
    }

    // Run the preprompt agent and extract its final assistant context block.
    let (output, stop_reason) = brain.run(conversation);
    let context_block = match stop_reason {
        ToolCallingStopReason::Done => output
            .iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
            .and_then(LLMMessage::content_text_owned)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
        _ => {
            warn!("{LOG_PREFIX} inference ended without normal completion: {stop_reason:?}");
            None
        }
    };

    history.push(user_message);
    history.extend(output);
    save_history(ctx.cache, ctx.history_key, history);
    context_block
}
