use serde::{Deserialize, Serialize};

pub const LLM_KIND_FIELD: &str = "llm_kind";
pub const LLM_KIND_MAIN: &str = "main";
pub const LLM_KIND_INTENT_CLASSIFICATION: &str = "intent_classification";
pub const LLM_KIND_MATH_PROGRAMMING: &str = "math_programming";
pub const LLM_KIND_NATURAL_LANGUAGE_REPLY: &str = "natural_language_reply";

pub mod agent;
pub mod brain_agent;
pub mod qq_chat;
pub mod runtime_context;
pub mod service_config;
pub mod sub_agent;
pub mod sub_agent_manager;
pub mod tool_config;

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

pub mod dream_agent;
pub mod inference_provider;
pub mod resource_resolver;
pub mod session_state;
mod shared_tool;
pub mod tool_definitions;
pub mod tools;
pub mod utils;

pub use crate::model_inference::llm::tooling::FunctionTool;
pub use agent::{Agent, AgentCancellation, AgentContext, AgentDescriptor};
pub use brain_agent::BrainAgent;
pub(crate) use shared_tool::SharedTool;
pub use sub_agent::{SubAgent, SubAgentDefinition, SubAgentTool};
pub use tools::{AgentExecutor, ToolCallingEngine, ToolCallingRequest, ToolCallingResult};
