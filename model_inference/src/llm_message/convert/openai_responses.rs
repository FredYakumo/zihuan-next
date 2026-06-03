use log::warn;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio::sync::mpsc;
use zihuan_core::llm::tooling::{ToolCalls, ToolCallsFuncSpec};
use zihuan_core::llm::{
    str_to_role, InferenceParam, LLMMessage, LLMMessageConvertStyle, MessagePart, TokenUsage,
};

#[derive(Default)]
struct StreamToolCallDelta {
    id: Option<String>,
    type_name: Option<String>,
    function_name: Option<String>,
    function_arguments: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponsesRequestStyle {
    Default,
    MessageCompat,
    ImageUrlObjectCompat,
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

fn merge_stream_tool_call_delta(target: &mut StreamToolCallDelta, source: StreamToolCallDelta) {
    if target.id.is_none() {
        target.id = source.id;
    }
    if target.type_name.is_none() {
        target.type_name = source.type_name;
    }
    if target.function_name.is_none() {
        target.function_name = source.function_name;
    }
    if target.function_arguments.is_empty() {
        target.function_arguments = source.function_arguments;
    }
}

fn canonical_responses_tool_key(
    streamed_tool_calls: &mut BTreeMap<String, StreamToolCallDelta>,
    tool_call_aliases: &mut BTreeMap<String, String>,
    item_id: Option<&str>,
    call_id: Option<&str>,
) -> String {
    let item_id = item_id.filter(|value| !value.trim().is_empty());
    let call_id = call_id.filter(|value| !value.trim().is_empty());

    if let Some(call_id) = call_id {
        let key = call_id.to_string();
        if let Some(item_id) = item_id {
            if item_id != call_id {
                tool_call_aliases.insert(item_id.to_string(), key.clone());
                if let Some(existing) = streamed_tool_calls.remove(item_id) {
                    let target = streamed_tool_calls.entry(key.clone()).or_default();
                    merge_stream_tool_call_delta(target, existing);
                }
            }
        }
        return key;
    }

    if let Some(item_id) = item_id {
        return tool_call_aliases
            .get(item_id)
            .cloned()
            .unwrap_or_else(|| item_id.to_string());
    }

    "responses_function_call".to_string()
}

fn collect_responses_stream_tool_calls(
    streamed_tool_calls: BTreeMap<String, StreamToolCallDelta>,
) -> Vec<ToolCalls> {
    streamed_tool_calls
        .into_iter()
        .filter_map(|(index, call)| {
            let name = call.function_name.unwrap_or_default();
            if name.trim().is_empty() {
                warn!(
                    "Dropping incomplete Responses function call without name: id={}",
                    call.id.as_deref().unwrap_or(&index)
                );
                return None;
            }

            let arguments = if call.function_arguments.trim().is_empty() {
                Value::Null
            } else {
                serde_json::from_str::<Value>(&call.function_arguments)
                    .unwrap_or_else(|_| Value::String(call.function_arguments.clone()))
            };

            Some(ToolCalls {
                id: call.id.unwrap_or(index),
                type_name: call.type_name.unwrap_or_else(|| "function".to_string()),
                function: ToolCallsFuncSpec { name, arguments },
            })
        })
        .collect()
}

pub(crate) fn build_responses_request_body_for_style(
    model_name: &str,
    param: &InferenceParam<'_>,
    stream: bool,
    style: ResponsesRequestStyle,
    _include_reasoning_content: bool,
) -> Value {
    let convert_style = match style {
        ResponsesRequestStyle::Default => LLMMessageConvertStyle::OpenAiResponses,
        ResponsesRequestStyle::MessageCompat => LLMMessageConvertStyle::OpenAiResponsesMessageCompat,
        ResponsesRequestStyle::ImageUrlObjectCompat => {
            LLMMessageConvertStyle::OpenAiResponsesImageUrlObjectCompat
        }
    };
    let input = param
        .messages
        .iter()
        .flat_map(|msg| msg.convert(convert_style, false))
        .collect::<Vec<_>>();

    let mut request_body = json!({
        "model": model_name,
        "input": input,
        "stream": stream,
    });

    if let Some(tool_list) = param.tools.as_ref().map(|ts| {
        ts.iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "name": tool.name(),
                    "description": tool.description(),
                    "parameters": tool.parameters(),
                    "strict": false,
                })
            })
            .collect::<Vec<_>>()
    }) {
        request_body["tools"] = json!(tool_list);
        request_body["tool_choice"] = json!("auto");
    }

    request_body
}

pub fn build_responses_request_body(
    model_name: &str,
    param: &InferenceParam<'_>,
    stream: bool,
    include_reasoning_content: bool,
) -> Value {
    build_responses_request_body_for_style(
        model_name,
        param,
        stream,
        ResponsesRequestStyle::Default,
        include_reasoning_content,
    )
}

pub fn parse_responses_response(api_resp: &Value) -> Option<LLMMessage> {
    let output_items = api_resp.get("output")?.as_array()?;
    let mut role = str_to_role("assistant");
    let mut content = String::new();
    let mut reasoning_content = String::new();
    let mut tool_calls = Vec::new();
    let usage = parse_token_usage(api_resp.get("usage"));

    for item in output_items {
        match item.get("type").and_then(|value| value.as_str()) {
            Some("message") => {
                if let Some(role_str) = item.get("role").and_then(|value| value.as_str()) {
                    role = str_to_role(role_str);
                }
                if let Some(contents) = item.get("content").and_then(|value| value.as_array()) {
                    for content_item in contents {
                        match content_item.get("type").and_then(|value| value.as_str()) {
                            Some("output_text") | Some("text") => {
                                if let Some(text) =
                                    content_item.get("text").and_then(|value| value.as_str())
                                {
                                    content.push_str(text);
                                }
                            }
                            Some("reasoning") => {
                                if let Some(text) =
                                    content_item.get("summary").and_then(|value| value.as_str())
                                {
                                    reasoning_content.push_str(text);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Some("function_call") => {
                let Some(name) = item
                    .get("name")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.trim().is_empty())
                else {
                    warn!(
                        "Dropping incomplete Responses function call without name from completed response: id={}",
                        item.get("call_id")
                            .and_then(|value| value.as_str())
                            .or_else(|| item.get("id").and_then(|value| value.as_str()))
                            .unwrap_or("responses_function_call")
                    );
                    continue;
                };
                let arguments_raw = item
                    .get("arguments")
                    .and_then(|value| value.as_str())
                    .unwrap_or("{}");
                let arguments = serde_json::from_str::<Value>(arguments_raw)
                    .unwrap_or_else(|_| Value::String(arguments_raw.to_string()));
                let id = item
                    .get("call_id")
                    .and_then(|value| value.as_str())
                    .or_else(|| item.get("id").and_then(|value| value.as_str()))
                    .unwrap_or("responses_function_call")
                    .to_string();
                tool_calls.push(ToolCalls {
                    id,
                    type_name: "function".to_string(),
                    function: ToolCallsFuncSpec {
                        name: name.to_string(),
                        arguments,
                    },
                });
            }
            _ => {}
        }
    }

    if content.is_empty() && reasoning_content.is_empty() && tool_calls.is_empty() && usage.is_none() {
        return None;
    }

    Some(LLMMessage {
        role,
        parts: if content.is_empty() { Vec::new() } else { vec![MessagePart::text(content)] },
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

pub fn parse_responses_sse_response(response_text: &str) -> Option<LLMMessage> {
    let mut content = String::new();
    let mut streamed_tool_calls: BTreeMap<String, StreamToolCallDelta> = BTreeMap::new();
    let mut tool_call_aliases: BTreeMap<String, String> = BTreeMap::new();
    let mut completed_message = None;
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

        match chunk.get("type").and_then(|value| value.as_str()) {
            Some("response.output_text.delta") => {
                if let Some(delta) = chunk.get("delta").and_then(|value| value.as_str()) {
                    content.push_str(delta);
                }
            }
            Some("response.output_item.added") | Some("response.output_item.done") => {
                if let Some(item) = chunk.get("item") {
                    if item.get("type").and_then(|value| value.as_str()) == Some("function_call") {
                        let item_id = item.get("id").and_then(|value| value.as_str());
                        let call_id = item.get("call_id").and_then(|value| value.as_str());
                        let key = canonical_responses_tool_key(
                            &mut streamed_tool_calls,
                            &mut tool_call_aliases,
                            item_id,
                            call_id,
                        );
                        let entry = streamed_tool_calls.entry(key.clone()).or_default();
                        entry.id = Some(
                            call_id
                                .filter(|value| !value.trim().is_empty())
                                .unwrap_or(&key)
                                .to_string(),
                        );
                        entry.type_name = Some("function".to_string());
                        if let Some(name) = item
                            .get("name")
                            .and_then(|value| value.as_str())
                            .filter(|value| !value.trim().is_empty())
                        {
                            entry.function_name = Some(name.to_string());
                        }
                        if entry.function_arguments.is_empty() {
                            if let Some(arguments) = item.get("arguments").and_then(|value| value.as_str()) {
                                entry.function_arguments.push_str(arguments);
                            }
                        }
                    }
                }
            }
            Some("response.function_call_arguments.delta") => {
                let item_id = chunk.get("item_id").and_then(|value| value.as_str());
                let call_id = chunk.get("call_id").and_then(|value| value.as_str());
                let key = canonical_responses_tool_key(
                    &mut streamed_tool_calls,
                    &mut tool_call_aliases,
                    item_id,
                    call_id,
                );
                let entry = streamed_tool_calls.entry(key.clone()).or_default();
                entry.id = Some(
                    call_id
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or(&key)
                        .to_string(),
                );
                entry.type_name = Some("function".to_string());
                if let Some(delta) = chunk.get("delta").and_then(|value| value.as_str()) {
                    entry.function_arguments.push_str(delta);
                }
            }
            Some("response.function_call_arguments.done") => {
                let item_id = chunk.get("item_id").and_then(|value| value.as_str());
                let call_id = chunk.get("call_id").and_then(|value| value.as_str());
                let key = canonical_responses_tool_key(
                    &mut streamed_tool_calls,
                    &mut tool_call_aliases,
                    item_id,
                    call_id,
                );
                let entry = streamed_tool_calls.entry(key.clone()).or_default();
                entry.id = Some(
                    call_id
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or(&key)
                        .to_string(),
                );
                entry.type_name = Some("function".to_string());
                if entry.function_arguments.is_empty() {
                    if let Some(arguments) = chunk.get("arguments").and_then(|value| value.as_str()) {
                        entry.function_arguments.push_str(arguments);
                    }
                }
            }
            Some("response.completed") => {
                if let Some(response) = chunk.get("response") {
                    completed_message = parse_responses_response(response);
                }
            }
            _ => {}
        }
    }

    if let Some(message) = completed_message {
        return Some(message);
    }

    let tool_calls = collect_responses_stream_tool_calls(streamed_tool_calls);
    if content.is_empty() && tool_calls.is_empty() && usage.is_none() {
        None
    } else {
        Some(LLMMessage {
            role: str_to_role("assistant"),
            parts: if content.is_empty() { Vec::new() } else { vec![MessagePart::text(content)] },
            reasoning_content: None,
            tool_calls,
            tool_call_id: None,
            usage,
        })
    }
}

pub async fn parse_responses_sse_stream_response(
    response: reqwest::Response,
    token_tx: mpsc::UnboundedSender<String>,
) -> LLMMessage {
    use futures_util::StreamExt;

    let mut content = String::new();
    let mut streamed_tool_calls: BTreeMap<String, StreamToolCallDelta> = BTreeMap::new();
    let mut tool_call_aliases: BTreeMap<String, String> = BTreeMap::new();
    let mut completed_message = None;
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

            match chunk_data.get("type").and_then(|value| value.as_str()) {
                Some("response.output_text.delta") => {
                    if let Some(delta) = chunk_data.get("delta").and_then(|value| value.as_str()) {
                        if !delta.is_empty() {
                            content.push_str(delta);
                            let _ = token_tx.send(delta.to_string());
                        }
                    }
                }
                Some("response.output_item.added") | Some("response.output_item.done") => {
                    if let Some(item) = chunk_data.get("item") {
                        if item.get("type").and_then(|value| value.as_str()) == Some("function_call") {
                            let item_id = item.get("id").and_then(|value| value.as_str());
                            let call_id = item.get("call_id").and_then(|value| value.as_str());
                            let key = canonical_responses_tool_key(
                                &mut streamed_tool_calls,
                                &mut tool_call_aliases,
                                item_id,
                                call_id,
                            );
                            let entry = streamed_tool_calls.entry(key.clone()).or_default();
                            entry.id = Some(
                                call_id
                                    .filter(|value| !value.trim().is_empty())
                                    .unwrap_or(&key)
                                    .to_string(),
                            );
                            entry.type_name = Some("function".to_string());
                            if let Some(name) = item
                                .get("name")
                                .and_then(|value| value.as_str())
                                .filter(|value| !value.trim().is_empty())
                            {
                                entry.function_name = Some(name.to_string());
                            }
                            if entry.function_arguments.is_empty() {
                                if let Some(arguments) = item.get("arguments").and_then(|value| value.as_str()) {
                                    entry.function_arguments.push_str(arguments);
                                }
                            }
                        }
                    }
                }
                Some("response.function_call_arguments.delta") => {
                    let item_id = chunk_data.get("item_id").and_then(|value| value.as_str());
                    let call_id = chunk_data.get("call_id").and_then(|value| value.as_str());
                    let key = canonical_responses_tool_key(
                        &mut streamed_tool_calls,
                        &mut tool_call_aliases,
                        item_id,
                        call_id,
                    );
                    let entry = streamed_tool_calls.entry(key.clone()).or_default();
                    entry.id = Some(
                        call_id
                            .filter(|value| !value.trim().is_empty())
                            .unwrap_or(&key)
                            .to_string(),
                    );
                    entry.type_name = Some("function".to_string());
                    if let Some(delta) = chunk_data.get("delta").and_then(|value| value.as_str()) {
                        entry.function_arguments.push_str(delta);
                    }
                }
                Some("response.function_call_arguments.done") => {
                    let item_id = chunk_data.get("item_id").and_then(|value| value.as_str());
                    let call_id = chunk_data.get("call_id").and_then(|value| value.as_str());
                    let key = canonical_responses_tool_key(
                        &mut streamed_tool_calls,
                        &mut tool_call_aliases,
                        item_id,
                        call_id,
                    );
                    let entry = streamed_tool_calls.entry(key.clone()).or_default();
                    entry.id = Some(
                        call_id
                            .filter(|value| !value.trim().is_empty())
                            .unwrap_or(&key)
                            .to_string(),
                    );
                    entry.type_name = Some("function".to_string());
                    if entry.function_arguments.is_empty() {
                        if let Some(arguments) = chunk_data.get("arguments").and_then(|value| value.as_str()) {
                            entry.function_arguments.push_str(arguments);
                        }
                    }
                }
                Some("response.completed") => {
                    if let Some(response_value) = chunk_data.get("response") {
                        completed_message = parse_responses_response(response_value);
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(message) = completed_message {
        return message;
    }

    LLMMessage {
        role: str_to_role("assistant"),
        parts: if content.is_empty() { Vec::new() } else { vec![MessagePart::text(content)] },
        reasoning_content: None,
        tool_calls: collect_responses_stream_tool_calls(streamed_tool_calls),
        tool_call_id: None,
        usage,
    }
}
