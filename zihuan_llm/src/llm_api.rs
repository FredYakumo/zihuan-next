use log::{debug, error, warn};
use reqwest::blocking::Client;
use reqwest::StatusCode;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::error::Error as _;
use std::fmt::Write as _;
use std::thread;
use std::time::Duration;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::{ToolCalls, ToolCallsFuncSpec};
use zihuan_core::llm::{role_to_str, str_to_role, InferenceParam, MessageContent, OpenAIMessage};

const DEFAULT_RETRY_COUNT: u32 = 2;
const RETRY_DELAY_MS: u64 = 1_000;
const USER_VISIBLE_REQUEST_ERROR: &str = "Error: LLM API request failed";

#[derive(Default)]
struct StreamToolCallDelta {
    id: Option<String>,
    type_name: Option<String>,
    function_name: Option<String>,
    function_arguments: String,
}

enum RequestError {
    Retryable(String),
    NonRetryable(String),
}

#[derive(Debug, Clone)]
struct RequestContext {
    message_count: usize,
    tool_count: usize,
}

#[derive(Debug, Clone)]
pub struct LLMAPI {
    model_name: String,
    api_endpoint: String,
    api_key: Option<String>,
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
        stream: bool,
        supports_multimodal_input: bool,
        timeout: Duration,
    ) -> Self {
        Self {
            model_name,
            api_endpoint,
            api_key,
            stream,
            supports_multimodal_input,
            timeout,
            retry_count: DEFAULT_RETRY_COUNT,
        }
    }

    /// Set custom timeout for requests
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set max retry count for retryable request failures
    pub fn with_retry_count(mut self, retry_count: u32) -> Self {
        self.retry_count = retry_count;
        self
    }

    /// Create a system message
    pub fn system_message(content: &str) -> OpenAIMessage {
        OpenAIMessage::system(content)
    }

    /// Create a user message
    pub fn user_message(content: &str) -> OpenAIMessage {
        OpenAIMessage::user(content)
    }

    /// Parse tool calls from JSON array
    fn parse_tool_calls(tool_calls_value: &Value) -> Vec<ToolCalls> {
        tool_calls_value
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        let id = tc.get("id")?.as_str()?.to_string();
                        let type_name = tc.get("type")?.as_str()?.to_string();
                        let func = tc.get("function")?;
                        let name = func.get("name")?.as_str()?.to_string();

                        let arguments = func
                            .get("arguments")
                            .and_then(|args| {
                                if args.is_string() {
                                    args.as_str()
                                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                } else {
                                    Some(args.clone())
                                }
                            })
                            .unwrap_or(Value::Null);

                        Some(ToolCalls {
                            id,
                            type_name,
                            function: ToolCallsFuncSpec { name, arguments },
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parse_api_message(api_resp: &Value) -> Option<OpenAIMessage> {
        let choices = api_resp.get("choices")?.as_array()?;
        let choice = choices.first()?;
        let msg = choice.get("message")?;

        let role_str = msg.get("role")?.as_str().unwrap_or("assistant");
        let role = str_to_role(role_str);

        let content = msg
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| MessageContent::Text(s.to_string()));
        let reasoning_content = msg
            .get("reasoning_content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let tool_calls = msg
            .get("tool_calls")
            .map(|tc| Self::parse_tool_calls(tc))
            .unwrap_or_default();
        let tool_call_id = msg
            .get("tool_call_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        Some(OpenAIMessage {
            role,
            content,
            reasoning_content,
            tool_calls,
            tool_call_id,
        })
    }

    fn parse_sse_message(response_text: &str) -> Option<OpenAIMessage> {
        let mut role = None;
        let mut content = String::new();
        let mut reasoning_content = String::new();
        let mut streamed_tool_calls: BTreeMap<usize, StreamToolCallDelta> = BTreeMap::new();
        let mut final_tool_calls: Option<Vec<ToolCalls>> = None;

        for line in response_text.lines() {
            let line = line.trim();
            if !line.starts_with("data:") {
                continue;
            }

            let payload = line.trim_start_matches("data:").trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }

            let Ok(chunk) = serde_json::from_str::<Value>(payload) else {
                continue;
            };

            let choice = chunk
                .get("choices")
                .and_then(|value| value.as_array())
                .and_then(|arr| arr.first());
            let Some(choice) = choice else {
                continue;
            };

            if let Some(delta) = choice.get("delta") {
                if let Some(role_str) = delta.get("role").and_then(|value| value.as_str()) {
                    role = Some(str_to_role(role_str));
                }
                if let Some(piece) = delta.get("content").and_then(|value| value.as_str()) {
                    content.push_str(piece);
                }
                if let Some(piece) = delta
                    .get("reasoning_content")
                    .and_then(|value| value.as_str())
                {
                    reasoning_content.push_str(piece);
                }
                if let Some(tool_calls) = delta.get("tool_calls").and_then(|value| value.as_array())
                {
                    for tool_call in tool_calls {
                        let index = tool_call
                            .get("index")
                            .and_then(|value| value.as_u64())
                            .unwrap_or(streamed_tool_calls.len() as u64)
                            as usize;
                        let entry = streamed_tool_calls.entry(index).or_default();

                        if let Some(id) = tool_call.get("id").and_then(|value| value.as_str()) {
                            if !id.is_empty() {
                                entry.id = Some(id.to_string());
                            }
                        }
                        if let Some(type_name) =
                            tool_call.get("type").and_then(|value| value.as_str())
                        {
                            if !type_name.is_empty() {
                                entry.type_name = Some(type_name.to_string());
                            }
                        }

                        if let Some(function) = tool_call.get("function") {
                            if let Some(name) = function.get("name").and_then(|value| value.as_str())
                            {
                                if !name.is_empty() {
                                    entry.function_name = Some(name.to_string());
                                }
                            }
                            if let Some(arguments) =
                                function.get("arguments").and_then(|value| value.as_str())
                            {
                                entry.function_arguments.push_str(arguments);
                            }
                        }
                    }
                }
            } else if let Some(message) = choice.get("message") {
                if let Some(role_str) = message.get("role").and_then(|value| value.as_str()) {
                    role = Some(str_to_role(role_str));
                }
                if let Some(text) = message.get("content").and_then(|value| value.as_str()) {
                    content.push_str(text);
                }
                if let Some(text) = message
                    .get("reasoning_content")
                    .and_then(|value| value.as_str())
                {
                    reasoning_content.push_str(text);
                }
                if let Some(tool_calls_value) = message.get("tool_calls") {
                    let parsed = Self::parse_tool_calls(tool_calls_value);
                    if !parsed.is_empty() {
                        final_tool_calls = Some(parsed);
                    }
                }
            }
        }

        let tool_calls = if let Some(tool_calls) = final_tool_calls {
            tool_calls
        } else {
            streamed_tool_calls
                .into_iter()
                .map(|(index, call)| {
                    let arguments = if call.function_arguments.trim().is_empty() {
                        Value::Null
                    } else {
                        serde_json::from_str::<Value>(&call.function_arguments)
                            .unwrap_or_else(|_| Value::String(call.function_arguments.clone()))
                    };

                    ToolCalls {
                        id: call
                            .id
                            .unwrap_or_else(|| format!("stream_tool_call_{index}")),
                        type_name: call.type_name.unwrap_or_else(|| "function".to_string()),
                        function: ToolCallsFuncSpec {
                            name: call.function_name.unwrap_or_default(),
                            arguments,
                        },
                    }
                })
                .collect::<Vec<_>>()
        };

        if content.is_empty() && reasoning_content.is_empty() && tool_calls.is_empty() {
            return None;
        }

        Some(OpenAIMessage {
            role: role.unwrap_or_else(|| str_to_role("assistant")),
            content: if content.is_empty() {
                None
            } else {
                Some(MessageContent::Text(content))
            },
            reasoning_content: if reasoning_content.is_empty() {
                None
            } else {
                Some(reasoning_content)
            },
            tool_calls,
            tool_call_id: None,
        })
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
            "model={} endpoint={} timeout_secs={} messages={} tools={}",
            self.model_name,
            self.endpoint_label(),
            self.timeout.as_secs(),
            request_context.message_count,
            request_context.tool_count
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

    fn shorten_text(text: &str, limit: usize) -> String {
        if text.chars().count() <= limit {
            return text.to_string();
        }

        let truncated: String = text.chars().take(limit).collect();
        format!("{}...(truncated)", truncated)
    }

    fn send_request(
        &self,
        client: &Client,
        request_body: &Value,
        request_context: &RequestContext,
        attempt: u32,
        max_attempts: u32,
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
                self.format_request_context(request_context, Some((attempt, max_attempts))),
                Self::describe_reqwest_error(&e),
                e
            );
            RequestError::Retryable(err_detail)
        })?;
        let status = response.status();
        let response_text = response
            .text()
            .unwrap_or_else(|_| "Failed to read response".to_string());


        if self.stream {
            if let Some(message) = Self::parse_sse_message(&response_text) {
                return Ok(message);
            }
        }
        if !status.is_success() {
            let err_msg = format!(
                "{} status={} body={}",
                self.format_request_context(request_context, Some((attempt, max_attempts))),
                status,
                Self::shorten_text(&response_text, 800)
            );
            return if Self::should_retry_status(status) {
                Err(RequestError::Retryable(err_msg))
            } else {
                Err(RequestError::NonRetryable(err_msg))
            };
        }

        let api_resp = serde_json::from_str::<Value>(&response_text).map_err(|e| {
            RequestError::NonRetryable(format!(
                "{} parse_error={} body={}",
                self.format_request_context(request_context, Some((attempt, max_attempts))),
                e,
                Self::shorten_text(&response_text, 800)
            ))
        })?;

        Self::parse_api_message(&api_resp).ok_or_else(|| {
            RequestError::NonRetryable(format!(
                "{} invalid_response choices_present={} body={}",
                self.format_request_context(request_context, Some((attempt, max_attempts))),
                api_resp.get("choices").is_some(),
                Self::shorten_text(&response_text, 800)
            ))
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

    fn inference(&self, param: &InferenceParam) -> OpenAIMessage {
        let client = Client::builder()
            .timeout(self.timeout)
            .build()
            .expect("Failed to create HTTP client");

        // Convert internal MessageRole enum to string
        let messages: Vec<serde_json::Value> = param
            .messages
            .iter()
            .map(|msg| {
                let role_str = role_to_str(&msg.role);

                let content_value = msg
                    .content
                    .as_ref()
                    .map(|c| serde_json::to_value(c).unwrap_or(Value::Null))
                    .unwrap_or(Value::Null);

                let mut msg_obj = json!({
                    "role": role_str,
                    "content": content_value,
                });

                if let Some(reasoning_content) = &msg.reasoning_content {
                    msg_obj["reasoning_content"] = json!(reasoning_content);
                }

                // Add tool_calls if present
                if !msg.tool_calls.is_empty() {
                    let tool_calls: Vec<_> = msg
                        .tool_calls
                        .iter()
                        .map(|tc| {
                            json!({
                                "id": tc.id,
                                "type": tc.type_name,
                                "function": {
                                    "name": tc.function.name,
                                    "arguments": tc.function.arguments.to_string(),
                                }
                            })
                        })
                        .collect();
                    msg_obj["tool_calls"] = json!(tool_calls);
                }

                if let Some(ref id) = msg.tool_call_id {
                    msg_obj["tool_call_id"] = json!(id);
                }

                msg_obj
            })
            .collect();

        // Build tools array if provided
        let tools: Option<Vec<Value>> = param
            .tools
            .as_ref()
            .map(|ts| ts.iter().map(|tool| tool.get_json()).collect());

        let mut request_body = json!({
            "model": self.model_name,
            "messages": messages,
            "stream": self.stream,
        });

        if let Some(tool_list) = tools {
            request_body["tools"] = json!(tool_list);
            request_body["tool_choice"] = json!("auto");
        }

        let request_context = RequestContext {
            message_count: param.messages.len(),
            tool_count: param.tools.as_ref().map(|tools| tools.len()).unwrap_or(0),
        };
        let max_attempts = self.retry_count.saturating_add(1);
        let mut last_error = None;

        for attempt in 1..=max_attempts {
            debug!(
                "Sending LLM API request: {}",
                self.format_request_context(&request_context, Some((attempt, max_attempts)))
            );

            match self.send_request(
                &client,
                &request_body,
                &request_context,
                attempt,
                max_attempts,
            ) {
                Ok(msg) => {
                    debug!(
                        "Successfully parsed API response: {}",
                        self.format_request_context(
                            &request_context,
                            Some((attempt, max_attempts))
                        )
                    );
                    return msg;
                }
                Err(RequestError::Retryable(err_msg)) => {
                    last_error = Some(err_msg.clone());

                    if attempt < max_attempts {
                        warn!(
                            "LLM API request failed on attempt {}/{} and will retry: {}",
                            attempt, max_attempts, err_msg
                        );
                        thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                    } else {
                        error!(
                            "LLM API request failed on attempt {}/{}: {}",
                            attempt, max_attempts, err_msg
                        );
                        break;
                    }
                }
                Err(RequestError::NonRetryable(err_msg)) => {
                    error!(
                        "LLM API request failed on attempt {}/{} without retry: {}",
                        attempt, max_attempts, err_msg
                    );
                    last_error = Some(err_msg);
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
