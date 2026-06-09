use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokenizers::Tokenizer;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::{FunctionTool, ToolCalls, ToolCallsFuncSpec};
use zihuan_core::llm::{InferenceParam, LLMMessage, MessagePart, StreamToken, TokenUsage};

use crate::message_content_utils::downgrade_messages_for_model;

pub const DEFAULT_MAX_NEW_TOKENS: usize = 512;
pub const USER_VISIBLE_REQUEST_ERROR: &str = "Error: local Candle inference failed";

const THINK_OPEN_TAG: &str = "<think>";
const THINK_CLOSE_TAG: &str = "</think>";
const CALL_TOOL_MARKER: &str = "CALL_TOOL ";
const STREAM_CONTENT_MARKERS: &[&str] = &[THINK_OPEN_TAG, CALL_TOOL_MARKER, "\nCALL_TOOL ", "\r\nCALL_TOOL "];

#[derive(Debug, Clone)]
pub struct ParsedLocalResponse {
    pub content: String,
    pub reasoning_content: Option<String>,
    pub tool_calls: Vec<ToolCalls>,
    pub saw_tool_call_marker: bool,
    pub parsed_tool_call: bool,
}

impl ParsedLocalResponse {
    pub fn into_message(self) -> LLMMessage {
        LLMMessage {
            role: zihuan_core::llm::MessageRole::Assistant,
            parts: if self.content.is_empty() {
                Vec::new()
            } else {
                vec![MessagePart::text(self.content)]
            },
            reasoning_content: self.reasoning_content,
            tool_calls: self.tool_calls,
            tool_call_id: None,
            usage: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LocalStreamMode {
    Content,
    Thinking,
    ToolCall,
}

#[derive(Debug)]
pub struct LocalResponseStreamRenderer {
    mode: LocalStreamMode,
    pending: String,
}

impl Default for LocalResponseStreamRenderer {
    fn default() -> Self {
        Self {
            mode: LocalStreamMode::Content,
            pending: String::new(),
        }
    }
}

impl LocalResponseStreamRenderer {
    pub fn push_piece(&mut self, piece: &str) -> Vec<StreamToken> {
        self.pending.push_str(piece);
        let mut emitted = Vec::new();

        loop {
            match self.mode {
                LocalStreamMode::ToolCall => {
                    self.pending.clear();
                    break;
                }
                LocalStreamMode::Content => {
                    if let Some((pos, marker)) = earliest_marker(&self.pending, STREAM_CONTENT_MARKERS) {
                        let marker_len = marker.len();
                        let is_think_marker = marker == THINK_OPEN_TAG;
                        push_if_non_empty(
                            &mut emitted,
                            StreamToken::content(self.pending[..pos].trim_end_matches(char::is_whitespace)),
                        );
                        self.pending.drain(..pos + marker_len);
                        self.mode = if is_think_marker {
                            LocalStreamMode::Thinking
                        } else {
                            LocalStreamMode::ToolCall
                        };
                        continue;
                    }

                    let safe_len = stable_prefix_len(&self.pending, STREAM_CONTENT_MARKERS);
                    if safe_len == 0 {
                        break;
                    }
                    push_if_non_empty(&mut emitted, StreamToken::content(&self.pending[..safe_len]));
                    self.pending.drain(..safe_len);
                }
                LocalStreamMode::Thinking => {
                    if let Some(pos) = self.pending.find(THINK_CLOSE_TAG) {
                        push_if_non_empty(&mut emitted, StreamToken::thinking(&self.pending[..pos]));
                        self.pending.drain(..pos + THINK_CLOSE_TAG.len());
                        self.mode = LocalStreamMode::Content;
                        continue;
                    }

                    let safe_len = stable_prefix_len(&self.pending, &[THINK_CLOSE_TAG]);
                    if safe_len == 0 {
                        break;
                    }
                    push_if_non_empty(&mut emitted, StreamToken::thinking(&self.pending[..safe_len]));
                    self.pending.drain(..safe_len);
                }
            }
        }

        emitted
    }

    pub fn finish(&mut self) -> Vec<StreamToken> {
        let mut emitted = Vec::new();
        match self.mode {
            LocalStreamMode::Content => {
                push_if_non_empty(&mut emitted, StreamToken::content(self.pending.trim_end_matches('\0')));
            }
            LocalStreamMode::Thinking => {
                push_if_non_empty(&mut emitted, StreamToken::thinking(self.pending.trim_end_matches('\0')));
            }
            LocalStreamMode::ToolCall => {}
        }
        self.pending.clear();
        emitted
    }
}

pub fn render_local_prompt(messages: &[LLMMessage], tools: Option<&Vec<Arc<dyn FunctionTool>>>) -> String {
    let mut sections = Vec::new();
    sections.push(
        "You are a local assistant running inside zihuan-next. Reply concisely in plain text.\nIf you need hidden reasoning, place it inside <think>...</think> and do not mention the tags in the final answer.\nIf you need to call a tool, output a single line starting with CALL_TOOL followed by strict JSON: {\"name\":\"tool_name\",\"arguments\":{...}}.\nDo not wrap the tool call in markdown.".to_string(),
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

pub fn decode_token_piece(tokenizer: &Tokenizer, token_id: u32) -> Option<String> {
    tokenizer.decode(&[token_id], false).ok().and_then(|piece| {
        if piece.is_empty() {
            None
        } else {
            Some(piece)
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

pub fn parse_local_response(output_text: &str) -> ParsedLocalResponse {
    let mut content = String::new();
    let mut reasoning = String::new();
    let mut mode = LocalStreamMode::Content;
    let mut saw_tool_call_marker = false;
    let mut tool_calls = Vec::new();
    let mut parsed_tool_call = false;

    let mut cursor = 0;
    while cursor < output_text.len() {
        let remainder = &output_text[cursor..];
        match mode {
            LocalStreamMode::Content => {
                if remainder.starts_with(THINK_OPEN_TAG) {
                    cursor += THINK_OPEN_TAG.len();
                    mode = LocalStreamMode::Thinking;
                    continue;
                }
                if remainder.starts_with(CALL_TOOL_MARKER) {
                    saw_tool_call_marker = true;
                    let payload = remainder[CALL_TOOL_MARKER.len()..].trim();
                    if let Some(tool_call) = parse_tool_call_payload(payload) {
                        tool_calls.push(tool_call);
                        parsed_tool_call = true;
                    }
                    break;
                }
                let ch = remainder.chars().next().expect("cursor is at char boundary");
                content.push(ch);
                cursor += ch.len_utf8();
            }
            LocalStreamMode::Thinking => {
                if remainder.starts_with(THINK_CLOSE_TAG) {
                    cursor += THINK_CLOSE_TAG.len();
                    mode = LocalStreamMode::Content;
                    continue;
                }
                let ch = remainder.chars().next().expect("cursor is at char boundary");
                reasoning.push(ch);
                cursor += ch.len_utf8();
            }
            LocalStreamMode::ToolCall => break,
        }
    }

    let content = content.trim().to_string();
    let reasoning_content = trim_to_option(reasoning);

    ParsedLocalResponse {
        content,
        reasoning_content,
        tool_calls,
        saw_tool_call_marker,
        parsed_tool_call,
    }
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

fn parse_tool_call_payload(payload: &str) -> Option<ToolCalls> {
    let value = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let name = value.get("name")?.as_str()?;
    let arguments = value
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));

    Some(ToolCalls {
        id: next_tool_call_id(),
        type_name: "function".to_string(),
        function: ToolCallsFuncSpec {
            name: name.to_string(),
            arguments,
        },
    })
}

fn next_tool_call_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    format!("local-tool-call-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

fn push_if_non_empty(tokens: &mut Vec<StreamToken>, token: StreamToken) {
    if token.as_str().is_empty() {
        return;
    }
    tokens.push(token);
}

fn earliest_marker<'a>(text: &'a str, markers: &'a [&'a str]) -> Option<(usize, &'a str)> {
    markers
        .iter()
        .filter_map(|marker| text.find(marker).map(|pos| (pos, *marker)))
        .min_by_key(|(pos, _)| *pos)
}

fn stable_prefix_len(text: &str, markers: &[&str]) -> usize {
    let partial_len = partial_marker_suffix_len(text, markers);
    text.len().saturating_sub(partial_len)
}

fn partial_marker_suffix_len(text: &str, markers: &[&str]) -> usize {
    let max_suffix_len = markers
        .iter()
        .map(|marker| marker.len().saturating_sub(1))
        .max()
        .unwrap_or(0)
        .min(text.len());

    for suffix_len in (1..=max_suffix_len).rev() {
        let start = text.len() - suffix_len;
        if !text.is_char_boundary(start) {
            continue;
        }
        let suffix = &text[start..];
        if markers.iter().any(|marker| marker.starts_with(suffix)) {
            return suffix_len;
        }
    }

    0
}

fn trim_to_option(text: String) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_local_response, LocalResponseStreamRenderer};
    use zihuan_core::llm::StreamToken;

    #[test]
    fn parse_local_response_extracts_reasoning_and_tool_call() {
        let parsed = parse_local_response(
            "<think>先思考一下</think>\n\nCALL_TOOL {\"name\":\"get_time\",\"arguments\":{}}",
        );

        assert_eq!(parsed.reasoning_content.as_deref(), Some("先思考一下"));
        assert!(parsed.content.is_empty());
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].function.name, "get_time");
        assert!(parsed.saw_tool_call_marker);
        assert!(parsed.parsed_tool_call);
    }

    #[test]
    fn parse_local_response_keeps_visible_content() {
        let parsed = parse_local_response("你好\n<think>内部推理</think>\n我是助手");

        assert_eq!(parsed.reasoning_content.as_deref(), Some("内部推理"));
        assert_eq!(parsed.content, "你好\n\n我是助手");
        assert!(parsed.tool_calls.is_empty());
    }

    #[test]
    fn stream_renderer_hides_tags_and_tool_protocol() {
        let mut renderer = LocalResponseStreamRenderer::default();
        let chunks = [
            "你好，",
            "<thi",
            "nk>先",
            "想一",
            "下</th",
            "ink>\nCALL",
            "_TOOL {\"name\":\"x\",\"arguments\":{}}",
        ];

        let mut output = Vec::new();
        for chunk in chunks {
            output.extend(renderer.push_piece(chunk));
        }
        output.extend(renderer.finish());

        let content = output
            .iter()
            .filter_map(|token| match token {
                StreamToken::Content(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();
        let thinking = output
            .iter()
            .filter_map(|token| match token {
                StreamToken::Thinking(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<String>();

        assert_eq!(content, "你好，");
        assert_eq!(thinking, "先想一下");
    }
}
