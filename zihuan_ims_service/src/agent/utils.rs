use zihuan_core::agent::tools::memory::MemoryRecall;

/// Builds the per-turn state prefix lines that stay in the user message.
///
/// The stable prompt material — identity, tool rules, the role service's own prompt, the reply
/// output contract and the language style — travels as the conversation's system message. This
/// helper therefore carries only what changes between turns: the current emotion prompt and the
/// memories recalled for this message.
///
/// `memory_recall` is the typed recall prepared by the recall phase; it is optional because turns
/// without a memory backend carry no hint.
pub fn build_turn_state_prefix_lines(
    emotion_prompt: &str,
    memory_recall: Option<&MemoryRecall>,
) -> Vec<String> {
    let mut lines = Vec::new();
    if !emotion_prompt.trim().is_empty() {
        lines.push("**Your character's current state**:".to_string());
        lines.push(format!("- {emotion_prompt}"));
    }
    // The block is emitted whenever the recall phase ran, even with no hits: its source line is
    // the only place the model learns which authenticated sender the memory tools act for.
    if let Some(recall) = memory_recall {
        lines.push("[Relevant Memory Titles]".to_string());
        if let Some(source) = recall.source() {
            lines.push(format!("Authenticated memory request source: {source}"));
        }
        let entries = memory_hint_lines(recall);
        if entries.is_empty() {
            lines.push(
                "No memory titles matched this turn; call `search_memory` for anything missing."
                    .to_string(),
            );
        } else {
            lines.push(
                "Recalled memory titles (call `search_memory` for their content):".to_string(),
            );
            lines.extend(entries);
        }
    }
    lines
}

/// Renders one recalled memory per line, tagged with its category.
///
/// A concept is named by its title, a person by its name plus stored key (the QQ ID stays the
/// identity), and a behavior by the rule itself — its key is an opaque hash and carries no
/// meaning. The hint only says which memories exist; their content stays reachable through
/// `search_memory`.
pub fn memory_hint_lines(recall: &MemoryRecall) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(
        recall
            .concepts()
            .iter()
            .map(|memory| format!("- [concept] {}", memory.title())),
    );
    lines.extend(
        recall
            .people()
            .iter()
            .map(|memory| format!("- [person] {}", memory.display_title())),
    );
    lines.extend(
        recall
            .behaviors()
            .iter()
            .map(|memory| format!("- [behavioral] {}", memory.rule())),
    );
    lines
}

/// A compact, serializable view of a recall for the turn's trace stages.
///
/// The Data Explorer shows what the turn's prompt was built from; it only needs the entries, not
/// the backend records behind them.
pub fn memory_recall_trace(recall: &MemoryRecall) -> serde_json::Value {
    serde_json::json!({
        "source": recall.source(),
        "entries": memory_hint_lines(recall),
        "total": recall.len(),
    })
}
