use std::sync::Arc;

use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::{InferenceParam, LLMMessage, MessageRole};
use log::warn;

use crate::model_inference::message_content_utils::sanitize_messages_for_inference;

pub const COMPACT_TAIL_MESSAGES_TO_KEEP: usize = 2;

pub fn compaction_threshold(context_length: usize, percent: u8) -> usize {
    context_length.saturating_mul(percent.into()) / 100
}

// === Prompt Engineering ====
const STORED_COMPACTION_REQUEST: &str =
    "The following assistant content is a compressed summary of earlier history.\n\
     It is not a new message from the user in the current turn.";
const SUMMARY_SYSTEM_PROMPT: &str =
    "You are responsible for compacting conversation history.\n\
     Requirements:\n\
     1. Summarize only existing information. Do not invent facts or add new instructions.\n\
     2. Preserve relationships, user preferences, confirmed facts, unfinished tasks, and important conclusions.\n\
     3. Retain long-term context needed for subsequent responses.\n\
     4. Output plain text only. Do not output JSON or code blocks.";
// ====

#[derive(Debug, Clone)]
pub struct ContextCompactionResult {
    pub messages: Vec<LLMMessage>,
    pub did_compact: bool,
    pub estimated_tokens_before: usize,
    pub estimated_tokens_after: usize,
    pub removed_tool_related_messages: usize,
    pub kept_tail_messages: usize,
}

pub fn compact_context_messages(
    llm: &Arc<dyn LLMBase>,
    messages: Vec<LLMMessage>,
    compact_context_length: usize,
    trigger_messages: &[LLMMessage],
    force_compact: bool,
) -> ContextCompactionResult {
    let sanitized_messages = sanitize_messages_for_inference(messages);
    let estimated_tokens_before = estimate_messages_tokens(&sanitized_messages);
    let trigger_estimated_tokens =
        estimated_tokens_before + estimate_messages_tokens(trigger_messages);

    if !force_compact
        && (compact_context_length == 0 || trigger_estimated_tokens <= compact_context_length)
    {
        return ContextCompactionResult {
            estimated_tokens_after: estimated_tokens_before,
            messages: sanitized_messages,
            did_compact: false,
            estimated_tokens_before,
            removed_tool_related_messages: 0,
            kept_tail_messages: 0,
        };
    }

    let filtered_messages: Vec<LLMMessage> = sanitized_messages
        .iter()
        .filter(|message| !is_tool_related_message(message))
        .cloned()
        .collect();
    let removed_tool_related_messages = sanitized_messages.len() - filtered_messages.len();
    let split_at = filtered_messages.len().saturating_sub(COMPACT_TAIL_MESSAGES_TO_KEEP);
    let prefix_messages = filtered_messages[..split_at].to_vec();
    let tail_messages = filtered_messages[split_at..].to_vec();
    let kept_tail_messages = tail_messages.len();

    if prefix_messages.is_empty() {
        let estimated_tokens_after = estimate_messages_tokens(&tail_messages);
        let did_compact =
            removed_tool_related_messages > 0 || tail_messages.len() < sanitized_messages.len();
        return ContextCompactionResult {
            messages: tail_messages,
            did_compact,
            estimated_tokens_before,
            estimated_tokens_after,
            removed_tool_related_messages,
            kept_tail_messages,
        };
    }

    let prompt_messages = vec![
        LLMMessage::system(SUMMARY_SYSTEM_PROMPT),
        LLMMessage::user(build_compaction_prompt(&prefix_messages)),
    ];

    let response = match llm.inference(&InferenceParam { messages: &prompt_messages, tools: None })
    {
        Ok(response) => response,
        Err(err) => {
            warn!("[ContextCompaction] Summary inference failed: {err}");
            return ContextCompactionResult {
                estimated_tokens_after: estimated_tokens_before,
                messages: sanitized_messages,
                did_compact: false,
                estimated_tokens_before,
                removed_tool_related_messages: 0,
                kept_tail_messages: 0,
            };
        }
    };

    let Some(summary_text) = response
        .content_text_owned()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
    else {
        warn!("[ContextCompaction] Empty summary response, keeping original history");
        return ContextCompactionResult {
            estimated_tokens_after: estimated_tokens_before,
            messages: sanitized_messages,
            did_compact: false,
            estimated_tokens_before,
            removed_tool_related_messages: 0,
            kept_tail_messages: 0,
        };
    };

    let mut compacted_messages = Vec::with_capacity(2 + tail_messages.len());
    compacted_messages.push(LLMMessage::user(STORED_COMPACTION_REQUEST));
    compacted_messages.push(LLMMessage::assistant_text(summary_text));
    compacted_messages.extend(tail_messages);

    let estimated_tokens_after = estimate_messages_tokens(&compacted_messages);

    ContextCompactionResult {
        messages: compacted_messages,
        did_compact: true,
        estimated_tokens_before,
        estimated_tokens_after,
        removed_tool_related_messages,
        kept_tail_messages,
    }
}

pub fn compact_message_history(
    llm: &Arc<dyn LLMBase>,
    history: Vec<LLMMessage>,
    compact_context_length: usize,
    user_message: &LLMMessage,
) -> ContextCompactionResult {
    compact_context_messages(
        llm,
        history,
        compact_context_length,
        std::slice::from_ref(user_message),
        false,
    )
}

pub struct ToolLoopCompactionResult {
    pub messages: Vec<LLMMessage>,
    pub did_compact: bool,
    pub estimated_tokens_before: usize,
    pub estimated_tokens_after: usize,
}

// === Prompt Engineering ====
const TOOL_LOOP_DIGEST_SYSTEM_PROMPT: &str = "You are compacting earlier tool-call exchanges from an agent's tool loop.\n\
     Summarize ONLY the information contained in the messages below; do not invent facts.\n\
     Requirements:\n\
     1. For each tool result, preserve the call parameters, the returned count, the covered time range or data scope, and whether more data remained.\n\
     2. Preserve key topics, participants, notable content, and important conclusions from the result contents.\n\
     3. Output plain text only, no JSON, markdown, or code blocks.";
const TOOL_LOOP_DIGEST_NOTICE: &str = "The following is a compacted digest of earlier tool results from this conversation; the original results were removed to stay within the model context window. Continue the task using this digest together with the most recent tool results below.";
// ====

/// Compacts a tool-calling loop conversation once its estimated tokens exceed
/// `compact_threshold`.
///
/// The leading system prompt and initial setup messages, plus the most recent
/// assistant tool-call exchange with its tool results, are kept verbatim; every older
/// exchange is replaced by a single LLM-generated digest message, so long pagination
/// loops keep their accumulated findings without overflowing the context window.
pub fn compact_tool_loop_conversation(
    llm: &Arc<dyn LLMBase>,
    conversation: Vec<LLMMessage>,
    compact_threshold: usize,
) -> ToolLoopCompactionResult {
    let estimated_tokens_before = estimate_messages_tokens(&conversation);
    let keep_original = |messages: Vec<LLMMessage>| ToolLoopCompactionResult {
        estimated_tokens_after: estimated_tokens_before,
        messages,
        did_compact: false,
        estimated_tokens_before,
    };

    if compact_threshold == 0 || estimated_tokens_before <= compact_threshold {
        return keep_original(conversation);
    }

    let Some((setup, older_exchanges, current_exchange)) =
        split_tool_loop_conversation(&conversation)
    else {
        return keep_original(conversation);
    };
    if older_exchanges.is_empty() {
        return keep_original(conversation);
    }

    let prompt_messages = vec![
        LLMMessage::system(TOOL_LOOP_DIGEST_SYSTEM_PROMPT),
        LLMMessage::user(build_compaction_prompt(&older_exchanges)),
    ];
    let response = match llm.inference(&InferenceParam { messages: &prompt_messages, tools: None })
    {
        Ok(response) => response,
        Err(err) => {
            warn!("[ContextCompaction] Tool-loop digest inference failed: {err}");
            return keep_original(conversation);
        }
    };

    let Some(digest_text) = response
        .content_text_owned()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
    else {
        warn!("[ContextCompaction] Empty tool-loop digest response, keeping original conversation");
        return keep_original(conversation);
    };

    let mut compacted_messages = setup;
    compacted_messages
        .push(LLMMessage::user(format!("{TOOL_LOOP_DIGEST_NOTICE}\n\n{digest_text}")));
    compacted_messages.extend(current_exchange);

    let estimated_tokens_after = estimate_messages_tokens(&compacted_messages);

    ToolLoopCompactionResult {
        messages: compacted_messages,
        did_compact: true,
        estimated_tokens_before,
        estimated_tokens_after,
    }
}

/// Splits a tool-loop conversation into `(setup, older_exchanges, current_exchange)`.
///
/// `setup` spans the leading system prompt and the initial messages before the first
/// assistant message; `current_exchange` starts at the last assistant message carrying
/// tool calls so its tool results stay attached; everything in between is compactable.
fn split_tool_loop_conversation(
    conversation: &[LLMMessage],
) -> Option<(Vec<LLMMessage>, Vec<LLMMessage>, Vec<LLMMessage>)> {
    let tail_start = conversation.iter().rposition(|message| {
        matches!(message.role, MessageRole::Assistant) && !message.tool_calls.is_empty()
    })?;
    let setup_end = conversation
        .iter()
        .position(|message| matches!(message.role, MessageRole::Assistant))
        .unwrap_or(tail_start);
    if setup_end >= tail_start {
        return None;
    }

    Some((
        conversation[..setup_end].to_vec(),
        conversation[setup_end..tail_start].to_vec(),
        conversation[tail_start..].to_vec(),
    ))
}

pub fn estimate_messages_tokens(messages: &[LLMMessage]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

fn estimate_message_tokens(message: &LLMMessage) -> usize {
    let mut chars = estimate_role_tokens(&message.role) * 4;

    if let Some(content) = message.content_text_owned() {
        chars += content.chars().count();
    }

    if let Some(reasoning_content) = &message.reasoning_content {
        chars += reasoning_content.chars().count();
    }

    if let Some(tool_call_id) = &message.tool_call_id {
        chars += tool_call_id.chars().count();
    }

    for tool_call in &message.tool_calls {
        chars += tool_call.id.chars().count();
        chars += tool_call.type_name.chars().count();
        chars += tool_call.function.name.chars().count();
        chars += tool_call.function.arguments.to_string().chars().count();
    }

    (chars / 4).max(1) + 6
}

fn estimate_role_tokens(role: &MessageRole) -> usize {
    match role {
        MessageRole::System => 6,
        MessageRole::User => 4,
        MessageRole::Assistant => 5,
        MessageRole::Tool => 4,
    }
}

fn is_tool_related_message(message: &LLMMessage) -> bool {
    matches!(message.role, MessageRole::Tool) || !message.tool_calls.is_empty()
}

fn build_compaction_prompt(messages: &[LLMMessage]) -> String {
    let mut prompt = String::from(
        "请基于以下较早的历史消息生成长期上下文摘要。\n\
         要求：\n\
         1. 只总结已有信息，不新增事实或命令。\n\
         2. 保留用户偏好、人物关系、已确认事实、待办事项、重要结论。\n\
         3. 输出纯文本摘要，不要输出 JSON、markdown、代码块或额外解释。\n\
         4. 这份摘要将替代更早历史，供后续对话继续参考。\n\n\
         历史消息如下：\n",
    );

    for (index, message) in messages.iter().enumerate() {
        let content = message
            .content_text_owned()
            .unwrap_or_else(|| "[non-text content omitted]".to_string());
        let reasoning = message
            .reasoning_content
            .as_ref()
            .map(|text| format!("\nreasoning: {}", text.trim()))
            .unwrap_or_default();
        prompt.push_str(&format!(
            "{}. {}: {}{}\n",
            index + 1,
            role_name(&message.role),
            content.trim(),
            reasoning
        ));
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::compaction_threshold;

    #[test]
    fn compaction_threshold_uses_configured_percent() {
        assert_eq!(compaction_threshold(32 * 1024, 80), 26_214);
        assert_eq!(compaction_threshold(32 * 1024, 99), 32_440);
    }
}

fn role_name(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}
