use crate::llm_message::convert::{
    build_chat_completions_request_body, build_responses_image_url_object_compat_request_body,
    build_responses_message_compat_request_body, build_responses_request_body,
    build_tencent_multimodal_chat_completions_request_body, has_multimodal_messages, parse_chat_completions_response,
    parse_chat_completions_sse_response, parse_chat_completions_sse_stream_response,
    parse_responses_image_url_object_compat_response, parse_responses_image_url_object_compat_sse_response,
    parse_responses_image_url_object_compat_sse_stream_response, parse_responses_message_compat_response,
    parse_responses_message_compat_sse_response, parse_responses_message_compat_sse_stream_response,
    parse_responses_response, parse_responses_sse_response, parse_responses_sse_stream_response,
};
use crate::system_config::{LlmApiStyle, ReasoningEffort, ThinkingType};
use log::{debug, error, warn};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde_json::Value;
use std::error::Error as _;
use std::fmt::Write as _;
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;
use zihuan_core::llm::llm_base::{LLMBase, StreamingLLMBase};
use zihuan_core::llm::{InferenceParam, LLMMessage, StreamToken};
use zihuan_core::utils::string_utils;

const DEFAULT_RETRY_COUNT: u32 = 2;
const RETRY_DELAY_MS: u64 = 1_000;
const USER_VISIBLE_REQUEST_ERROR: &str = "Error: LLM API request failed";

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
    pub timeout: Duration,
    retry_count: u32,
}

impl LLMAPI {
    fn format_cache_hit_rate(&self, cached_prompt_tokens: Option<usize>, prompt_tokens: Option<usize>) -> String {
        match (cached_prompt_tokens, prompt_tokens) {
            (Some(cached), Some(prompt)) if prompt > 0 => {
                format!("{:.2}%", (cached as f64 / prompt as f64) * 100.0)
            }
            _ => "unavailable".to_string(),
        }
    }

    fn log_usage(&self, request_context: &RequestContext, usage: &zihuan_core::llm::TokenUsage) {
        let prompt_tokens = usage.prompt_tokens.or_else(|| {
            usage
                .cached_prompt_tokens
                .zip(usage.prompt_cache_miss_tokens)
                .map(|(hit, miss)| hit + miss)
        });
        log::info!(
            "[LLMAPI] usage model={} endpoint={} api_style={:?} format={} messages={} tools={} multimodal={} include_reasoning_content={} thinking_type={} reasoning_effort={} prompt_tokens={} cached_prompt_tokens={} prompt_cache_miss_tokens={} completion_tokens={} total_tokens={} cache_hit_rate={}",
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
            timeout,
            retry_count: DEFAULT_RETRY_COUNT,
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

    fn format_request_context(&self, request_context: &RequestContext, attempt: Option<(u32, u32)>) -> String {
        let mut context = format!(
            "model={} endpoint={} api_style={:?} format={} timeout_secs={} messages={} tools={} multimodal={} include_reasoning_content={} thinking_type={} reasoning_effort={}",
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
        );

        if let Some((current, total)) = attempt {
            let _ = write!(context, " attempt={}/{}", current, total);
        }

        context
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
            LlmApiStyle::CandleGguf => "candle_gguf",
            LlmApiStyle::CandleHf => "candle_hf",
            LlmApiStyle::OpenAiChatCompletions => "open_ai_chat_completions",
            LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat => {
                "open_ai_chat_completions_tencent_multimodal_compat"
            }
            LlmApiStyle::OpenAiResponses => "open_ai_responses",
            LlmApiStyle::OpenAiResponsesMessageCompat => "open_ai_responses_message_compat",
            LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => "open_ai_responses_image_url_object_compat",
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
        let response_text = response.text().unwrap_or_else(|_| "Failed to read response".to_string());

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

        let api_resp = serde_json::from_str::<Value>(&response_text).map_err(|e| RequestError::NonRetryable {
            message: format!(
                "{} parse_error={} body={}",
                self.format_request_context(request_context, Some((attempt, max_attempts)),),
                e,
                string_utils::shorten_text(&response_text, 800)
            ),
        })?;

        let parsed_message = match self.uses_responses_api() {
            true => match self.api_style {
                LlmApiStyle::OpenAiResponses => parse_responses_response(&api_resp),
                LlmApiStyle::OpenAiResponsesMessageCompat => parse_responses_message_compat_response(&api_resp),
                LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                    parse_responses_image_url_object_compat_response(&api_resp)
                }
                _ => unreachable!("non-responses style reached responses parser"),
            },
            _ => parse_chat_completions_response(&api_resp),
        };
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

    fn api_style(&self) -> Option<&str> {
        Some(self.api_style_label())
    }

    fn supports_multimodal_input(&self) -> bool {
        self.supports_multimodal_input
    }

    fn as_streaming(&self) -> Option<&dyn StreamingLLMBase> {
        Some(self)
    }

    fn inference(&self, param: &InferenceParam) -> LLMMessage {
        if matches!(self.api_style, LlmApiStyle::CandleGguf | LlmApiStyle::CandleHf) {
            error!("Local Candle styles should be routed through the local runtime, not LLMAPI");
            return LLMMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR);
        }

        let client = Client::builder()
            .timeout(self.timeout)
            .build()
            .expect("Failed to create HTTP client");

        let request_context = RequestContext {
            message_count: param.messages.len(),
            tool_count: param.tools.as_ref().map(|tools| tools.len()).unwrap_or(0),
            has_multimodal_input: has_multimodal_messages(param.messages),
        };
        let request_body = if self.uses_responses_api() {
            match self.api_style {
                LlmApiStyle::OpenAiResponses => {
                    build_responses_request_body(&self.model_name, param, self.stream, self.include_reasoning_content)
                }
                LlmApiStyle::OpenAiResponsesMessageCompat => build_responses_message_compat_request_body(
                    &self.model_name,
                    param,
                    self.stream,
                    self.include_reasoning_content,
                ),
                LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                    build_responses_image_url_object_compat_request_body(
                        &self.model_name,
                        param,
                        self.stream,
                        self.include_reasoning_content,
                    )
                }
                _ => unreachable!("non-responses style reached responses request builder"),
            }
        } else if matches!(self.api_style, LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat) {
            build_tencent_multimodal_chat_completions_request_body(
                &self.model_name,
                param,
                self.stream,
                self.include_reasoning_content,
                self.thinking_type.as_ref(),
                self.reasoning_effort.as_ref(),
            )
        } else {
            build_chat_completions_request_body(
                &self.model_name,
                param,
                self.stream,
                self.include_reasoning_content,
                self.thinking_type.as_ref(),
                self.reasoning_effort.as_ref(),
            )
        };
        let max_attempts = self.retry_count.saturating_add(1);
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            debug!(
                "Sending LLM API request: {}",
                self.format_request_context(&request_context, Some((attempt, max_attempts)),)
            );

            match self.send_request(&client, &request_body, &request_context, attempt, max_attempts) {
                Ok(msg) => {
                    if let Some(usage) = msg.usage.as_ref() {
                        self.log_usage(&request_context, usage);
                    }
                    debug!(
                        "Successfully parsed API response: {}",
                        self.format_request_context(&request_context, Some((attempt, max_attempts)),)
                    );
                    return msg;
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
                        error!("LLM API request failed on attempt {}/{}: {}", attempt, max_attempts, message);
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

        if let Some(err_msg) = last_error {
            error!(
                "Returning sanitized LLM API error to caller; detailed error kept in logs: {}",
                err_msg
            );
        } else {
            error!("Returning sanitized LLM API error to caller without detailed context");
        }

        LLMMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR)
    }
}

impl LLMAPI {
    pub async fn inference_streaming(
        &self,
        param: &InferenceParam<'_>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
    ) -> LLMMessage {
        if matches!(self.api_style, LlmApiStyle::CandleGguf | LlmApiStyle::CandleHf) {
            error!("Local Candle styles should be routed through the local runtime, not LLMAPI");
            return LLMMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR);
        }

        let request_context = RequestContext {
            message_count: param.messages.len(),
            tool_count: param.tools.as_ref().map(|tools| tools.len()).unwrap_or(0),
            has_multimodal_input: has_multimodal_messages(param.messages),
        };
        let request_body = if self.uses_responses_api() {
            match self.api_style {
                LlmApiStyle::OpenAiResponses => {
                    build_responses_request_body(&self.model_name, param, true, self.include_reasoning_content)
                }
                LlmApiStyle::OpenAiResponsesMessageCompat => build_responses_message_compat_request_body(
                    &self.model_name,
                    param,
                    true,
                    self.include_reasoning_content,
                ),
                LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                    build_responses_image_url_object_compat_request_body(
                        &self.model_name,
                        param,
                        true,
                        self.include_reasoning_content,
                    )
                }
                _ => unreachable!("non-responses style reached responses request builder"),
            }
        } else if matches!(self.api_style, LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat) {
            build_tencent_multimodal_chat_completions_request_body(
                &self.model_name,
                param,
                true,
                self.include_reasoning_content,
                self.thinking_type.as_ref(),
                self.reasoning_effort.as_ref(),
            )
        } else {
            build_chat_completions_request_body(
                &self.model_name,
                param,
                true,
                self.include_reasoning_content,
                self.thinking_type.as_ref(),
                self.reasoning_effort.as_ref(),
            )
        };

        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .expect("Failed to create async HTTP client");

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
                error!("Streaming LLM API request failed: {e}");
                return LLMMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR);
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            error!(
                "Streaming LLM API request failed: {} status={} body={}",
                self.format_request_context(&request_context, None),
                status,
                string_utils::shorten_text(&body, 800)
            );
            return LLMMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR);
        }

        let message = match self.uses_responses_api() {
            true => match self.api_style {
                LlmApiStyle::OpenAiResponses => parse_responses_sse_stream_response(response, token_tx).await,
                LlmApiStyle::OpenAiResponsesMessageCompat => {
                    parse_responses_message_compat_sse_stream_response(response, token_tx).await
                }
                LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                    parse_responses_image_url_object_compat_sse_stream_response(response, token_tx).await
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
