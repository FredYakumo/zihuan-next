use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokenizers::Tokenizer;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::{FunctionTool, ToolCalls, ToolCallsFuncSpec};
use zihuan_core::llm::{InferenceParam, LLMMessage, MessagePart, StreamToken, TokenUsage};

use crate::message_content_utils::downgrade_messages_for_model;

pub const DEFAULT_MAX_NEW_TOKENS: usize = 512;
pub const USER_VISIBLE_REQUEST_ERROR: &str = "Error: local Candle inference failed";

pub fn render_local_prompt(messages: &[LLMMessage], tools: Option<&Vec<Arc<dyn FunctionTool>>>) -> String {
    let mut sections = Vec::new();
    sections.push(
        "You are a local assistant running inside zihuan-next. Reply concisely in plain text.\nIf you need to call a tool, output a single line starting with CALL_TOOL followed by strict JSON: {\"name\":\"tool_name\",\"arguments\":{...}}.\nDo not wrap the tool call in markdown.".to_string(),
    );

    if let Some(tools) = tools.filter(|tools| !tools.is_empty()) {
        let tool_descriptions = tools
            .iter()
            .map(|tool| {
                format!(
                    "- {}: {}\n  parameters: {}",
                    tool.name(),
                    tool.description(),
                    tool.parameters()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        sections.push(format!("Available tools:\n{}", tool_descriptions));
    }

    for message in messages {
        match message.role {
            zihuan_core::llm::MessageRole::System => {
                sections.push(format!("System:\n{}", join_parts(&message.parts)));
            }
            zihuan_core::llm::MessageRole::User => {
                sections.push(format!("User:\n{}", join_parts(&message.parts)));
            }
            zihuan_core::llm::MessageRole::Assistant => {
                if !message.tool_calls.is_empty() {
                    sections.push(format!(
                        "Assistant tool calls:\n{}",
                        serde_json::to_string(&message.tool_calls).unwrap_or_default()
                    ));
                } else {
                    sections.push(format!("Assistant:\n{}", join_parts(&message.parts)));
                }
            }
            zihuan_core::llm::MessageRole::Tool => {
                sections.push(format!(
                    "Tool result (tool_call_id={}):\n{}",
                    message.tool_call_id.as_deref().unwrap_or("unknown"),
                    join_parts(&message.parts)
                ));
            }
        }
    }
    sections.push("Assistant:".to_string());
    sections.join("\n\n")
}

pub fn prepare_prompt(param: &InferenceParam, supports_multimodal_input: bool) -> Result<(String, usize)> {
    let downgraded = downgrade_messages_for_model(param.messages.clone(), supports_multimodal_input);
    let prompt = render_local_prompt(&downgraded, param.tools);
    let prompt_chars = prompt.chars().count();
    if prompt_chars == 0 {
        return Err(Error::ValidationError("local llm prompt must not be empty".to_string()));
    }
    Ok((prompt, prompt_chars))
}

pub fn decode_token_piece(tokenizer: &Tokenizer, token_id: u32) -> Option<StreamToken> {
    tokenizer.decode(&[token_id], false).ok().and_then(|piece| {
        if piece.is_empty() {
            None
        } else {
            Some(StreamToken::content(piece))
        }
    })
}

pub fn build_usage(prompt_tokens: usize, completion_tokens: usize) -> Option<TokenUsage> {
    Some(TokenUsage {
        prompt_tokens: Some(prompt_tokens),
        completion_tokens: Some(completion_tokens),
        total_tokens: Some(prompt_tokens + completion_tokens),
        ..Default::default()
    })
}

pub fn parse_local_response(output_text: &str) -> LLMMessage {
    let trimmed = output_text.trim();
    if let Some(payload) = trimmed.strip_prefix("CALL_TOOL ") {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
            if let Some(name) = value.get("name").and_then(|item| item.as_str()) {
                let arguments = value
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                return LLMMessage {
                    role: zihuan_core::llm::MessageRole::Assistant,
                    parts: Vec::new(),
                    reasoning_content: None,
                    tool_calls: vec![ToolCalls {
                        id: next_tool_call_id(),
                        type_name: "function".to_string(),
                        function: ToolCallsFuncSpec {
                            name: name.to_string(),
                            arguments,
                        },
                    }],
                    tool_call_id: None,
                    usage: None,
                };
            }
        }
    }
    LLMMessage::assistant_text(trimmed)
}

fn join_parts(parts: &[MessagePart]) -> String {
    parts
        .iter()
        .map(|part| match part {
            MessagePart::Text { text } => text.clone(),
            MessagePart::Image { media } => format!("[image omitted] {}", media.primary_locator().unwrap_or_default()),
            MessagePart::Video { media } => format!("[video omitted] {}", media.primary_locator().unwrap_or_default()),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn next_tool_call_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    format!("local-tool-call-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}
