pub mod string_utils;

use crate::emotion::utils::emotion_expression_prompt;
use crate::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::qq_chat::QqChatEmotionDimensionConfig;

/// Builds the prefix lines shared by all user-message construction paths:
/// `[Agent State Snapshot]` + `[System Instructions]`.
pub fn build_state_system_prefix_lines(
    session_state: &QqChatAgentServiceSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
    character_instructions: &str,
) -> Vec<String> {
    let mut lines = vec!["**Your character's current state**:".to_string()];
    let emotion_prompt = emotion_expression_prompt(session_state, emotion_dimensions);
    if !emotion_prompt.is_empty() {
        lines.push(format!("- Your emotion expression instructions: {emotion_prompt}"));
    }
    lines.push(format!("- Your character instructions: {character_instructions}"));
    lines
}
