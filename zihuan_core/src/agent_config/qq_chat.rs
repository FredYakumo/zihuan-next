use std::cell::RefCell;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    EmbeddingServiceConfig, LLM_KIND_INTENT_CLASSIFICATION, LLM_KIND_MAIN, LLM_KIND_MATH_PROGRAMMING,
    LLM_KIND_NATURAL_LANGUAGE_REPLY,
};
use crate::error::{Error, Result};

thread_local! {
    static CURRENT_QQ_CHAT_AGENT_SERVICE_CONFIG: RefCell<Vec<QqChatAgentServiceConfig>> =
        const { RefCell::new(Vec::new()) };
}

pub fn with_current_qq_chat_agent_service_config<T>(config: QqChatAgentServiceConfig, f: impl FnOnce() -> T) -> T {
    CURRENT_QQ_CHAT_AGENT_SERVICE_CONFIG.with(|slot| {
        slot.borrow_mut().push(config);
    });
    let result = f();
    CURRENT_QQ_CHAT_AGENT_SERVICE_CONFIG.with(|slot| {
        slot.borrow_mut().pop();
    });
    result
}

pub fn current_qq_chat_agent_service_config() -> Result<QqChatAgentServiceConfig> {
    CURRENT_QQ_CHAT_AGENT_SERVICE_CONFIG.with(|slot| {
        slot.borrow().last().cloned().ok_or_else(|| {
            Error::ValidationError("当前节点不在 Agent 工具调用上下文中，无法读取 Agent 配置".to_string())
        })
    })
}

pub fn llm_ref_id_for_kind<'a>(config: &'a QqChatAgentServiceConfig, llm_kind: &str) -> Option<&'a str> {
    match llm_kind {
        LLM_KIND_MAIN => config.llm_ref_id.as_deref(),
        LLM_KIND_INTENT_CLASSIFICATION => config
            .intent_classification_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        LLM_KIND_MATH_PROGRAMMING => config.math_programming_llm_ref_id.as_deref().or(config.llm_ref_id.as_deref()),
        LLM_KIND_NATURAL_LANGUAGE_REPLY => config.natural_language_reply_llm_ref_id.as_deref(),
        _ => None,
    }
}

pub fn image_understand_llm_ref_id<'a>(config: &'a QqChatAgentServiceConfig) -> Option<&'a str> {
    config.image_understand_llm_ref_id.as_deref().or(config.llm_ref_id.as_deref())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatEmotionDimensionConfig {
    pub name: String,
    #[serde(default = "default_emotion_adjust_weight")]
    pub increase_weight: f64,
    #[serde(default = "default_emotion_adjust_weight")]
    pub decrease_weight: f64,
    #[serde(default)]
    pub positive_prompt: Option<String>,
    #[serde(default)]
    pub negative_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QqChatMessageRateLimitWindowUnit {
    Minute,
    Hour,
    Day,
}

impl QqChatMessageRateLimitWindowUnit {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Hour => "hour",
            Self::Day => "day",
        }
    }

    pub fn window_seconds(&self) -> i64 {
        match self {
            Self::Minute => 60,
            Self::Hour => 60 * 60,
            Self::Day => 60 * 60 * 24,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatMessageRateLimitRule {
    #[serde(default)]
    pub unlimited: bool,
    #[serde(default)]
    pub window_unit: Option<QqChatMessageRateLimitWindowUnit>,
    #[serde(default)]
    pub max_calls: Option<usize>,
    #[serde(default = "default_message_rate_limit_window_size")]
    pub window_size: i64,
}

impl QqChatMessageRateLimitRule {
    pub fn is_effectively_unlimited(&self) -> bool {
        self.unlimited
    }

    pub fn sanitized(&self) -> Option<Self> {
        if self.unlimited {
            return Some(Self {
                unlimited: true,
                window_unit: None,
                max_calls: None,
                window_size: 1,
            });
        }

        let window_unit = self.window_unit?;
        let max_calls = self.max_calls.filter(|value| *value > 0)?;
        let window_size = if self.window_size > 0 { self.window_size } else { 1 };
        Some(Self {
            unlimited: false,
            window_unit: Some(window_unit),
            max_calls: Some(max_calls),
            window_size,
        })
    }

    /// Total window length in seconds: `window_unit` seconds multiplied by `window_size`.
    pub fn window_seconds(&self) -> Option<i64> {
        self.window_unit
            .map(|unit| unit.window_seconds() * self.window_size.max(1))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatMessageRateLimitGroupRule {
    pub group_id: String,
    #[serde(flatten)]
    pub limit: QqChatMessageRateLimitRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatMessageRateLimitUserRule {
    pub sender_id: String,
    #[serde(flatten)]
    pub limit: QqChatMessageRateLimitRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatAgentServiceConfig {
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
    pub image_understand_llm_ref_id: Option<String>,
    #[serde(default)]
    pub intent_classification_llm_ref_id: Option<String>,
    #[serde(default)]
    pub math_programming_llm_ref_id: Option<String>,
    #[serde(default)]
    pub natural_language_reply_llm_ref_id: Option<String>,
    #[serde(default)]
    pub natural_language_reply_system_prompt: Option<String>,
    #[serde(default)]
    pub embedding_model_ref_id: Option<String>,
    #[serde(default)]
    pub tokenizer_connection_id: Option<String>,
    pub web_search_engine_connection_id: String,
    #[serde(default)]
    pub rdb_id: Option<String>,
    #[serde(default)]
    pub embedding: Option<EmbeddingServiceConfig>,
    #[serde(default)]
    #[serde(skip_serializing)]
    pub mysql_connection_id: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing)]
    pub task_db_connection_id: Option<String>,
    #[serde(default)]
    pub weaviate_image_connection_id: Option<String>,
    #[serde(default)]
    pub weaviate_memory_connection_id: Option<String>,
    #[serde(default = "default_max_message_length")]
    pub max_message_length: usize,
    #[serde(default)]
    pub compact_context_length: usize,
    #[serde(default = "default_max_steer_count")]
    pub max_steer_count: usize,
    #[serde(default = "default_qq_chat_default_tools_enabled")]
    pub default_tools_enabled: HashMap<String, bool>,
    #[serde(default = "default_qq_chat_tool_session_call_limits")]
    pub tool_session_call_limits: HashMap<String, usize>,
    #[serde(default)]
    pub tool_session_limit_message: Option<String>,
    #[serde(default)]
    pub message_rate_limit_default: Option<QqChatMessageRateLimitRule>,
    #[serde(default)]
    pub message_rate_limit_groups: Vec<QqChatMessageRateLimitGroupRule>,
    #[serde(default)]
    pub message_rate_limit_users: Vec<QqChatMessageRateLimitUserRule>,
    #[serde(default = "default_qq_chat_emotion_dimensions")]
    pub emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    #[serde(default)]
    pub event_handler_threads: Option<usize>,
}

impl QqChatAgentServiceConfig {
    pub fn resolved_rdb_id(&self) -> Option<&str> {
        self.rdb_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                self.mysql_connection_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| {
                self.task_db_connection_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
    }

    pub fn resolved_emotion_dimensions(&self) -> Vec<QqChatEmotionDimensionConfig> {
        let mut dimensions = Vec::new();
        for dimension in &self.emotion_dimensions {
            let name = dimension.name.trim();
            if name.is_empty()
                || dimensions
                    .iter()
                    .any(|existing: &QqChatEmotionDimensionConfig| existing.name == name)
            {
                continue;
            }
            dimensions.push(QqChatEmotionDimensionConfig {
                name: name.to_string(),
                increase_weight: sanitize_emotion_adjust_weight(dimension.increase_weight),
                decrease_weight: sanitize_emotion_adjust_weight(dimension.decrease_weight),
                positive_prompt: dimension.positive_prompt.clone(),
                negative_prompt: dimension.negative_prompt.clone(),
            });
        }
        if dimensions.is_empty() {
            return default_qq_chat_emotion_dimensions();
        }
        dimensions
    }

    pub fn resolved_message_rate_limit_default(&self) -> Option<QqChatMessageRateLimitRule> {
        self.message_rate_limit_default.as_ref()?.sanitized()
    }

    pub fn resolved_message_rate_limit_groups(&self) -> Vec<QqChatMessageRateLimitGroupRule> {
        let mut rules = Vec::new();
        for rule in &self.message_rate_limit_groups {
            let group_id = rule.group_id.trim();
            let Some(limit) = rule.limit.sanitized() else {
                continue;
            };
            if group_id.is_empty()
                || rules
                    .iter()
                    .any(|existing: &QqChatMessageRateLimitGroupRule| existing.group_id == group_id)
            {
                continue;
            }
            rules.push(QqChatMessageRateLimitGroupRule {
                group_id: group_id.to_string(),
                limit,
            });
        }
        rules
    }

    pub fn resolved_message_rate_limit_users(&self) -> Vec<QqChatMessageRateLimitUserRule> {
        let mut rules = Vec::new();
        for rule in &self.message_rate_limit_users {
            let sender_id = rule.sender_id.trim();
            let Some(limit) = rule.limit.sanitized() else {
                continue;
            };
            if sender_id.is_empty()
                || rules
                    .iter()
                    .any(|existing: &QqChatMessageRateLimitUserRule| existing.sender_id == sender_id)
            {
                continue;
            }
            rules.push(QqChatMessageRateLimitUserRule {
                sender_id: sender_id.to_string(),
                limit,
            });
        }
        rules
    }
}

fn default_max_message_length() -> usize {
    500
}

fn default_max_steer_count() -> usize {
    4
}

fn default_message_rate_limit_window_size() -> i64 {
    1
}

fn default_qq_chat_default_tools_enabled() -> HashMap<String, bool> {
    [
        "web_search",
        "get_agent_public_info",
        "get_function_list",
        "get_recent_group_messages",
        "get_recent_user_messages",
        "search_similar_images",
        "save_image",
        "image_understand",
        "list_available_memory_keys",
        "search_memory_content",
        "remember_content",
    ]
    .into_iter()
    .map(|name| (name.to_string(), true))
    .collect()
}

fn default_qq_chat_tool_session_call_limits() -> HashMap<String, usize> {
    [("web_search".to_string(), 1usize)].into_iter().collect()
}

fn default_qq_chat_emotion_dimensions() -> Vec<QqChatEmotionDimensionConfig> {
    ["开心", "烦恼", "生气", "伤心", "害怕", "焦虑", "激动"]
        .into_iter()
        .map(|name| QqChatEmotionDimensionConfig {
            name: name.to_string(),
            increase_weight: default_emotion_adjust_weight(),
            decrease_weight: default_emotion_adjust_weight(),
            positive_prompt: None,
            negative_prompt: None,
        })
        .collect()
}

fn default_emotion_adjust_weight() -> f64 {
    1.0
}

fn sanitize_emotion_adjust_weight(weight: f64) -> f64 {
    if weight.is_finite() && weight > 0.0 {
        weight
    } else {
        default_emotion_adjust_weight()
    }
}

#[cfg(test)]
mod tests {
    use super::QqChatAgentServiceConfig;

    #[test]
    fn qq_chat_agent_service_config_defaults_max_steer_count_to_four() {
        let config: QqChatAgentServiceConfig = serde_json::from_value(serde_json::json!({
            "ims_bot_adapter_connection_id": "bot",
            "web_search_engine_connection_id": "tavily"
        }))
        .expect("config should deserialize");

        assert_eq!(config.max_steer_count, 4);
    }
}
