use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmApiStyle {
    #[serde(alias = "open_ai_chat_completions_api")]
    OpenAiChatCompletions,
    OpenAiChatCompletionsTencentMultimodalCompat,
    #[serde(alias = "open_ai_responses_api")]
    OpenAiResponses,
    OpenAiResponsesMessageCompat,
    OpenAiResponsesImageUrlObjectCompat,
}

/// How a locally executed chat model obtains its outputs.
///
/// `Mock` replays locally recorded conversations (the recording directory is
/// carried in `api_endpoint`); `Gguf` / `Hf` run real inference over local
/// weight files with Candle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelWeightStyle {
    Mock,
    Gguf,
    Hf,
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
    #[serde(skip)]
    pub usage_config_id: Option<String>,
    pub model_name: String,
    pub api_endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_llm_api_style")]
    pub api_style: LlmApiStyle,
    /// Weight/execution style for locally executed chat models
    /// (`ModelRefSpec::ChatLlmLocal`); `None` for API models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight_style: Option<ModelWeightStyle>,
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
    #[serde(
        default = "default_context_length",
        deserialize_with = "deserialize_context_length"
    )]
    pub context_length: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelRefSpec {
    ChatLlmLocal { llm: LlmServiceConfig },
    ChatLlmApi { llm: LlmServiceConfig },
    TextEmbeddingLocal { model_name: String },
    TextEmbeddingApi {
        model_name: String,
        api_endpoint: String,
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default = "default_timeout_secs")]
        timeout_secs: u64,
        #[serde(default = "default_retry_count")]
        retry_count: u32,
    },
}

impl ModelRefSpec {
    /// Parses a spec from JSON with backward compatibility for the legacy
    /// formats: the pre-split `"type": "chat_llm"` tag and the even older
    /// bare-`LlmServiceConfig` encoding. Legacy chat specs are dispatched by
    /// their `api_style`: candle values become `chat_llm_local` (with the
    /// weight style carried over and `api_style` reset to the default), and
    /// `mock` becomes `chat_llm_local` with `ModelWeightStyle::Mock` (the
    /// recording directory in `api_endpoint` is preserved). Everything else
    /// becomes `chat_llm_api`.
    pub fn from_json(value: Value) -> Result<Self, serde_json::Error> {
        let object = match value.as_object() {
            Some(object) => object.clone(),
            None => {
                return Ok(ModelRefSpec::ChatLlmApi {
                    llm: serde_json::from_value(value)?,
                });
            }
        };

        match object.get("type").and_then(Value::as_str) {
            Some("chat_llm_local") | Some("chat_llm_api") => {
                return serde_json::from_value(Value::Object(object));
            }
            Some("text_embedding_local") | Some("text_embedding_api") => {
                return serde_json::from_value(Value::Object(object));
            }
            Some("chat_llm") | None => {}
            Some(other) => {
                return Err(serde::de::Error::custom(format!(
                    "unknown model ref type '{other}'"
                )));
            }
        }

        // Legacy chat spec: decide local vs api by the recorded api_style.
        let llm_value = object
            .get("llm")
            .cloned()
            .unwrap_or_else(|| Value::Object(object));
        let api_style = llm_value
            .get("api_style")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let mut llm: LlmServiceConfig = serde_json::from_value(llm_value)?;
        match api_style.as_str() {
            "candle" | "candle_gguf" => {
                llm.api_style = default_llm_api_style();
                llm.weight_style = Some(ModelWeightStyle::Gguf);
                Ok(ModelRefSpec::ChatLlmLocal { llm })
            }
            "candle_hf" => {
                llm.api_style = default_llm_api_style();
                llm.weight_style = Some(ModelWeightStyle::Hf);
                Ok(ModelRefSpec::ChatLlmLocal { llm })
            }
            "mock" => {
                llm.api_style = default_llm_api_style();
                llm.weight_style = Some(ModelWeightStyle::Mock);
                Ok(ModelRefSpec::ChatLlmLocal { llm })
            }
            _ => Ok(ModelRefSpec::ChatLlmApi { llm }),
        }
    }
}

// The derived `Serialize` above is tag-based; deserialization is handwritten
// so the legacy encodings keep loading (see `ModelRefSpec::from_json`).
impl<'de> Deserialize<'de> for ModelRefSpec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        ModelRefSpec::from_json(value).map_err(serde::de::Error::custom)
    }
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

pub const DEFAULT_CONTEXT_LENGTH: usize = 32 * 1024;

fn default_context_length() -> usize {
    DEFAULT_CONTEXT_LENGTH
}

fn deserialize_context_length<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<usize>::deserialize(deserializer)?.unwrap_or(DEFAULT_CONTEXT_LENGTH))
}

#[cfg(test)]
mod tests {
    use super::{LlmServiceConfig, ModelRefSpec, ModelWeightStyle, DEFAULT_CONTEXT_LENGTH};

    #[test]
    fn context_length_defaults_for_missing_and_null_values() {
        let missing: LlmServiceConfig = serde_json::from_value(serde_json::json!({
            "model_name": "model",
            "api_endpoint": "https://example.test"
        }))
        .unwrap();
        let null_value: LlmServiceConfig = serde_json::from_value(serde_json::json!({
            "model_name": "model",
            "api_endpoint": "https://example.test",
            "context_length": null
        }))
        .unwrap();

        assert_eq!(missing.context_length, DEFAULT_CONTEXT_LENGTH);
        assert_eq!(null_value.context_length, DEFAULT_CONTEXT_LENGTH);
    }

    #[test]
    fn legacy_chat_llm_specs_dispatch_by_api_style() {
        let gguf: ModelRefSpec = serde_json::from_value(serde_json::json!({
            "type": "chat_llm",
            "llm": { "model_name": "m", "api_endpoint": "", "api_style": "candle_gguf" }
        }))
        .unwrap();
        assert!(matches!(gguf, ModelRefSpec::ChatLlmLocal { .. }));

        let api: ModelRefSpec = serde_json::from_value(serde_json::json!({
            "type": "chat_llm",
            "llm": { "model_name": "m", "api_endpoint": "https://example.test" }
        }))
        .unwrap();
        assert!(matches!(api, ModelRefSpec::ChatLlmApi { .. }));

        let mock: ModelRefSpec = serde_json::from_value(serde_json::json!({
            "type": "chat_llm",
            "llm": { "model_name": "m", "api_endpoint": "recordings/x", "api_style": "mock" }
        }))
        .unwrap();
        match mock {
            ModelRefSpec::ChatLlmLocal { llm } => {
                assert_eq!(llm.weight_style, Some(ModelWeightStyle::Mock));
                assert_eq!(llm.api_endpoint, "recordings/x");
            }
            _ => panic!("expected chat_llm_local"),
        }
    }
}
