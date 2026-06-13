pub mod string_utils;

use crate::emotion::utils::emotion_dimensions_snapshot_text;
use crate::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::QqChatEmotionDimensionConfig;

/// Builds the prefix lines shared by all user-message construction paths:
/// `[Agent State Snapshot]` + `[System Instructions]`.
pub fn build_state_system_prefix_lines(
    session_state: &QqChatAgentServiceSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
    character_instructions: &str,
) -> Vec<String> {
    vec![
        "**Your character's current state**:".to_string(),
        format!(
            "- Your emotion state: {}",
            emotion_dimensions_snapshot_text(session_state, emotion_dimensions)
        ),
        format!("- Your character instructions: {}", character_instructions),
    ]
}
