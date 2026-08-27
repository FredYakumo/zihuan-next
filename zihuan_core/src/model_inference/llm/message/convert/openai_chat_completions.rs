use std::collections::BTreeMap;

use serde_json::Value;
use tokio::sync::mpsc;

use crate::model_inference::llm::message::marker::dsml::{DsmlContentParser, merge_tool_calls};
use crate::model_inference::model_config::{ReasoningEffort, ThinkingType};
use crate::model_inference::llm::tooling::{ToolCalls, ToolCallsFuncSpec};
use crate::model_inference::llm::{
    str_to_role, InferenceParam, LLMMessage, LLMMessageConvertStyle, MessagePart, StreamToken, TokenUsage,
};

#[derive(Default)]
struct StreamToolCallDelta {
    id: Option<String>,
    type_name: Option<String>,
    function_name: Option<String>,
    function_arguments: String,
}


fn collect_stream_tool_calls(streamed_tool_calls: BTreeMap<usize, StreamToolCallDelta>) -> Vec<ToolCalls> {
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
                id: call.id.unwrap_or_else(|| format!("stream_tool_call_{index}")),
                type_name: call.type_name.unwrap_or_else(|| "function".to_string()),
                function: ToolCallsFuncSpec {
                    name: call.function_name.unwrap_or_default(),
                    arguments,
                },
            }
        })
        .collect()
}

fn text_parts(text: String) -> Vec<MessagePart> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![MessagePart::text(text)]
    }
}

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
                                args.as_str().and_then(|s| serde_json::from_str::<Value>(s).ok())
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

fn parse_token_usage(value: Option<&Value>) -> Option<TokenUsage> {
    let value = value?;

    let cached_prompt_tokens = value
        .get("prompt_tokens_details")
        .and_then(|details| details.get("cached_tokens"))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .or_else(|| {
            value
                .get("input_tokens_details")
                .and_then(|details| details.get("cached_tokens"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
        })
        .or_else(|| {
            value
                .get("prompt_cache_hit_tokens")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
        })
        .or_else(|| value.get("cache_hit_tokens").and_then(|v| v.as_u64()).map(|v| v as usize));

    let prompt_cache_miss_tokens = value
        .get("prompt_cache_miss_tokens")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .or_else(|| value.get("cache_miss_tokens").and_then(|v| v.as_u64()).map(|v| v as usize));

    let prompt_tokens = value
        .get("prompt_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| value.get("input_tokens").and_then(|v| v.as_u64()))
        .map(|v| v as usize)
        .or_else(|| cached_prompt_tokens.zip(prompt_cache_miss_tokens).map(|(hit, miss)| hit + miss));

    Some(TokenUsage {
        prompt_tokens,
        cached_prompt_tokens,
        prompt_cache_miss_tokens,
        completion_tokens: value
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .or_else(|| value.get("output_tokens").and_then(|v| v.as_u64()))
            .map(|v| v as usize),
        total_tokens: value.get("total_tokens").and_then(|v| v.as_u64()).map(|v| v as usize),
    })
}

pub fn build_chat_completions_request_body(
    model_name: &str,
    param: &InferenceParam<'_>,
    stream: bool,
    include_reasoning_content: bool,
    thinking_type: Option<&ThinkingType>,
    reasoning_effort: Option<&ReasoningEffort>,
) -> Value {
    let mut request_body = serde_json::json!({
        "model": model_name,
        "messages": LLMMessage::convert_list(
            param.messages,
            LLMMessageConvertStyle::OpenAiChatCompletions,
            include_reasoning_content,
        ),
        "stream": stream,
    });

    if let Some(effort) = reasoning_effort {
        request_body["reasoning_effort"] = serde_json::json!(effort);
    }

    if let Some(thinking) = thinking_type {
        request_body["thinking"] = serde_json::json!({ "type": thinking });
    }

    if stream {
        request_body["stream_options"] = serde_json::json!({ "include_usage": true });
    }

    if let Some(tool_list) = param
        .tools
        .as_ref()
        .map(|ts| ts.iter().map(|tool| tool.get_json()).collect::<Vec<_>>())
    {
        request_body["tools"] = serde_json::json!(tool_list);
        request_body["tool_choice"] = serde_json::json!("auto");
    }

    request_body
}

pub fn parse_chat_completions_response(api_resp: &Value) -> Option<LLMMessage> {
    let choices = api_resp.get("choices")?.as_array()?;
    let choice = choices.first()?;
    let msg = choice.get("message")?;
    let mut content_dsml_parser = DsmlContentParser::new();
    let mut reasoning_dsml_parser = DsmlContentParser::new();
    let mut content = String::new();
    if let Some(text) = msg.get("content").and_then(Value::as_str) {
        content.extend(content_dsml_parser.push(text));
    }
    content.extend(content_dsml_parser.finish());
    let mut reasoning_content = String::new();
    if let Some(text) = msg.get("reasoning_content").and_then(Value::as_str) {
        reasoning_content.extend(reasoning_dsml_parser.push(text));
    }
    reasoning_content.extend(reasoning_dsml_parser.finish());
    let tool_calls = merge_tool_calls(
        merge_tool_calls(
            msg.get("tool_calls").map(parse_tool_calls).unwrap_or_default(),
            content_dsml_parser.tool_calls,
        ),
        reasoning_dsml_parser.tool_calls,
    );

    Some(LLMMessage {
        role: str_to_role(msg.get("role")?.as_str().unwrap_or("assistant")),
        parts: text_parts(content),
        reasoning_content: (!reasoning_content.is_empty()).then_some(reasoning_content),
        tool_calls,
        tool_call_id: msg.get("tool_call_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
        usage: parse_token_usage(api_resp.get("usage")),
    })
}

pub fn parse_chat_completions_sse_response(response_text: &str) -> Option<LLMMessage> {
    let mut role = None;
    let mut content = String::new();
    let mut reasoning_content = String::new();
    let mut streamed_tool_calls: BTreeMap<usize, StreamToolCallDelta> = BTreeMap::new();
    let mut final_tool_calls: Option<Vec<ToolCalls>> = None;
    let mut content_dsml_parser = DsmlContentParser::new();
    let mut reasoning_dsml_parser = DsmlContentParser::new();
    let mut usage: Option<TokenUsage> = None;

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

        if let Some(parsed_usage) = parse_token_usage(chunk.get("usage")) {
            usage = Some(parsed_usage);
        }

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
                content.extend(content_dsml_parser.push(piece));
            }
            if let Some(piece) = delta.get("reasoning_content").and_then(|value| value.as_str()) {
                reasoning_content.extend(reasoning_dsml_parser.push(piece));
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|value| value.as_array()) {
                for tool_call in tool_calls {
                    let index = tool_call
                        .get("index")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(streamed_tool_calls.len() as u64) as usize;
                    let entry = streamed_tool_calls.entry(index).or_default();
                    if let Some(id) = tool_call.get("id").and_then(|value| value.as_str()) {
                        if !id.is_empty() {
                            entry.id = Some(id.to_string());
                        }
                    }
                    if let Some(type_name) = tool_call.get("type").and_then(|value| value.as_str()) {
                        if !type_name.is_empty() {
                            entry.type_name = Some(type_name.to_string());
                        }
                    }
                    if let Some(function) = tool_call.get("function") {
                        if let Some(name) = function.get("name").and_then(|value| value.as_str()) {
                            if !name.is_empty() {
                                entry.function_name = Some(name.to_string());
                            }
                        }
                        if let Some(arguments) = function.get("arguments").and_then(|value| value.as_str()) {
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
                content.extend(content_dsml_parser.push(text));
            }
            if let Some(text) = message.get("reasoning_content").and_then(|value| value.as_str()) {
                reasoning_content.extend(reasoning_dsml_parser.push(text));
            }
            if let Some(tool_calls_value) = message.get("tool_calls") {
                let parsed = parse_tool_calls(tool_calls_value);
                if !parsed.is_empty() {
                    final_tool_calls = Some(parsed);
                }
            }
        }
    }

    content.extend(content_dsml_parser.finish());
    reasoning_content.extend(reasoning_dsml_parser.finish());
    let native_tool_calls = if let Some(tool_calls) = final_tool_calls {
        tool_calls
    } else {
        collect_stream_tool_calls(streamed_tool_calls)
    };
    let tool_calls = merge_tool_calls(
        merge_tool_calls(native_tool_calls, content_dsml_parser.tool_calls),
        reasoning_dsml_parser.tool_calls,
    );

    if content.is_empty() && reasoning_content.is_empty() && tool_calls.is_empty() {
        return None;
    }

    Some(LLMMessage {
        role: role.unwrap_or_else(|| str_to_role("assistant")),
        parts: text_parts(content),
        reasoning_content: if reasoning_content.is_empty() {
            None
        } else {
            Some(reasoning_content)
        },
        tool_calls,
        tool_call_id: None,
        usage,
    })
}

pub async fn parse_chat_completions_sse_stream_response(
    response: reqwest::Response,
    token_tx: mpsc::UnboundedSender<StreamToken>,
) -> LLMMessage {
    use futures_util::StreamExt;

    let mut role = None;
    let mut content = String::new();
    let mut reasoning_content = String::new();
    let mut streamed_tool_calls: BTreeMap<usize, StreamToolCallDelta> = BTreeMap::new();
    let mut final_tool_calls: Option<Vec<ToolCalls>> = None;
    let mut content_dsml_parser = DsmlContentParser::new();
    let mut reasoning_dsml_parser = DsmlContentParser::new();
    let mut usage: Option<TokenUsage> = None;
    let mut stream = response.bytes_stream();
    let mut sse_buffer = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(_) => break,
        };

        sse_buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(line_end) = sse_buffer.find('\n') {
            let line = sse_buffer[..line_end].trim_end_matches('\r').to_string();
            sse_buffer = sse_buffer[line_end + 1..].to_string();

            if !line.starts_with("data:") {
                continue;
            }

            let payload = line.trim_start_matches("data:").trim();
            if payload.is_empty() || payload == "[DONE]" {
                continue;
            }

            let Ok(chunk_data) = serde_json::from_str::<Value>(payload) else {
                continue;
            };

            if let Some(parsed_usage) = parse_token_usage(chunk_data.get("usage")) {
                usage = Some(parsed_usage);
            }

            let choice = chunk_data.get("choices").and_then(|v| v.as_array()).and_then(|arr| arr.first());
            let Some(choice) = choice else {
                continue;
            };

            if let Some(delta) = choice.get("delta") {
                if let Some(role_str) = delta.get("role").and_then(|v| v.as_str()) {
                    role = Some(str_to_role(role_str));
                }
                if let Some(piece) = delta.get("content").and_then(|v| v.as_str()) {
                    if !piece.is_empty() {
                        for emitted in content_dsml_parser.push(piece) {
                            content.push_str(&emitted);
                            let _ = token_tx.send(StreamToken::content(emitted));
                        }
                    }
                }
                if let Some(piece) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                    for emitted in reasoning_dsml_parser.push(piece) {
                        reasoning_content.push_str(&emitted);
                        let _ = token_tx.send(StreamToken::thinking(emitted));
                    }
                }
                if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                    for tool_call in tool_calls {
                        let index = tool_call
                            .get("index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(streamed_tool_calls.len() as u64) as usize;
                        let entry = streamed_tool_calls.entry(index).or_default();
                        if let Some(id) = tool_call.get("id").and_then(|v| v.as_str()) {
                            if !id.is_empty() {
                                entry.id = Some(id.to_string());
                            }
                        }
                        if let Some(type_name) = tool_call.get("type").and_then(|v| v.as_str()) {
                            if !type_name.is_empty() {
                                entry.type_name = Some(type_name.to_string());
                            }
                        }
                        if let Some(function) = tool_call.get("function") {
                            if let Some(name) = function.get("name").and_then(|v| v.as_str()) {
                                if !name.is_empty() {
                                    entry.function_name = Some(name.to_string());
                                }
                            }
                            if let Some(arguments) = function.get("arguments").and_then(|v| v.as_str()) {
                                entry.function_arguments.push_str(arguments);
                            }
                        }
                    }
                }
            } else if let Some(message) = choice.get("message") {
                if let Some(role_str) = message.get("role").and_then(|v| v.as_str()) {
                    role = Some(str_to_role(role_str));
                }
                if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
                    for emitted in content_dsml_parser.push(text) {
                        content.push_str(&emitted);
                        let _ = token_tx.send(StreamToken::content(emitted));
                    }
                }
                if let Some(text) = message.get("reasoning_content").and_then(|v| v.as_str()) {
                    for emitted in reasoning_dsml_parser.push(text) {
                        reasoning_content.push_str(&emitted);
                        let _ = token_tx.send(StreamToken::thinking(emitted));
                    }
                }
                if let Some(tool_calls_value) = message.get("tool_calls") {
                    let parsed = parse_tool_calls(tool_calls_value);
                    if !parsed.is_empty() {
                        final_tool_calls = Some(parsed);
                    }
                }
            }
        }
    }

    for emitted in content_dsml_parser.finish() {
        content.push_str(&emitted);
        let _ = token_tx.send(StreamToken::content(emitted));
    }
    for emitted in reasoning_dsml_parser.finish() {
        reasoning_content.push_str(&emitted);
        let _ = token_tx.send(StreamToken::thinking(emitted));
    }
    drop(token_tx);

    let native_tool_calls = if let Some(tool_calls) = final_tool_calls {
        tool_calls
    } else {
        collect_stream_tool_calls(streamed_tool_calls)
    };
    let tool_calls = merge_tool_calls(
        merge_tool_calls(native_tool_calls, content_dsml_parser.tool_calls),
        reasoning_dsml_parser.tool_calls,
    );

    if content.is_empty() && reasoning_content.is_empty() && tool_calls.is_empty() && usage.is_none() {
        return LLMMessage::assistant_text("");
    }

    LLMMessage {
        role: role.unwrap_or_else(|| str_to_role("assistant")),
        parts: text_parts(content),
        reasoning_content: if reasoning_content.is_empty() {
            None
        } else {
            Some(reasoning_content)
        },
        tool_calls,
        tool_call_id: None,
        usage,
    }
}
