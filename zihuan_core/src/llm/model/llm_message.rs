use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::tooling::ToolCalls;
use crate::message_part::MessagePart;

use super::message_role::MessageRole;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_prompt_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_miss_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMMessage {
    pub role: MessageRole,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<MessagePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCalls>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LLMMessageConvertStyle {
    OpenAiChatCompletions,
    OpenAiChatCompletionsTencentMultimodalCompat,
    OpenAiResponses,
    OpenAiResponsesMessageCompat,
    OpenAiResponsesImageUrlObjectCompat,
}

impl LLMMessage {
    /// Construct a system-role message with a single text part.
    pub fn system<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::System,
            parts: vec![MessagePart::text(content)],
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            usage: None,
        }
    }

    /// Construct a user-role message with a single text part.
    pub fn user<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::User,
            parts: vec![MessagePart::text(content)],
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            usage: None,
        }
    }

    /// Construct a user-role message from pre-built multimodal parts.
    pub fn user_with_parts(parts: Vec<MessagePart>) -> Self {
        Self {
            role: MessageRole::User,
            parts,
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            usage: None,
        }
    }

    /// Construct an assistant-role message with a single text part.
    pub fn assistant_text<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::Assistant,
            parts: vec![MessagePart::text(content)],
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            usage: None,
        }
    }

    /// Construct a tool-role result message linked to a prior tool call id.
    pub fn tool_result<I: Into<String>, C: Into<String>>(tool_call_id: I, content: C) -> Self {
        Self {
            role: MessageRole::Tool,
            parts: vec![MessagePart::text(content)],
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
            usage: None,
        }
    }

    /// Return borrowed text only when the message is exactly one text part.
    pub fn content_text(&self) -> Option<&str> {
        match self.parts.as_slice() {
            [MessagePart::Text { text }] => Some(text.as_str()),
            _ => None,
        }
    }

    /// Join every text part into a single owned string, ignoring non-text media parts.
    pub fn content_text_owned(&self) -> Option<String> {
        let combined = self
            .parts
            .iter()
            .filter_map(|part| match part {
                MessagePart::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        if combined.is_empty() {
            None
        } else {
            Some(combined)
        }
    }

    /// Internal helper for providers that need all text parts merged into one string.
    pub(crate) fn text_parts_joined(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| match part {
                MessagePart::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Internal helper to detect whether a message can be serialized as pure text.
    pub(crate) fn has_only_text_parts(&self) -> bool {
        self.parts.iter().all(|part| matches!(part, MessagePart::Text { .. }))
    }

    /// Dispatch this message to the concrete provider/style-specific payload converter.
    pub fn convert(&self, style: LLMMessageConvertStyle, include_reasoning_content: bool) -> Vec<Value> {
        match style {
            LLMMessageConvertStyle::OpenAiChatCompletions => {
                super::convert::openai_chat_completions::convert(self, include_reasoning_content)
            }
            LLMMessageConvertStyle::OpenAiChatCompletionsTencentMultimodalCompat => {
                super::convert::openai_chat_completions_tencent_multimodal_compat::convert(
                    self,
                    include_reasoning_content,
                )
            }
            LLMMessageConvertStyle::OpenAiResponses => super::convert::openai_responses::convert(self),
            LLMMessageConvertStyle::OpenAiResponsesMessageCompat => {
                super::convert::openai_responses_message_compat::convert(self)
            }
            LLMMessageConvertStyle::OpenAiResponsesImageUrlObjectCompat => {
                super::convert::openai_responses_image_url_object_compat::convert(self)
            }
        }
    }

    /// Convert a whole message list by delegating each item to `convert` and
    /// flattening styles that expand one logical message into multiple payload items.
    pub fn convert_list(
        messages: &[LLMMessage],
        style: LLMMessageConvertStyle,
        include_reasoning_content: bool,
    ) -> Vec<Value> {
        messages
            .iter()
            .flat_map(|message| message.convert(style, include_reasoning_content))
            .collect()
    }
}
