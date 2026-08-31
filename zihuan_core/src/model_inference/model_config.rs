use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmApiStyle {
    #[serde(alias = "candle")]
    CandleGguf,
    CandleHf,
    #[serde(alias = "open_ai_chat_completions_api")]
    OpenAiChatCompletions,
    OpenAiChatCompletionsTencentMultimodalCompat,
    #[serde(alias = "open_ai_responses_api")]
    OpenAiResponses,
    OpenAiResponsesMessageCompat,
    OpenAiResponsesImageUrlObjectCompat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingType {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
    Max,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmServiceConfig {
    pub model_name: String,
    pub api_endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_llm_api_style")]
    pub api_style: LlmApiStyle,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub supports_multimodal_input: bool,
    #[serde(default)]
    pub include_reasoning_content: bool,
    #[serde(default)]
    pub thinking_type: Option<ThinkingType>,
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(default)]
    pub context_length: Option<usize>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelRefSpec {
    ChatLlm { llm: LlmServiceConfig },
    TextEmbeddingLocal { model_name: String },
}

fn default_llm_api_style() -> LlmApiStyle {
    LlmApiStyle::OpenAiChatCompletions
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_retry_count() -> u32 {
    2
}
