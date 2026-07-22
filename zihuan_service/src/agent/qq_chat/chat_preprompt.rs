use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use log::{info, warn};

use model_inference::inference_function::compact_message::compact_message_history;

use zihuan_agent::brain::{Brain, BrainStopReason};
use zihuan_agent::emotion::utils::emotion_dimensions_snapshot_text;
use zihuan_agent::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::qq_chat::QqChatEmotionDimensionConfig;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::{LLMMessage, MessageRole};
use zihuan_core::steer::message_with_api_style;
use zihuan_graph_engine::data_value::LLMMessageSessionCacheRef;

use crate::agent::tools::{
    AgentMemoryToolResources, GetRecentGroupMessagesBrainTool, GetRecentUserMessagesBrainTool,
    ListAvailableMemoryKeysBrainTool, SearchMemoryContentBrainTool, ToolNotificationTarget,
    UpdateAgentStateBrainTool, DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES, DEFAULT_TOOL_GET_RECENT_USER_MESSAGES,
    DEFAULT_TOOL_LIST_AVAILABLE_MEMORY_KEYS, DEFAULT_TOOL_SEARCH_MEMORY_CONTENT,
};
use crate::storage::qq_chat_history_store::{load_history, save_history};

use super::PreparedCurrentTurnUserInput;

const LOG_PREFIX: &str = "[QqChatPrepromptAgent]";

fn build_chat_preprompt_agent_system_prompt(bot_name: &str, emotion_snapshot: &str) -> String {
    format!(
        "You are the chat-preprompt agent for the QQ bot `{bot_name}`. You run before the main reply agent every turn and prepare a context block that anchors its reply. You have two responsibilities.\n\
         \n\
         [Responsibility 1: Emotion management]\n\
         Current emotion state:\n{emotion_snapshot}\n\
         Based on the current event and the independent emotion history, decide whether the emotion should be adjusted. Call `update_agent_state` only when a change is truly warranted; do not call any tool when no change is needed. When an adjustment is needed, specify an emotion dimension and `increase` or `decrease`. You may adjust multiple dimensions in the same event if each is genuinely necessary.\n\
         \n\
         [Responsibility 2: Recall & consistency preprompt]\n\
         - Extract the key nouns / entities / proper nouns from the user's current message.\n\
         - For each, call `search_memory_content` to check whether you have related memory or an existing stance.\n\
        - When memory contains your prior stance on a topic, surface it so the main agent stays consistent and does not flip its likes/dislikes or opinions across turns.\n\
         - For nouns that have no related memory and that you do not already know, include in the final context block a line exactly like: 「xxx」这些名词没有相关内容，可能需要联网查询？\n\
        - When the user's question references something you said before, or tests consistency of your preferences, call `get_recent_user_messages` with your own id (provided in the event message below) to recall your own previous replies; you may also pass the current sender's id (also provided below) to recall that user's recent messages. In a group you may also use `get_recent_group_messages` for surrounding context.\n\
        - Only surface prior statements that are genuinely relevant to the current topic; never include unrelated recent replies just because they are recent.\n\
        \n\
        [Output contract]\n\
        Your FINAL assistant message (the one with no further tool calls) MUST be a concise context block, and nothing else. Use this fixed shape, omitting any empty section:\n\
        [Recalled Memory]\n\
        - <title>: <brief>\n\
        [Missing Knowledge]\n\
        - 「xxx」这些名词没有相关内容，可能需要联网查询？\n\
        [Recent Self Statements] (仅纳入与当前话题相关的过往发言，话题不相关的不要引入)\n\
        - {bot_name} 之前说过: \"<summary>\"\n\
        [Emotion Note]\n\
         - <only when emotion changed this turn>\n\
         If nothing relevant was found and no emotion changed, output a single line: [Preprompt] no recall needed.\n\
         This block is injected into the main reply prompt; it is NOT a reply to the user. Never claim to have sent a message."
    )
}

fn build_chat_preprompt_agent_user_message(
    input: &PreparedCurrentTurnUserInput,
    bot_name: &str,
    bot_id: &str,
    sender_id: &str,
) -> String {
    let sender_name =
        ims_bot_adapter::utils::sender_display_name!(&input.event.sender.nickname, &input.event.sender.card);
    format!(
        "[Current QQ Event]\n`{sender_name}` sent a message to you (`{bot_name}`):\n{}\n\n\
         Your own id is `{bot_id}`; pass it to `get_recent_user_messages` to recall your own previous replies.\n\
         The current sender's id is `{sender_id}`; pass it to `get_recent_user_messages` to recall this user's recent messages.\n\
         Evaluate whether this event should change your emotion state, and prepare the preprompt context block per the output contract.",
        input.current_text_for_prompt()
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_chat_preprompt_agent(
    llm: &Arc<dyn LLMBase>,
    cache: &Arc<LLMMessageSessionCacheRef>,
    history_key: &str,
    input: &PreparedCurrentTurnUserInput,
    bot_name: &str,
    bot_id: &str,
    sender_id: &str,
    target_id: &str,
    is_group: bool,
    session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
    emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    compact_context_length: usize,
    memory_resources: Option<AgentMemoryToolResources>,
    rdb_pool: Option<RelationalDbConnection>,
    default_tools_enabled: &HashMap<String, bool>,
) -> Option<String> {
    let original_session_state = {
        let session_state = session_state.lock().unwrap();
        session_state.clone()
    };

    let emotion_snapshot = emotion_dimensions_snapshot_text(&original_session_state, &emotion_dimensions);
    let user_message = message_with_api_style(
        LLMMessage::user(build_chat_preprompt_agent_user_message(input, bot_name, bot_id, sender_id)),
        llm.api_style(),
    );

    let history = load_history(cache, history_key);
    let compact_result = compact_message_history(llm, history, compact_context_length, &user_message);
    let mut history = compact_result.messages;
    if compact_result.did_compact {
        info!(
            "{LOG_PREFIX} history compacted for {history_key}: tokens {} -> {}",
            compact_result.estimated_tokens_before, compact_result.estimated_tokens_after
        );
    }

    let mut conversation = Vec::with_capacity(history.len() + 2);
    conversation.push(message_with_api_style(
        LLMMessage::system(build_chat_preprompt_agent_system_prompt(bot_name, &emotion_snapshot)),
        llm.api_style(),
    ));
    conversation.extend(history.iter().cloned());
    conversation.push(user_message.clone());

    let mut brain = Brain::new(Arc::clone(llm));
    brain.add_tool(UpdateAgentStateBrainTool::new(
        Arc::clone(&session_state),
        emotion_dimensions,
        Arc::clone(llm),
        input.current_text_for_prompt().to_string(),
    ));

    let is_enabled = |name: &str| *default_tools_enabled.get(name).unwrap_or(&true);

    if let Some(memory_resources) = memory_resources {
        if is_enabled(DEFAULT_TOOL_SEARCH_MEMORY_CONTENT) {
            brain.add_tool(SearchMemoryContentBrainTool::new(memory_resources.clone()));
        }
        if is_enabled(DEFAULT_TOOL_LIST_AVAILABLE_MEMORY_KEYS) {
            brain.add_tool(ListAvailableMemoryKeysBrainTool::new(memory_resources));
        }
    }

    // The preprompt agent must not emit user-facing tool progress notifications, so the
    // notification target carries no adapter and has progress disabled. Read-only history
    // tools only read `target_id` / `is_group` from it and never send messages.
    let notification_target = ToolNotificationTarget::new(None, target_id.to_string(), None, is_group, false);

    if rdb_pool.is_some() && is_enabled(DEFAULT_TOOL_GET_RECENT_USER_MESSAGES) {
        brain.add_tool(GetRecentUserMessagesBrainTool::new(
            rdb_pool.clone(),
            notification_target.clone(),
        ));
    }
    if is_group && rdb_pool.is_some() && is_enabled(DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES) {
        brain.add_tool(GetRecentGroupMessagesBrainTool::new(rdb_pool, notification_target));
    }

    let (output, stop_reason) = brain.run(conversation);

    let context_block = match stop_reason {
        BrainStopReason::Done => output
            .iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
            .and_then(|message| message.content_text_owned())
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
        _ => {
            warn!("{LOG_PREFIX} inference ended without normal completion: {stop_reason:?}");
            *session_state.lock().unwrap() = original_session_state;
            None
        }
    };

    history.push(user_message);
    history.extend(output);
    save_history(cache, history_key, history);
    context_block
}
