use log::{debug, error};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration;
use zihuan_llm_types::llm_base::LLMBase;
use zihuan_llm_types::tooling::{ToolCalls, ToolCallsFuncSpec};
use zihuan_llm_types::{
    role_to_str, str_to_role, InferenceParam, MessageContent, OpenAIMessage,
};

#[derive(Debug, Clone)]
pub struct LLMAPI {
    model_name: String,
    api_endpoint: String,
    api_key: Option<String>,
    pub timeout: Duration,
}

impl LLMAPI {
    pub fn new(
        model_name: String,
        api_endpoint: String,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            model_name,
            api_endpoint,
            api_key,
            timeout,
        }
    }

    /// Set custom timeout for requests
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
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
}

impl LLMBase for LLMAPI {
    fn get_model_name(&self) -> &str {
        &self.model_name
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

                // Add tool_call_id for tool result messages
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
        });

        if let Some(tool_list) = tools {
            request_body["tools"] = json!(tool_list);
            request_body["tool_choice"] = json!("auto");
        }

        let mut request = client.post(&self.api_endpoint).json(&request_body);

        // Add authorization header if API key is provided
        if let Some(ref api_key) = self.api_key {
            // Check if api_key already contains "Bearer " prefix
            let auth_header = if api_key.starts_with("Bearer ") {
                api_key.to_string()
            } else {
                format!("Bearer {}", api_key)
            };
            request = request.header("Authorization", auth_header);
        }

        // Make the request and handle response
        match request.send() {
            Ok(response) => {
                let status = response.status();
                let response_text = response
                    .text()
                    .unwrap_or_else(|_| "Failed to read response".to_string());
                if status.is_success() {
                    match serde_json::from_str::<Value>(&response_text) {
                        Ok(api_resp) => {
                            if let Some(msg) = Self::parse_api_message(&api_resp) {
                                debug!("Successfully parsed API response");
                                msg
                            } else {
                                error!("Invalid API response structure: missing required fields");
                                OpenAIMessage::assistant_text(
                                    "Error: Invalid response structure from API",
                                )
                            }
                        }
                        Err(e) => {
                            error!(
                                "Failed to parse API response: {}, original response: {:?}",
                                e, &response_text
                            );
                            OpenAIMessage::assistant_text(format!(
                                "Error: Failed to parse response - {}",
                                e
                            ))
                        }
                    }
                } else {
                    error!(
                        "API request failed with status {}: {}",
                        status, response_text
                    );
                    OpenAIMessage::assistant_text(format!(
                        "Error: API request failed with status {}",
                        status
                    ))
                }
            }
            Err(e) => {
                error!("Failed to send API request: {}", e);
                OpenAIMessage::assistant_text(format!("Error: Failed to send request - {}", e))
            }
        }
    }
}
