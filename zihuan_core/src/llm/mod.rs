pub mod embedding_base;
pub mod llm_base;
pub mod model;
pub mod tooling;
pub mod util;

pub use crate::message_part::MessagePart;
pub use llm_base::StreamingLLMBase;
pub use model::{InferenceParam, LLMMessage, LLMMessageConvertStyle, MessageRole, TokenUsage};
pub use util::{role_to_str, str_to_role, SystemMessage, UserMessage};

/// Token streamed from LLM inference, tagged with its kind so the relay can
/// emit distinct SSE events for thinking vs. content.
#[derive(Debug, Clone)]
pub enum StreamToken {
    /// Regular assistant content delta.
    Content(String),
    /// Thinking / reasoning content delta (e.g. DeepSeek-R1, Qwen thinking).
    Thinking(String),
}

impl StreamToken {
    pub fn content(text: impl Into<String>) -> Self {
        Self::Content(text.into())
    }

    pub fn thinking(text: impl Into<String>) -> Self {
        Self::Thinking(text.into())
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Content(s) | Self::Thinking(s) => s.as_str(),
        }
    }
}
