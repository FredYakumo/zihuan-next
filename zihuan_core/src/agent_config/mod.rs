use serde::{Deserialize, Serialize};

pub const LLM_KIND_FIELD: &str = "llm_kind";
pub const LLM_KIND_MAIN: &str = "main";
pub const LLM_KIND_INTENT_CLASSIFICATION: &str = "intent_classification";
pub const LLM_KIND_MATH_PROGRAMMING: &str = "math_programming";
pub const LLM_KIND_NATURAL_LANGUAGE_REPLY: &str = "natural_language_reply";

pub mod qq_chat;

pub fn normalize_llm_kind(llm_kind: Option<&str>) -> crate::error::Result<&'static str> {
    match llm_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(LLM_KIND_MAIN)
    {
        LLM_KIND_MAIN => Ok(LLM_KIND_MAIN),
        LLM_KIND_INTENT_CLASSIFICATION => Ok(LLM_KIND_INTENT_CLASSIFICATION),
        LLM_KIND_MATH_PROGRAMMING => Ok(LLM_KIND_MATH_PROGRAMMING),
        LLM_KIND_NATURAL_LANGUAGE_REPLY => Ok(LLM_KIND_NATURAL_LANGUAGE_REPLY),
        other => Err(crate::error::Error::ValidationError(format!(
            "unsupported llm_kind '{}', expected one of: {}, {}, {}, {}",
            other,
            LLM_KIND_MAIN,
            LLM_KIND_INTENT_CLASSIFICATION,
            LLM_KIND_MATH_PROGRAMMING,
            LLM_KIND_NATURAL_LANGUAGE_REPLY
        ))),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingServiceConfig {
    pub model_name: String,
    pub api_endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_retry_count() -> u32 {
    2
}
