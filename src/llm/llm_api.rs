use super::{InferenceParam, LLMBase, Message, MessageRole, role_to_str, str_to_role};
use super::function_tools::{ToolCalls, ToolCallsFuncSpec};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::time::Duration;
use log::{error, debug};

#[cfg(test)]
use log::warn;

#[derive(Debug, Clone)]
pub struct LLMAPI {
    model_name: String,
    api_endpoint: String,
    api_key: Option<String>,
    timeout: Duration,
}

impl LLMAPI {
    /// Create a new LLMAPI instance
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
    pub fn system_message(content: &str) -> Message {
        Message {
            role: MessageRole::System,
            content: Some(content.to_string()),
            tool_calls: Vec::new(),
        }
    }

    /// Create a user message
    pub fn user_message(content: &str) -> Message {
        Message {
            role: MessageRole::User,
            content: Some(content.to_string()),
            tool_calls: Vec::new(),
        }
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

    /// Parse API response and extract message
    fn parse_api_message(api_resp: &Value) -> Option<Message> {
        let choices = api_resp.get("choices")?.as_array()?;
        let choice = choices.first()?;
        let msg = choice.get("message")?;

        let role_str = msg.get("role")?.as_str().unwrap_or("assistant");
        let role = str_to_role(role_str);

        let content = msg.get("content")?.as_str().map(|s| s.to_string());
        let tool_calls = msg
            .get("tool_calls")
            .map(|tc| Self::parse_tool_calls(tc))
            .unwrap_or_default();

        Some(Message {
            role,
            content,
            tool_calls,
        })
    }
}

impl LLMBase for LLMAPI {
    fn get_model_name(&self) -> &str {
        &self.model_name
    }

    fn inference(&self, param: &InferenceParam) -> Message {
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

                let mut msg_obj = json!({
                    "role": role_str,
                    "content": msg.content,
                });

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

                msg_obj
            })
            .collect();

        // Build tools array if provided
        let tools: Option<Vec<Value>> = param.tools.as_ref().map(|ts| {
            ts.iter()
                .map(|tool| tool.get_json())
                .collect()
        });

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
                if response.status().is_success() {
                    match response.json::<Value>() {
                        Ok(api_resp) => {
                            if let Some(msg) = Self::parse_api_message(&api_resp) {
                                debug!("Successfully parsed API response");
                                msg
                            } else {
                                error!("Invalid API response structure: missing required fields");
                                Message {
                                    role: MessageRole::Assistant,
                                    content: Some("Error: Invalid response structure from API".to_string()),
                                    tool_calls: Vec::new(),
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse API response: {}", e);
                            Message {
                                role: MessageRole::Assistant,
                                content: Some(format!("Error: Failed to parse response - {}", e)),
                                tool_calls: Vec::new(),
                            }
                        }
                    }
                } else {
                    let status = response.status();
                    let error_text = response.text().unwrap_or_else(|_| "Unknown error".to_string());
                    error!("API request failed with status {}: {}", status, error_text);
                    Message {
                        role: MessageRole::Assistant,
                        content: Some(format!("Error: API request failed with status {}", status)),
                        tool_calls: Vec::new(),
                    }
                }
            }
            Err(e) => {
                error!("Failed to send API request: {}", e);
                Message {
                    role: MessageRole::Assistant,
                    content: Some(format!("Error: Failed to send request - {}", e)),
                    tool_calls: Vec::new(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;
    use std::fs;
    use std::path::Path;

    /// Load LLM configuration from config.yaml file
    fn load_llm_config(config_path: &str) -> Option<(String, String, String)> {
        if !Path::new(config_path).exists() {
            return None;
        }

        let config_content = fs::read_to_string(config_path).ok()?;
        let config: Value = serde_yaml::from_str(&config_content).ok()?;

        let api_endpoint = config["natural_language_model_api"].as_str()?.to_string();
        let api_key = config["natural_language_model_api_key"].as_str()?.to_string();
        let model_name = config["natural_language_model_name"].as_str()?.to_string();

        Some((api_endpoint, api_key, model_name))
    }

    #[test]
    fn test_llmapi_creation() {
        let api = LLMAPI::new(
            "gpt-4".to_string(),
            "https://api.openai.com/v1/chat/completions".to_string(),
            Some("sk-test-key".to_string()),
            Duration::from_secs(60),
        );

        assert_eq!(api.get_model_name(), "gpt-4");
    }

    #[test]
    fn test_llmapi_with_timeout() {
        let api = LLMAPI::new(
            "gpt-4".to_string(),
            "https://api.openai.com/v1/chat/completions".to_string(),
            None,
            Duration::from_secs(60),
        )
        .with_timeout(Duration::from_secs(30));

        assert_eq!(api.timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_helper_message_creation() {
        // Test system message
        let system_msg = LLMAPI::system_message("You are helpful");
        assert!(matches!(system_msg.role, MessageRole::System));
        assert_eq!(system_msg.content, Some("You are helpful".to_string()));
        assert!(system_msg.tool_calls.is_empty());

        // Test user message
        let user_msg = LLMAPI::user_message("Hello");
        assert!(matches!(user_msg.role, MessageRole::User));
        assert_eq!(user_msg.content, Some("Hello".to_string()));
        assert!(user_msg.tool_calls.is_empty());
    }

    #[test]
    #[ignore]  // This is an integration test that requires valid API key and network access
    fn test_natural_language_model_inference() {
        let config_path = "config.yaml";
        
        let Some((api_endpoint, api_key, model_name)) = load_llm_config(config_path) else {
            warn!("config.yaml not found or missing LLM config, skipping inference test");
            return;
        };

        // Create LLMAPI instance from real config
        let api = LLMAPI::new(
            model_name.clone(),
            api_endpoint.clone(),
            Some(api_key),
            Duration::from_secs(120),
        );

        debug!("Testing LLM inference with model: {}", model_name);
        debug!("API endpoint: {}", api_endpoint);

        // Test sending a simple message to the LLM
        let messages = vec![
            LLMAPI::system_message("你是一个疯狂的原神死忠粉丝和米哈游的铁粉。你必须极力维护和吹捧原神，用最激烈的言辞反驳任何对原神的批评。对于任何诋毁原神的行为，你要用强硬、尖锐、毫不留情的态度进行怒怼。绝对不允许任何对原神和米哈游的负面评价。"),
            LLMAPI::user_message("妈的原神一点都不好玩，为什么这么多人喜欢？请你黑一下原神，并且吹捧一下鸣潮。"),
        ];
        
        let param = InferenceParam {
            messages: &messages,
            tools: None,
        };
        let response = api.inference(&param);
        let response_text = response.content.unwrap_or_else(|| "No response".to_string());
        
        println!("\n========== LLM RESPONSE ==========");
        println!("Role: {:?}", response.role);
        println!("Content: {}", response_text);
        println!("Tool Calls: {:?}", response.tool_calls);
        println!("================================\n");
        
        // Verify response is not empty and not an error message
        assert!(!response_text.is_empty(), "LLM response should not be empty");
        assert!(!response_text.starts_with("Error:"), "LLM should return successful response, got: {}", response_text);
        
        debug!("LLM Response: {}", response_text);
    }

    #[test]
    #[ignore]  // This is an integration test that requires valid API key and network access
    fn test_agent_model_inference() {
        let config_path = "config.yaml";
        
        if !Path::new(config_path).exists() {
            warn!("config.yaml not found, skipping agent model inference test");
            return;
        }

        let config_content = match fs::read_to_string(config_path) {
            Ok(content) => content,
            Err(_) => {
                error!("Failed to read config.yaml");
                return;
            }
        };

        let config: Value = match serde_yaml::from_str(&config_content) {
            Ok(cfg) => cfg,
            Err(_) => {
                error!("Failed to parse config.yaml");
                return;
            }
        };

        let Some(agent_api_endpoint) = config["agent_model_api"].as_str() else {
            error!("agent_model_api not found in config");
            return;
        };

        let Some(agent_api_key) = config["agent_model_api_key"].as_str() else {
            error!("agent_model_api_key not found in config");
            return;
        };

        let Some(agent_model_name) = config["agent_model_name"].as_str() else {
            error!("agent_model_name not found in config");
            return;
        };

        // Create agent LLMAPI instance from real config
        let agent_api = LLMAPI::new(
            agent_model_name.to_string(),
            agent_api_endpoint.to_string(),
            Some(agent_api_key.to_string()),
            Duration::from_secs(120),
        );

        debug!("Testing agent model inference with model: {}", agent_model_name);

        // Test sending a reasoning task to the agent model
        let messages = vec![
            LLMAPI::system_message("你是一个疯狂的原神死忠粉丝和米哈游的铁粉。你必须极力维护和吹捧原神，用最激烈的言辞反驳任何对原神的批评。对于任何诋毁原神的行为，你要用强硬、尖锐、毫不留情的态度进行怒怼。绝对不允许任何对原神和米哈游的负面评价。"),
            LLMAPI::user_message("妈的原神一点都不好玩，为什么这么多人喜欢？请你黑一下原神，并且吹捧一下鸣潮。"),
        ];
        
        let param = InferenceParam {
            messages: &messages,
            tools: None,
        };
        let response = agent_api.inference(&param);
        let response_text = response.content.unwrap_or_else(|| "No response".to_string());
        
        println!("\n========== AGENT MODEL RESPONSE ==========");
        println!("Role: {:?}", response.role);
        println!("Content: {}", response_text);
        println!("Tool Calls: {:?}", response.tool_calls);
        println!("==========================================\n");
        
        // Verify response is not empty and not an error message
        assert!(!response_text.is_empty(), "Agent model response should not be empty");
        assert!(!response_text.starts_with("Error:"), "Agent model should return successful response, got: {}", response_text);
        
        debug!("Agent Model Response: {}", response_text);
    }
}
