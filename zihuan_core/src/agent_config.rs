use std::cell::RefCell;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const LLM_KIND_FIELD: &str = "llm_kind";
pub const LLM_KIND_MAIN: &str = "main";
pub const LLM_KIND_INTENT: &str = "intent";
pub const LLM_KIND_MATH_PROGRAMMING: &str = "math_programming";

thread_local! {
    static CURRENT_QQ_CHAT_AGENT_CONFIG: RefCell<Vec<QqChatAgentConfig>> = const { RefCell::new(Vec::new()) };
}

pub fn with_current_qq_chat_agent_config<T>(config: QqChatAgentConfig, f: impl FnOnce() -> T) -> T {
    CURRENT_QQ_CHAT_AGENT_CONFIG.with(|slot| {
        slot.borrow_mut().push(config);
    });
    let result = f();
    CURRENT_QQ_CHAT_AGENT_CONFIG.with(|slot| {
        slot.borrow_mut().pop();
    });
    result
}

pub fn current_qq_chat_agent_config() -> Result<QqChatAgentConfig> {
    CURRENT_QQ_CHAT_AGENT_CONFIG.with(|slot| {
        slot.borrow().last().cloned().ok_or_else(|| {
            Error::ValidationError(
                "当前节点不在 Agent 工具调用上下文中，无法读取 Agent 配置".to_string(),
            )
        })
    })
}

pub fn normalize_llm_kind(llm_kind: Option<&str>) -> Result<&'static str> {
    match llm_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(LLM_KIND_MAIN)
    {
        LLM_KIND_MAIN => Ok(LLM_KIND_MAIN),
        LLM_KIND_INTENT => Ok(LLM_KIND_INTENT),
        LLM_KIND_MATH_PROGRAMMING => Ok(LLM_KIND_MATH_PROGRAMMING),
        other => Err(Error::ValidationError(format!(
            "unsupported llm_kind '{}', expected one of: {}, {}, {}",
            other, LLM_KIND_MAIN, LLM_KIND_INTENT, LLM_KIND_MATH_PROGRAMMING
        ))),
    }
}

pub fn llm_ref_id_for_kind<'a>(config: &'a QqChatAgentConfig, llm_kind: &str) -> Option<&'a str> {
    match llm_kind {
        LLM_KIND_MAIN => config.llm_ref_id.as_deref(),
        LLM_KIND_INTENT => config
            .intent_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        LLM_KIND_MATH_PROGRAMMING => config
            .math_programming_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        _ => None,
    }
}

use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatAgentConfig {
    pub ims_bot_adapter_connection_id: String,
    #[serde(default)]
    pub rustfs_connection_id: Option<String>,
    #[serde(default)]
    pub bot_name: String,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub llm_ref_id: Option<String>,
    #[serde(default)]
    pub intent_llm_ref_id: Option<String>,
    #[serde(default)]
    pub math_programming_llm_ref_id: Option<String>,
    #[serde(default)]
    pub embedding_model_ref_id: Option<String>,
    pub tavily_connection_id: String,
    #[serde(default)]
    pub embedding: Option<EmbeddingServiceConfig>,
    #[serde(default)]
    pub mysql_connection_id: Option<String>,
    #[serde(default)]
    pub weaviate_image_connection_id: Option<String>,
    #[serde(default = "default_max_message_length")]
    pub max_message_length: usize,
    #[serde(default)]
    pub compact_context_length: usize,
    #[serde(default = "default_max_steer_count")]
    pub max_steer_count: usize,
    #[serde(default = "default_qq_chat_default_tools_enabled")]
    pub default_tools_enabled: HashMap<String, bool>,
    #[serde(default)]
    pub event_handler_threads: Option<usize>,
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

fn default_max_message_length() -> usize {
    500
}

fn default_max_steer_count() -> usize {
    4
}

fn default_qq_chat_default_tools_enabled() -> HashMap<String, bool> {
    [
        "web_search",
        "get_agent_public_info",
        "get_function_list",
        "get_recent_group_messages",
        "get_recent_user_messages",
        "search_similar_images",
    ]
    .into_iter()
    .map(|name| (name.to_string(), true))
    .collect()
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_retry_count() -> u32 {
    2
}

#[cfg(test)]
mod tests {
    use super::QqChatAgentConfig;

    #[test]
    fn qq_chat_agent_config_defaults_max_steer_count_to_four() {
        let config: QqChatAgentConfig = serde_json::from_value(serde_json::json!({
            "ims_bot_adapter_connection_id": "bot",
            "tavily_connection_id": "tavily"
        }))
        .expect("config should deserialize");

        assert_eq!(config.max_steer_count, 4);
    }
}
