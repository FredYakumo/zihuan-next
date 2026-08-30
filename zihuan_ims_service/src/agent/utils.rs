/// Builds the prefix lines shared by all user-message construction paths:
/// `[Agent State Snapshot]` + `[System Instructions]`.
pub fn build_state_system_prefix_lines(
    emotion_prompt: &str,
    character_instructions: &str,
    preprompt_context: Option<&str>,
) -> Vec<String> {
    let mut lines = vec!["**Your character's current state**:".to_string()];
    if !emotion_prompt.is_empty() {
        lines.push(format!("- {emotion_prompt}"));
    }
    lines.push(format!("- Your character instructions: {character_instructions}"));
    if let Some(block) = preprompt_context.map(str::trim).filter(|text| !text.is_empty()) {
        lines.push(format!("- [Preprompt Context]\n{block}"));
    }
    lines
}
