use crate::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::qq_chat::QqChatEmotionDimensionConfig;

/// Formats the current emotion dimension state as a multi-line display string.
///
/// Each line follows the pattern: `{name}: {value}`.
/// Returns `[No emotion dimensions]` when no dimensions are configured.
pub fn emotion_dimensions_snapshot_text(
    session_state: &QqChatAgentServiceSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
) -> String {
    let lines: Vec<String> = session_state
        .ordered_emotion_dimensions(emotion_dimensions)
        .into_iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect();
    if lines.is_empty() {
        "[No emotion dimensions]".to_string()
    } else {
        lines.join("\n")
    }
}
