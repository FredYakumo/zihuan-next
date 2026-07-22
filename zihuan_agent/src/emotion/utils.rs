use crate::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::qq_chat::QqChatEmotionDimensionConfig;

const NOTICEABLE_EMOTION_THRESHOLD: f64 = 20.0;
const STRONG_EMOTION_THRESHOLD: f64 = 60.0;

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

/// Builds model-facing emotion instructions from dynamically generated prompts and current weights.
/// Raw dimension names and numeric values intentionally never leave this module.
pub fn emotion_expression_prompt(
    session_state: &QqChatAgentServiceSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
) -> String {
    emotion_prompt_entries(session_state, emotion_dimensions)
        .into_iter()
        .map(|(weight, prompt)| {
            if weight < NOTICEABLE_EMOTION_THRESHOLD {
                format!("Slightly follow [{prompt}] for output.")
            } else if weight < STRONG_EMOTION_THRESHOLD {
                format!("Follow [{prompt}] for output.")
            } else {
                format!("Strongly use [{prompt}], must strictly follow [{prompt}] for output.")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns whether an emotion instruction is strong enough to take precedence over language style.
pub fn has_noticeable_emotion_expression(
    session_state: &QqChatAgentServiceSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
) -> bool {
    emotion_prompt_entries(session_state, emotion_dimensions)
        .into_iter()
        .any(|(weight, _)| weight >= NOTICEABLE_EMOTION_THRESHOLD)
}

fn emotion_prompt_entries(
    session_state: &QqChatAgentServiceSessionState,
    emotion_dimensions: &[QqChatEmotionDimensionConfig],
) -> Vec<(f64, String)> {
    emotion_dimensions
        .iter()
        .filter_map(|dimension| {
            let value = *session_state.emotion_dimensions.get(dimension.name.trim()).unwrap_or(&0.0);
            if !value.is_finite() || value == 0.0 {
                return None;
            }

            let prompt = session_state.emotion_expression_prompts.get(dimension.name.trim())?.trim();
            if prompt.is_empty() {
                return None;
            }
            Some((value.abs(), prompt.to_string()))
        })
        .collect()
}
