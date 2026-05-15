use crate::system_config::LlmApiStyle;
use crate::utils::openai_message_util::{
    build_request_body, has_multimodal_messages, parse_chat_completions_api_message,
    parse_chat_completions_sse_message, parse_chat_completions_sse_stream,
    parse_responses_api_message, parse_responses_sse_message, parse_responses_sse_stream,
    OpenAIRequestFormat as RequestFormat,
};
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
use zihuan_core::llm::{InferenceParam, OpenAIMessage};
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
    pub timeout: Duration,
    retry_count: u32,
}

impl LLMAPI {
    pub fn new(
        model_name: String,
        api_endpoint: String,
        api_key: Option<String>,
        api_style: LlmApiStyle,
        stream: bool,
        supports_multimodal_input: bool,
        timeout: Duration,
    ) -> Self {
        Self {
            model_name,
            api_endpoint,
            api_key,
            api_style,
            stream,
            supports_multimodal_input,
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

    pub fn system_message(content: &str) -> OpenAIMessage {
        OpenAIMessage::system(content)
    }

    pub fn user_message(content: &str) -> OpenAIMessage {
        OpenAIMessage::user(content)
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
        request_format: RequestFormat,
    ) -> String {
        let mut context = format!(
            "model={} endpoint={} api_style={:?} format={} timeout_secs={} messages={} tools={} multimodal={}",
            self.model_name,
            self.endpoint_label(),
            self.api_style,
            request_format.label(),
            self.timeout.as_secs(),
            request_context.message_count,
            request_context.tool_count,
            request_context.has_multimodal_input
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

    fn request_format(&self) -> RequestFormat {
        match self.api_style {
            LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat => {
                RequestFormat::TencentMultimodalCompat
            }
            LlmApiStyle::OpenAiResponsesMessageCompat => {
                RequestFormat::OpenAiResponsesMessageCompat
            }
            LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
                RequestFormat::OpenAiResponsesImageUrlObjectCompat
            }
            _ => RequestFormat::DefaultOpenAI,
        }
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
        request_format: RequestFormat,
    ) -> Result<OpenAIMessage, RequestError> {
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
                self.format_request_context(
                    request_context,
                    Some((attempt, max_attempts)),
                    request_format,
                ),
                Self::describe_reqwest_error(&e),
                e
            );
            RequestError::Retryable {
                message: err_detail,
            }
        })?;
        let status = response.status();
        let response_text = response
            .text()
            .unwrap_or_else(|_| "Failed to read response".to_string());

        if self.stream {
            if let Some(message) = match self.uses_responses_api() {
                true => parse_responses_sse_message(&response_text),
                _ => parse_chat_completions_sse_message(&response_text),
            } {
                return Ok(message);
            }
        }
        if !status.is_success() {
            let err_msg = format!(
                "{} status={} body={}",
                self.format_request_context(
                    request_context,
                    Some((attempt, max_attempts)),
                    request_format,
                ),
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
                    self.format_request_context(
                        request_context,
                        Some((attempt, max_attempts)),
                        request_format,
                    ),
                    e,
                    string_utils::shorten_text(&response_text, 800)
                ),
            }
        })?;

        let parsed_message = match self.uses_responses_api() {
            true => parse_responses_api_message(&api_resp),
            _ => parse_chat_completions_api_message(&api_resp),
        };
        parsed_message.ok_or_else(|| RequestError::NonRetryable {
            message: format!(
                "{} invalid_response choices_present={} body={}",
                self.format_request_context(
                    request_context,
                    Some((attempt, max_attempts)),
                    request_format,
                ),
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

    fn supports_multimodal_input(&self) -> bool {
        self.supports_multimodal_input
    }

    fn as_streaming(&self) -> Option<&dyn StreamingLLMBase> {
        Some(self)
    }

    fn inference(&self, param: &InferenceParam) -> OpenAIMessage {
        if matches!(self.api_style, LlmApiStyle::Candle) {
            error!("Candle chat backend is not implemented yet");
            return OpenAIMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR);
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
        let request_format = self.request_format();
        let request_body = build_request_body(
            &self.model_name,
            &self.api_style,
            param,
            self.stream,
            request_format,
        );
        let max_attempts = self.retry_count.saturating_add(1);
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            debug!(
                "Sending LLM API request: {}",
                self.format_request_context(
                    &request_context,
                    Some((attempt, max_attempts)),
                    request_format,
                )
            );

            match self.send_request(
                &client,
                &request_body,
                &request_context,
                attempt,
                max_attempts,
                request_format,
            ) {
                Ok(msg) => {
                    debug!(
                        "Successfully parsed API response: {}",
                        self.format_request_context(
                            &request_context,
                            Some((attempt, max_attempts)),
                            request_format,
                        )
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

        if let Some(err_msg) = last_error {
            error!(
                "Returning sanitized LLM API error to caller; detailed error kept in logs: {}",
                err_msg
            );
        } else {
            error!("Returning sanitized LLM API error to caller without detailed context");
        }

        OpenAIMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR)
    }
}

impl LLMAPI {
    pub async fn inference_streaming(
        &self,
        param: &InferenceParam<'_>,
        token_tx: mpsc::UnboundedSender<String>,
    ) -> OpenAIMessage {
        if matches!(self.api_style, LlmApiStyle::Candle) {
            error!("Candle chat backend is not implemented yet");
            return OpenAIMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR);
        }

        let request_context = RequestContext {
            message_count: param.messages.len(),
            tool_count: param.tools.as_ref().map(|tools| tools.len()).unwrap_or(0),
            has_multimodal_input: has_multimodal_messages(param.messages),
        };
        let request_format = self.request_format();
        let request_body = build_request_body(
            &self.model_name,
            &self.api_style,
            param,
            true,
            request_format,
        );

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
                return OpenAIMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR);
            }
        };

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            error!(
                "Streaming LLM API request failed: {} status={} body={}",
                self.format_request_context(&request_context, None, request_format),
                status,
                string_utils::shorten_text(&body, 800)
            );
            return OpenAIMessage::assistant_text(USER_VISIBLE_REQUEST_ERROR);
        }

        match self.uses_responses_api() {
            true => parse_responses_sse_stream(response, token_tx).await,
            _ => parse_chat_completions_sse_stream(response, token_tx).await,
        }
    }
}

impl StreamingLLMBase for LLMAPI {
    fn inference_streaming<'a>(
        &'a self,
        param: &'a InferenceParam<'a>,
        token_tx: mpsc::UnboundedSender<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = OpenAIMessage> + Send + 'a>> {
        Box::pin(async move { self.inference_streaming(param, token_tx).await })
    }
}
