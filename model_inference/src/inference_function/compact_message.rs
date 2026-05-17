use std::sync::Arc;

use log::warn;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::{InferenceParam, MessageRole, OpenAIMessage};

use crate::message_content_utils::{is_transport_error, sanitize_messages_for_inference};

pub const COMPACT_TAIL_MESSAGES_TO_KEEP: usize = 2;

const STORED_COMPACTION_REQUEST: &str =
    "以下 assistant 内容是对更早历史的压缩摘要，不代表当前轮用户的新发言。";
const SUMMARY_SYSTEM_PROMPT: &str = "你负责压缩对话历史。你只能总结已有信息，不能创造新事实、不能加入新指令、不能输出 JSON 或代码块。请重点保留人物关系、用户偏好、已确认事实、未完成事项、重要结论，以及后续回复需要延续的长期上下文。";

#[derive(Debug, Clone)]
pub struct ContextCompactionResult {
    pub messages: Vec<OpenAIMessage>,
    pub did_compact: bool,
    pub estimated_tokens_before: usize,
    pub estimated_tokens_after: usize,
    pub removed_tool_related_messages: usize,
    pub kept_tail_messages: usize,
}

pub fn compact_message_history(
    llm: &Arc<dyn LLMBase>,
    history: Vec<OpenAIMessage>,
    compact_context_length: usize,
    user_message: &OpenAIMessage,
) -> ContextCompactionResult {
    compact_context_messages(
        llm,
        history,
        compact_context_length,
        std::slice::from_ref(user_message),
        false,
    )
}

pub fn compact_context_messages(
    llm: &Arc<dyn LLMBase>,
    messages: Vec<OpenAIMessage>,
    compact_context_length: usize,
    trigger_messages: &[OpenAIMessage],
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

    let filtered_messages: Vec<OpenAIMessage> = sanitized_messages
        .iter()
        .filter(|message| !is_tool_related_message(message))
        .cloned()
        .collect();
    let removed_tool_related_messages = sanitized_messages.len() - filtered_messages.len();
    let split_at = filtered_messages
        .len()
        .saturating_sub(COMPACT_TAIL_MESSAGES_TO_KEEP);
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
        OpenAIMessage::system(SUMMARY_SYSTEM_PROMPT),
        OpenAIMessage::user(build_compaction_prompt(&prefix_messages)),
    ];

    let response = llm.inference(&InferenceParam {
        messages: &prompt_messages,
        tools: None,
    });

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

    if is_transport_error(&summary_text) {
        warn!("[ContextCompaction] Summary inference failed: {summary_text}");
        return ContextCompactionResult {
            estimated_tokens_after: estimated_tokens_before,
            messages: sanitized_messages,
            did_compact: false,
            estimated_tokens_before,
            removed_tool_related_messages: 0,
            kept_tail_messages: 0,
        };
    }

    let mut compacted_messages = Vec::with_capacity(2 + tail_messages.len());
    compacted_messages.push(OpenAIMessage::user(STORED_COMPACTION_REQUEST));
    compacted_messages.push(OpenAIMessage::assistant_text(summary_text));
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

pub fn estimate_messages_tokens(messages: &[OpenAIMessage]) -> usize {
    messages.iter().map(estimate_message_tokens).sum()
}

fn estimate_message_tokens(message: &OpenAIMessage) -> usize {
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

fn is_tool_related_message(message: &OpenAIMessage) -> bool {
    matches!(message.role, MessageRole::Tool) || !message.tool_calls.is_empty()
}

fn build_compaction_prompt(messages: &[OpenAIMessage]) -> String {
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

fn role_name(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}
