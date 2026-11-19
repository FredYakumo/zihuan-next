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
    // Emit the recall hint even when nothing matched this turn.
    if let Some(recall) = memory_recall {
        lines.push("[Recalled Memories]".to_string());
        let entries = memory_hint_lines(recall);
        if entries.is_empty() {
            lines.push(
                "No memories matched this turn; call `search_memory` for anything missing."
                    .to_string(),
            );
        } else {
            lines.push(
                "Recalled memories (behavioral rules are shown in full; concept entries list only their key — call `search_memory` with the key for the stored content):"
                    .to_string(),
            );
            lines.extend(entries);
        }
    }
    lines
}

/// Renders one recalled memory per line, tagged with its category.
///
/// A behavioral memory shows its key and the full rule on the next line; a concept shows only
/// its key, whose stored content stays reachable through `search_memory`; a person is named by
/// its name plus explicit QQ identity (names never prove identity). This renders both the turn's
/// hint and the compaction refresh hint, so both carry the same shape.
pub fn memory_hint_lines(recall: &MemoryRecall) -> Vec<String> {
    let mut lines = Vec::new();
    lines.extend(
        recall
            .behaviors()
            .iter()
            .map(|memory| format!("- [behavioral] {}\n  {}", memory.key(), memory.content())),
    );
    lines.extend(recall.concepts().iter().map(|memory| format!("- [concept] {}", memory.key())));
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
