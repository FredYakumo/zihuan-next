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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProxyKind {
    Http,
    Socks5,
}

/// Optional outbound proxy for API models (`ModelRefSpec::ChatLlmApi` /
/// `ModelRefSpec::TextEmbeddingApi`). `None` / absent field means the request
/// goes out directly (reqwest still honors `HTTP_PROXY`-style env vars).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProxyConfig {
    #[serde(default = "default_proxy_kind")]
    pub kind: LlmProxyKind,
    pub host: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl LlmProxyConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.host.trim().is_empty() {
            return Err("proxy host must not be empty".into());
        }
        if self.port == 0 {
            return Err("proxy port must be greater than zero".into());
        }
        Ok(())
    }

    /// Builds the reqwest proxy URL, e.g. `http://host:port` or
    /// `socks5://user:pass@host:port`. Credentials go into the URL userinfo,
    /// percent-encoded through `url::Url` so special characters survive.
    pub fn to_proxy_url(&self) -> Result<String, String> {
        let scheme = match self.kind {
            LlmProxyKind::Http => "http",
            LlmProxyKind::Socks5 => "socks5",
        };
        let host = self.host.trim();
        let mut url = url::Url::parse(&format!("{scheme}://{host}:{}", self.port))
            .map_err(|e| format!("invalid proxy host '{host}': {e}"))?;
        let username = self.username.as_deref().map(str::trim).unwrap_or("");
        if !username.is_empty() {
            let _ = url.set_username(username);
            let password = self.password.as_deref().unwrap_or("");
            if !password.is_empty() {
                let _ = url.set_password(Some(password));
            }
        }
        Ok(url.to_string())
    }
}

fn default_proxy_kind() -> LlmProxyKind {
    LlmProxyKind::Http
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningParamStyle {
    #[default]
    Thinking,
    ReasoningEnabled,
    Unsupported,
}

impl ReasoningParamStyle {
    pub fn supports_thinking_toggle(self, api_style: &LlmApiStyle) -> bool {
        self != Self::Unsupported
            && matches!(
                api_style,
                LlmApiStyle::OpenAiChatCompletions
                    | LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat
            )
    }
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
    #[serde(default)]
    pub reasoning_param_style: ReasoningParamStyle,
    #[serde(
        default = "default_context_length",
        deserialize_with = "deserialize_context_length"
    )]
    pub context_length: usize,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
    /// Outbound proxy for API models; `None` = connect directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy: Option<LlmProxyConfig>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelRefSpec {
    ChatLlmLocal {
        llm: LlmServiceConfig,
    },
    ChatLlmApi {
        llm: LlmServiceConfig,
    },
    TextEmbeddingLocal {
        model_name: String,
    },
    TextEmbeddingApi {
        model_name: String,
        api_endpoint: String,
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default = "default_timeout_secs")]
        timeout_secs: u64,
        #[serde(default = "default_retry_count")]
        retry_count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proxy: Option<LlmProxyConfig>,
    },
}

/// Deserialization-only mirror of [`ModelRefSpec`]'s tag-based encoding.
///
/// `ModelRefSpec` implements `Deserialize` by hand so `from_json` keeps
/// accepting the legacy encodings, and `from_json` must not deserialize into
/// `ModelRefSpec` itself: `serde_json::from_value::<ModelRefSpec>` would call
/// the `Deserialize` impl again, which calls `from_json`, recursing until the
/// stack overflows. This mirror gives the tagged branch a concrete target type
/// that does not route back through `from_json`.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TaggedModelRefSpec {
    ChatLlmLocal {
        llm: LlmServiceConfig,
    },
    ChatLlmApi {
        llm: LlmServiceConfig,
    },
    TextEmbeddingLocal {
        model_name: String,
    },
    TextEmbeddingApi {
        model_name: String,
        api_endpoint: String,
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default = "default_timeout_secs")]
        timeout_secs: u64,
        #[serde(default = "default_retry_count")]
        retry_count: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proxy: Option<LlmProxyConfig>,
    },
}

impl From<TaggedModelRefSpec> for ModelRefSpec {
    fn from(spec: TaggedModelRefSpec) -> Self {
        match spec {
            TaggedModelRefSpec::ChatLlmLocal { llm } => ModelRefSpec::ChatLlmLocal { llm },
            TaggedModelRefSpec::ChatLlmApi { llm } => ModelRefSpec::ChatLlmApi { llm },
            TaggedModelRefSpec::TextEmbeddingLocal { model_name } => {
                ModelRefSpec::TextEmbeddingLocal { model_name }
            }
            TaggedModelRefSpec::TextEmbeddingApi {
                model_name,
                api_endpoint,
                api_key,
                timeout_secs,
                retry_count,
                proxy,
            } => ModelRefSpec::TextEmbeddingApi {
                model_name,
                api_endpoint,
                api_key,
                timeout_secs,
                retry_count,
                proxy,
            },
        }
    }
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
                return Ok(ModelRefSpec::ChatLlmApi { llm: serde_json::from_value(value)? });
            }
        };

        match object.get("type").and_then(Value::as_str) {
            Some("chat_llm_local")
            | Some("chat_llm_api")
            | Some("text_embedding_local")
            | Some("text_embedding_api") => {
                return serde_json::from_value::<TaggedModelRefSpec>(Value::Object(object))
                    .map(Into::into);
            }
            Some("chat_llm") | None => {}
            Some(other) => {
                return Err(serde::de::Error::custom(format!("unknown model ref type '{other}'")));
            }
        }

        // Legacy chat spec: decide local vs api by the recorded api_style.
        let llm_value = object.get("llm").cloned().unwrap_or_else(|| Value::Object(object));
        let api_style =
            llm_value.get("api_style").and_then(Value::as_str).unwrap_or("").to_string();
        // Legacy candle/mock api_style values are no longer `LlmApiStyle`
        // variants; drop them so deserialization falls back to the default
        // api_style and the matching `ModelWeightStyle` is applied below.
        let llm_value = match api_style.as_str() {
            "candle" | "candle_gguf" | "candle_hf" | "mock" => {
                let mut llm_value = llm_value;
                if let Some(llm_object) = llm_value.as_object_mut() {
                    llm_object.remove("api_style");
                }
                llm_value
            }
            _ => llm_value,
        };
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
