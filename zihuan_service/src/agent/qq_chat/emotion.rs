use std::sync::{Arc, Mutex};

use log::{info, warn};
use model_inference::inference_function::compact_message::compact_message_history;

use zihuan_agent::brain::{Brain, BrainStopReason};
use zihuan_agent::emotion::utils::emotion_dimensions_snapshot_text;
use zihuan_agent::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::qq_chat::QqChatEmotionDimensionConfig;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::LLMMessage;
use zihuan_core::steer::message_with_api_style;
use zihuan_graph_engine::data_value::LLMMessageSessionCacheRef;

use crate::agent::tools::UpdateAgentStateBrainTool;
use crate::storage::qq_chat_history_store::{load_history, save_history};

use super::PreparedCurrentTurnUserInput;

const LOG_PREFIX: &str = "[QqChatEmotionAgent]";

fn build_emotion_agent_system_prompt(bot_name: &str, emotion_snapshot: &str) -> String {
    format!(
       "You are the emotion-management agent for the QQ bot `{bot_name}`. You are only responsible for deciding whether the current event warrants changing the bot's emotion dimensions.\n\
        Current emotion state:\n{emotion_snapshot}\n\
        Based on the current event and the independent emotion history, decide whether the emotion should be adjusted. Call `update_agent_state` only when a change is truly warranted; do not call any tool when no change is needed.\n\
        When an adjustment is needed, use that tool only to specify an emotion dimension and `increase` or `decrease`. You may adjust multiple dimensions in the same event if each is genuinely necessary.\n\
        Stop once the decision is complete; never produce a user-facing reply, and do not claim to have sent any message to the user."
    )
}

fn build_emotion_agent_user_message(input: &PreparedCurrentTurnUserInput, bot_name: &str) -> String {
    let sender_name =
        ims_bot_adapter::utils::sender_display_name!(&input.event.sender.nickname, &input.event.sender.card);
    format!(
        "[Current QQ Event]\n`{sender_name}` sent a message to you (`{bot_name}`):\n{}\n\nEvaluate only whether this event should change your emotion state.",
        input.current_text_for_prompt()
    )
}

pub(crate) fn run_emotion_agent(
    llm: &Arc<dyn LLMBase>,
    cache: &Arc<LLMMessageSessionCacheRef>,
    history_key: &str,
    input: &PreparedCurrentTurnUserInput,
    bot_name: &str,
    session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
    emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    compact_context_length: usize,
) {
    let original_session_state = {
        let session_state = session_state.lock().unwrap();
        session_state.clone()
    };
    let emotion_snapshot = emotion_dimensions_snapshot_text(&original_session_state, &emotion_dimensions);
    let user_message = message_with_api_style(
        LLMMessage::user(build_emotion_agent_user_message(input, bot_name)),
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
        LLMMessage::system(build_emotion_agent_system_prompt(bot_name, &emotion_snapshot)),
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
    let (output, stop_reason) = brain.run(conversation);
    if !matches!(stop_reason, BrainStopReason::Done) {
        warn!("{LOG_PREFIX} inference ended without normal completion: {stop_reason:?}");
        *session_state.lock().unwrap() = original_session_state;
    }

    history.push(user_message);
    history.extend(output);
    save_history(cache, history_key, history);
}
