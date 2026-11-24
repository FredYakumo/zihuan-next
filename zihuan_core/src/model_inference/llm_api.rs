use crate::error::Error;
use crate::model_inference::llm::llm_base::{LLMBase, StreamingLLMBase};
use crate::model_inference::llm::message::convert::{
    build_chat_completions_request_body, build_responses_image_url_object_compat_request_body,
    build_responses_message_compat_request_body, build_responses_request_body,
    build_tencent_multimodal_chat_completions_request_body, has_multimodal_messages,
    parse_chat_completions_response, parse_chat_completions_sse_response,
    parse_chat_completions_sse_stream_response, parse_responses_image_url_object_compat_response,
    parse_responses_image_url_object_compat_sse_response,
    parse_responses_image_url_object_compat_sse_stream_response,
    parse_responses_message_compat_response, parse_responses_message_compat_sse_response,
    parse_responses_message_compat_sse_stream_response, parse_responses_response,
    parse_responses_sse_response, parse_responses_sse_stream_response,
};
use crate::model_inference::llm::{InferenceParam, LLMMessage, StreamToken};
use crate::model_inference::message_content_utils::{
    downgrade_messages_for_model, normalize_system_messages,
};
use crate::model_inference::model_config::{
    LlmApiStyle, LlmProxyConfig, ReasoningEffort, ReasoningParamStyle, ThinkingType,
};
use crate::utils::string_utils;
use log::{debug, error, warn};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde_json::Value;
use std::error::Error as _;
use std::fmt::Write as _;
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;

const DEFAULT_RETRY_COUNT: u32 = 2;
const RETRY_DELAY_MS: u64 = 1_000;
const USER_VISIBLE_REQUEST_ERROR: &str = "Error: LLM API request failed";
/// Error-log caps: request bodies can embed base64 media, provider error bodies can echo a
/// long payload. Both dumps keep the message-role sequence visible, which is what matters when
/// diagnosing layout rejections such as "System message must be at the beginning.".
const REQUEST_BODY_LOG_MAX_CHARS: usize = 65_536;
const RESPONSE_BODY_LOG_MAX_CHARS: usize = 4_000;

/// Builds the reqwest proxy from the model's proxy config. Returns `None`
/// (with a warning) for an unusable proxy so the request falls back to the
/// direct connection instead of failing client construction.
pub(crate) fn build_reqwest_proxy(proxy: Option<&LlmProxyConfig>) -> Option<reqwest::Proxy> {
    let proxy = proxy?;
    let proxy_url = match proxy.to_proxy_url() {
        Ok(proxy_url) => proxy_url,
        Err(error) => {
            warn!(
                "[LLMAPI] invalid proxy config (kind={:?}, host='{}', port={}): {error}; connecting directly",
                proxy.kind, proxy.host, proxy.port
            );
            return None;
        }
    };
    match reqwest::Proxy::all(proxy_url) {
        Ok(reqwest_proxy) => Some(reqwest_proxy),
        Err(error) => {
            warn!(
                "[LLMAPI] invalid proxy config (kind={:?}, host='{}', port={}): {error}; connecting directly",
                proxy.kind, proxy.host, proxy.port
            );
            None
        }
    }
}

enum RequestError {
    Retryable { message: String },
    NonRetryable { message: String },
}

#[derive(Debug, Clone)]
struct RequestContext {
    message_count: usize,
    tool_count: usize,
    has_multimodal_input: bool,
}

#[derive(Debug, Clone)]
pub struct LLMAPI {
    model_name: String,
    api_endpoint: String,
    api_key: Option<String>,
    api_style: LlmApiStyle,
    stream: bool,
    supports_multimodal_input: bool,
    include_reasoning_content: bool,
    thinking_type: Option<ThinkingType>,
    reasoning_effort: Option<ReasoningEffort>,
    reasoning_param_style: ReasoningParamStyle,
    context_length: usize,
    pub timeout: Duration,
    retry_count: u32,
    proxy: Option<LlmProxyConfig>,
}

impl LLMAPI {
    fn format_cache_hit_rate(
        &self,
        cached_prompt_tokens: Option<usize>,
        prompt_tokens: Option<usize>,
    ) -> String {
        match (cached_prompt_tokens, prompt_tokens) {
            (Some(cached), Some(prompt)) if prompt > 0 => {
                format!("{:.2}%", (cached as f64 / prompt as f64) * 100.0)
            }
            _ => "unavailable".to_string(),
        }
    }

    fn log_usage(
        &self,
        request_context: &RequestContext,
        usage: &crate::model_inference::llm::TokenUsage,
    ) {
        let prompt_tokens = usage.prompt_tokens.or_else(|| {
            usage
                .cached_prompt_tokens
                .zip(usage.prompt_cache_miss_tokens)
                .map(|(hit, miss)| hit + miss)
        });
        log::info!(
            "[LLMAPI] usage model={} endpoint={} api_style={:?} format={} messages={} tools={} multimodal={} include_reasoning_content={} thinking_type={} reasoning_effort={} reasoning_param_style={} prompt_tokens={} cached_prompt_tokens={} prompt_cache_miss_tokens={} completion_tokens={} total_tokens={} cache_hit_rate={}",
            self.model_name,
            self.endpoint_label(),
            self.api_style,
            self.api_style_label(),
            request_context.message_count,
            request_context.tool_count,
            request_context.has_multimodal_input,
            self.include_reasoning_content,
            self.thinking_type.as_ref().map(|t| format!("{:?}", t)).unwrap_or_else(|| "none".to_string()),
            self.reasoning_effort.as_ref().map(|e| format!("{:?}", e)).unwrap_or_else(|| "none".to_string()),
            format!("{:?}", self.reasoning_param_style),
            usage
                .prompt_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            usage
                .cached_prompt_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            usage
                .prompt_cache_miss_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            usage
                .completion_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            usage
                .total_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            self.format_cache_hit_rate(usage.cached_prompt_tokens, prompt_tokens),
        );
    }

    pub fn new(
        model_name: String,
        api_endpoint: String,
        api_key: Option<String>,
        api_style: LlmApiStyle,
        stream: bool,
        supports_multimodal_input: bool,
        include_reasoning_content: bool,
        thinking_type: Option<ThinkingType>,
        reasoning_effort: Option<ReasoningEffort>,
        reasoning_param_style: ReasoningParamStyle,
        context_length: usize,
        timeout: Duration,
    ) -> Self {
        Self {
            model_name,
            api_endpoint,
            api_key,
            api_style,
            stream,
            supports_multimodal_input,
            include_reasoning_content,
            thinking_type,
            reasoning_effort,
            reasoning_param_style,
            context_length,
            timeout,
            retry_count: DEFAULT_RETRY_COUNT,
            proxy: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_retry_count(mut self, retry_count: u32) -> Self {
        self.retry_count = retry_count;
        self
    }

    pub fn with_proxy(mut self, proxy: Option<LlmProxyConfig>) -> Self {
        self.proxy = proxy;
        self
    }

    pub fn system_message(content: &str) -> LLMMessage {
        LLMMessage::system(content)
    }

    pub fn user_message(content: &str) -> LLMMessage {
        LLMMessage::user(content)
    }

    fn should_retry_status(status: StatusCode) -> bool {
        status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
    }

    fn endpoint_label(&self) -> &str {
        &self.api_endpoint
    }

    fn format_request_context(
        &self,
        request_context: &RequestContext,
        attempt: Option<(u32, u32)>,
    ) -> String {
        let mut context = format!(
            "model={} endpoint={} api_style={:?} format={} timeout_secs={} messages={} tools={} multimodal={} include_reasoning_content={} thinking_type={} reasoning_effort={} reasoning_param_style={}",
            self.model_name,
            self.endpoint_label(),
            self.api_style,
            self.api_style_label(),
            self.timeout.as_secs(),
            request_context.message_count,
            request_context.tool_count,
            request_context.has_multimodal_input,
            self.include_reasoning_content,
            self.thinking_type.as_ref().map(|t| format!("{:?}", t)).unwrap_or_else(|| "none".to_string()),
            self.reasoning_effort.as_ref().map(|e| format!("{:?}", e)).unwrap_or_else(|| "none".to_string()),
            format!("{:?}", self.reasoning_param_style),
        );

        if let Some((current, total)) = attempt {
            let _ = write!(context, " attempt={}/{}", current, total);
        }

        context
    }

    /// Serializes the outgoing request body for ERROR logs so a failed request can be
    /// diagnosed without reproducing it. Base64 media and oversized tool output would bloat the
    /// log, so those strings are replaced by a preview while the JSON structure, message roles,
    /// system prompts, and tool definitions stay intact.
    fn format_request_body_for_log(&self, request_body: &Value) -> String {
        let mut sanitized = request_body.clone();
        string_utils::elide_large_strings(&mut sanitized);
        match serde_json::to_string(&sanitized) {
            Ok(serialized) => string_utils::shorten_text(&serialized, REQUEST_BODY_LOG_MAX_CHARS),
            Err(error) => format!("<failed to serialize request body: {error}>"),
        }
    }

    fn describe_reqwest_error(error: &reqwest::Error) -> String {
        let mut tags = Vec::new();

        if error.is_timeout() {
            tags.push("timeout");
        }
        if error.is_connect() {
            tags.push("connect");
        }
        if error.is_request() {
            tags.push("request");
        }
        if error.is_body() {
            tags.push("body");
        }
        if error.is_decode() {
            tags.push("decode");
        }

        let mut description = if tags.is_empty() {
            "kind=unknown".to_string()
        } else {
            format!("kind={}", tags.join("|"))
        };

        if let Some(url) = error.url() {
            let _ = write!(description, " url={}", url);
        }

        let mut source = error.source();
        while let Some(cause) = source {
            let _ = write!(description, " cause={}", cause);
            source = cause.source();
        }

        description
    }

    fn api_style_label(&self) -> &'static str {
        match self.api_style {
            LlmApiStyle::OpenAiChatCompletions => "open_ai_chat_completions",
            LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat => {
                "open_ai_chat_completions_tencent_multimodal_compat"
            }
            LlmApiStyle::OpenAiResponses => "open_ai_responses",
            LlmApiStyle::OpenAiResponsesMessageCompat => "open_ai_responses_message_compat",
            LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                "open_ai_responses_image_url_object_compat"
            }
        }
    }

    fn tag_response_api_style(&self, message: LLMMessage) -> LLMMessage {
        message
    }

    fn uses_responses_api(&self) -> bool {
        matches!(
            self.api_style,
            LlmApiStyle::OpenAiResponses
                | LlmApiStyle::OpenAiResponsesMessageCompat
                | LlmApiStyle::OpenAiResponsesImageUrlObjectCompat
        )
    }

    fn send_request(
        &self,
        client: &Client,
        request_body: &Value,
        request_context: &RequestContext,
        attempt: u32,
        max_attempts: u32,
    ) -> Result<LLMMessage, RequestError> {
        let mut request = client.post(&self.api_endpoint).json(request_body);

        if let Some(ref api_key) = self.api_key {
            let auth_header = if api_key.starts_with("Bearer ") {
                api_key.to_string()
            } else {
                format!("Bearer {}", api_key)
            };
            request = request.header("Authorization", auth_header);
        }

        let response = request.send().map_err(|e| {
            let err_detail = format!(
                "{} detail={} message={}",
                self.format_request_context(request_context, Some((attempt, max_attempts)),),
                Self::describe_reqwest_error(&e),
                e
            );
            RequestError::Retryable { message: err_detail }
        })?;
        let status = response.status();
        // Read the body incrementally so a timeout or broken connection while
        // reading it does not discard whatever the model already returned; the
        // partial content is logged for diagnostics in the error below.
        let mut response = response;
        let mut body_bytes: Vec<u8> = Vec::new();
        let response_text = response
            .copy_to(&mut body_bytes)
            .map(|_| String::from_utf8_lossy(&body_bytes).into_owned())
            .map_err(|error| {
                let partial_body = string_utils::shorten_text(&String::from_utf8_lossy(&body_bytes), 800);
                let message = format!(
                    "{} status={} response_body_read_error detail={} message={} bytes_read={} partial_body={}",
                    self.format_request_context(request_context, Some((attempt, max_attempts))),
                    status,
                    Self::describe_reqwest_error(&error),
                    error,
                    body_bytes.len(),
                    partial_body,
                );
                // Headers can arrive before a timeout or a broken connection while reading
                // the body. Treat that as a transport failure, not malformed model JSON.
                if status.is_success() || Self::should_retry_status(status) {
                    RequestError::Retryable { message }
                } else {
                    RequestError::NonRetryable { message }
                }
            })?;

        if self.stream {
            if let Some(message) = match self.uses_responses_api() {
                true => match self.api_style {
                    LlmApiStyle::OpenAiResponses => parse_responses_sse_response(&response_text),
                    LlmApiStyle::OpenAiResponsesMessageCompat => {
                        parse_responses_message_compat_sse_response(&response_text)
                    }
                    LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                        parse_responses_image_url_object_compat_sse_response(&response_text)
                    }
                    _ => unreachable!("non-responses style reached responses sse parser"),
                },
                _ => parse_chat_completions_sse_response(&response_text),
            } {
                if matches!(self.api_style, LlmApiStyle::OpenAiResponsesMessageCompat)
                    && message.parts.is_empty()
                    && message.tool_calls.is_empty()
                    && message
                        .reasoning_content
                        .as_deref()
                        .map(|content| content.trim().is_empty())
                        .unwrap_or(true)
                {
                    warn!(
                        "[LLMAPI] Responses message-compatible SSE response has no parsed content: model={} response={}",
                        self.model_name,
                        response_text,
                    );
                }
                return Ok(self.tag_response_api_style(message));
            }
        }
        if !status.is_success() {
            let err_msg = format!(
                "{} status={} body={}",
                self.format_request_context(request_context, Some((attempt, max_attempts)),),
                status,
                string_utils::shorten_text(&response_text, 800)
            );
            return if Self::should_retry_status(status) {
                Err(RequestError::Retryable { message: err_msg })
            } else {
                Err(RequestError::NonRetryable { message: err_msg })
            };
        }

        let api_resp = serde_json::from_str::<Value>(&response_text).map_err(|e| {
            RequestError::NonRetryable {
                message: format!(
                    "{} parse_error={} body={}",
                    self.format_request_context(request_context, Some((attempt, max_attempts)),),
                    e,
                    string_utils::shorten_text(&response_text, 800)
                ),
            }
        })?;

        let parsed_message = match self.uses_responses_api() {
            true => match self.api_style {
                LlmApiStyle::OpenAiResponses => parse_responses_response(&api_resp),
                LlmApiStyle::OpenAiResponsesMessageCompat => {
                    parse_responses_message_compat_response(&api_resp)
                }
                LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                    parse_responses_image_url_object_compat_response(&api_resp)
                }
                _ => unreachable!("non-responses style reached responses parser"),
            },
            _ => parse_chat_completions_response(&api_resp),
        };
        if matches!(self.api_style, LlmApiStyle::OpenAiResponsesMessageCompat)
            && parsed_message.as_ref().is_some_and(|message| {
                message.parts.is_empty()
                    && message.tool_calls.is_empty()
                    && message
                        .reasoning_content
                        .as_deref()
                        .map(|content| content.trim().is_empty())
                        .unwrap_or(true)
            })
        {
            warn!(
                "[LLMAPI] Responses message-compatible response has no parsed content: model={} output={}",
                self.model_name,
                api_resp.get("output").unwrap_or(&Value::Null),
            );
        }
        parsed_message
            .map(|message| self.tag_response_api_style(message))
            .ok_or_else(|| RequestError::NonRetryable {
                message: format!(
                    "{} invalid_response choices_present={} body={}",
                    self.format_request_context(request_context, Some((attempt, max_attempts)),),
                    api_resp.get("choices").is_some() || api_resp.get("output").is_some(),
                    string_utils::shorten_text(&response_text, 800)
                ),
            })
    }
}

impl LLMBase for LLMAPI {
    fn get_model_name(&self) -> &str {
        &self.model_name
    }

    fn context_length(&self) -> usize {
        self.context_length
    }

    fn api_style(&self) -> Option<&str> {
        Some(self.api_style_label())
    }

    fn supports_multimodal_input(&self) -> bool {
        self.supports_multimodal_input
    }

    fn with_thinking_disabled(&self) -> Option<std::sync::Arc<dyn LLMBase>> {
        if !self.reasoning_param_style.supports_thinking_toggle(&self.api_style) {
            return None;
        }
        let mut model = self.clone();
        model.thinking_type = Some(ThinkingType::Disabled);
        model.reasoning_effort = None;
        Some(std::sync::Arc::new(model))
    }

    fn as_streaming(&self) -> Option<&dyn StreamingLLMBase> {
        Some(self)
    }

    fn inference(&self, param: &InferenceParam) -> crate::error::Result<LLMMessage> {
        // `reqwest::blocking` internally creates and drops a Tokio runtime; that must not happen
        // while an async context is entered (QQ turns call this from inside `block_async`).
        crate::runtime::run_outside_async_runtime(|| self.inference_inner(param))
    }
}

impl LLMAPI {
    fn inference_inner(&self, param: &InferenceParam) -> crate::error::Result<LLMMessage> {
        let messages = normalize_system_messages(downgrade_messages_for_model(
            param.messages.clone(),
            self.supports_multimodal_input,
        ));
        let param = InferenceParam { messages: &messages, tools: param.tools };

        let mut client_builder = Client::builder().timeout(self.timeout);
        if let Some(reqwest_proxy) = build_reqwest_proxy(self.proxy.as_ref()) {
            client_builder = client_builder.proxy(reqwest_proxy);
        }
        let client = client_builder.build().expect("Failed to create HTTP client");

        let request_context = RequestContext {
            message_count: param.messages.len(),
            tool_count: param.tools.as_ref().map(|tools| tools.len()).unwrap_or(0),
            has_multimodal_input: has_multimodal_messages(param.messages),
        };
        let request_body = if self.uses_responses_api() {
            match self.api_style {
                LlmApiStyle::OpenAiResponses => build_responses_request_body(
                    &self.model_name,
                    &param,
                    self.stream,
                    self.include_reasoning_content,
                ),
                LlmApiStyle::OpenAiResponsesMessageCompat => {
                    build_responses_message_compat_request_body(
                        &self.model_name,
                        &param,
                        self.stream,
                        self.include_reasoning_content,
                    )
                }
                LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                    build_responses_image_url_object_compat_request_body(
                        &self.model_name,
                        &param,
                        self.stream,
                        self.include_reasoning_content,
                    )
                }
                _ => unreachable!("non-responses style reached responses request builder"),
            }
        } else if matches!(
            self.api_style,
            LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat
        ) {
            build_tencent_multimodal_chat_completions_request_body(
                &self.model_name,
                &param,
                self.stream,
                self.include_reasoning_content,
                self.thinking_type.as_ref(),
                self.reasoning_effort.as_ref(),
                self.reasoning_param_style,
            )
        } else {
            build_chat_completions_request_body(
                &self.model_name,
                &param,
                self.stream,
                self.include_reasoning_content,
                self.thinking_type.as_ref(),
                self.reasoning_effort.as_ref(),
                self.reasoning_param_style,
            )
        };
        let max_attempts = self.retry_count.saturating_add(1);
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            debug!(
                "Sending LLM API request: {}",
                self.format_request_context(&request_context, Some((attempt, max_attempts)),)
            );

            match self.send_request(&client, &request_body, &request_context, attempt, max_attempts)
            {
                Ok(msg) => {
                    if let Some(usage) = msg.usage.as_ref() {
                        self.log_usage(&request_context, usage);
                    }
                    debug!(
                        "Successfully parsed API response: {}",
                        self.format_request_context(
                            &request_context,
                            Some((attempt, max_attempts)),
                        )
                    );
                    return Ok(msg);
                }
                Err(RequestError::Retryable { message }) => {
                    last_error = Some(message.clone());

                    if attempt < max_attempts {
                        warn!(
                            "LLM API request failed on attempt {}/{} and will retry: {}",
                            attempt, max_attempts, message
                        );
                        thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                    } else {
                        error!(
                            "LLM API request failed on attempt {}/{}: {}",
                            attempt, max_attempts, message
                        );
                    }
                }
                Err(RequestError::NonRetryable { message }) => {
                    error!(
                        "LLM API request failed on attempt {}/{} without retry: {}",
                        attempt, max_attempts, message
                    );
                    last_error = Some(message);
                    break;
                }
            }
        }

        match last_error {
            Some(err_msg) => {
                error!(
                    "LLM API request failed; returning error to caller: {err_msg} request={}",
                    self.format_request_body_for_log(&request_body)
                );
                Err(Error::StringError(err_msg))
            }
            None => {
                error!(
                    "LLM API request failed without detailed context request={}",
                    self.format_request_body_for_log(&request_body)
                );
                Err(Error::StaticStrError("LLM API request failed"))
            }
        }
    }
}

impl LLMAPI {
    pub async fn inference_streaming(
        &self,
        param: &InferenceParam<'_>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
    ) -> LLMMessage {
        let messages = normalize_system_messages(downgrade_messages_for_model(
            param.messages.clone(),
            self.supports_multimodal_input,
        ));
        let param = InferenceParam { messages: &messages, tools: param.tools };

        let request_context = RequestContext {
            message_count: param.messages.len(),
            tool_count: param.tools.as_ref().map(|tools| tools.len()).unwrap_or(0),
            has_multimodal_input: has_multimodal_messages(param.messages),
        };
        let request_body = if self.uses_responses_api() {
            match self.api_style {
                LlmApiStyle::OpenAiResponses => build_responses_request_body(
                    &self.model_name,
                    &param,
                    true,
                    self.include_reasoning_content,
                ),
                LlmApiStyle::OpenAiResponsesMessageCompat => {
                    build_responses_message_compat_request_body(
                        &self.model_name,
                        &param,
                        true,
                        self.include_reasoning_content,
                    )
                }
                LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                    build_responses_image_url_object_compat_request_body(
                        &self.model_name,
                        &param,
                        true,
                        self.include_reasoning_content,
                    )
                }
                _ => unreachable!("non-responses style reached responses request builder"),
            }
        } else if matches!(
            self.api_style,
            LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat
        ) {
            build_tencent_multimodal_chat_completions_request_body(
                &self.model_name,
                &param,
                true,
                self.include_reasoning_content,
                self.thinking_type.as_ref(),
                self.reasoning_effort.as_ref(),
                self.reasoning_param_style,
            )
        } else {
            build_chat_completions_request_body(
                &self.model_name,
                &param,
                true,
                self.include_reasoning_content,
                self.thinking_type.as_ref(),
                self.reasoning_effort.as_ref(),
                self.reasoning_param_style,
            )
        };

        let mut client_builder = reqwest::Client::builder().timeout(self.timeout);
        if let Some(reqwest_proxy) = build_reqwest_proxy(self.proxy.as_ref()) {
            client_builder = client_builder.proxy(reqwest_proxy);
        }
        let client = client_builder.build().expect("Failed to create async HTTP client");

        let mut request = client.post(&self.api_endpoint).json(&request_body);
        if let Some(ref api_key) = self.api_key {
            let auth_header = if api_key.starts_with("Bearer ") {
                api_key.to_string()
            } else {
                format!("Bearer {}", api_key)
            };
            request = request.header("Authorization", auth_header);
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Streaming LLM API request failed: {} detail={} message={} request={}",
                    self.format_request_context(&request_context, None),
                    Self::describe_reqwest_error(&e),
                    e,
                    self.format_request_body_for_log(&request_body)
                );
                return LLMMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR);
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            error!(
                "Streaming LLM API request failed: {} status={} body={} request={}",
                self.format_request_context(&request_context, None),
                status,
                string_utils::shorten_text(&body, RESPONSE_BODY_LOG_MAX_CHARS),
                self.format_request_body_for_log(&request_body)
            );
            let provider_message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|value| {
                    value
                        .pointer("/error/message")
                        .and_then(|message| message.as_str())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| USER_VISIBLE_REQUEST_ERROR.to_string());
            return LLMMessage::assistant_text(format!(
                "Error: LLM API request failed ({status}): {provider_message}"
            ));
        }

        let message = match self.uses_responses_api() {
            true => match self.api_style {
                LlmApiStyle::OpenAiResponses => {
                    parse_responses_sse_stream_response(response, token_tx).await
                }
                LlmApiStyle::OpenAiResponsesMessageCompat => {
                    parse_responses_message_compat_sse_stream_response(response, token_tx).await
                }
                LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                    parse_responses_image_url_object_compat_sse_stream_response(response, token_tx)
                        .await
                }
                _ => unreachable!("non-responses style reached responses streaming parser"),
            },
            _ => parse_chat_completions_sse_stream_response(response, token_tx).await,
        };
        let message = self.tag_response_api_style(message);
        if let Some(usage) = message.usage.as_ref() {
            self.log_usage(&request_context, usage);
        }
        message
    }
}

impl StreamingLLMBase for LLMAPI {
    fn inference_streaming<'a>(
        &'a self,
        param: &'a InferenceParam<'a>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LLMMessage> + Send + 'a>> {
        Box::pin(async move { self.inference_streaming(param, token_tx).await })
    }
}
