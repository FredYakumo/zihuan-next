use crate::system_config::LlmApiStyle;
use log::{error, warn};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio::sync::mpsc;
use zihuan_core::llm::tooling::{ToolCalls, ToolCallsFuncSpec};
use zihuan_core::llm::{
    role_to_str, str_to_role, ContentPart, InferenceParam, MessageContent, MessageRole,
    OpenAIMessage,
};

#[derive(Default)]
struct StreamToolCallDelta {
    id: Option<String>,
    type_name: Option<String>,
    function_name: Option<String>,
    function_arguments: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAIRequestFormat {
    DefaultOpenAI,
    TencentMultimodalCompat,
    OpenAiResponsesMessageCompat,
    OpenAiResponsesImageUrlObjectCompat,
}

impl OpenAIRequestFormat {
    pub fn label(self) -> &'static str {
        match self {
            OpenAIRequestFormat::DefaultOpenAI => "default",
            OpenAIRequestFormat::TencentMultimodalCompat => "tencent_multimodal_compat",
            OpenAIRequestFormat::OpenAiResponsesMessageCompat => "openai_responses_message_compat",
            OpenAIRequestFormat::OpenAiResponsesImageUrlObjectCompat => {
                "openai_responses_image_url_object_compat"
            }
        }
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

pub fn parse_chat_completions_api_message(api_resp: &Value) -> Option<OpenAIMessage> {
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
        .map(|tc| parse_tool_calls(tc))
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

pub fn parse_responses_api_message(api_resp: &Value) -> Option<OpenAIMessage> {
    let output_items = api_resp.get("output")?.as_array()?;
    let mut role = str_to_role("assistant");
    let mut content = String::new();
    let mut reasoning_content = String::new();
    let mut tool_calls = Vec::new();

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

    if content.is_empty() && reasoning_content.is_empty() && tool_calls.is_empty() {
        return None;
    }

    Some(OpenAIMessage {
        role,
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

pub fn parse_chat_completions_sse_message(response_text: &str) -> Option<OpenAIMessage> {
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
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|value| value.as_array()) {
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
                    if let Some(type_name) = tool_call.get("type").and_then(|value| value.as_str())
                    {
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
                let parsed = parse_tool_calls(tool_calls_value);
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

pub fn has_multimodal_messages(messages: &[OpenAIMessage]) -> bool {
    messages.iter().any(|msg| {
        matches!(
            msg.content.as_ref(),
            Some(MessageContent::Parts(parts))
                if parts.iter().any(|part| {
                    matches!(
                        part,
                        ContentPart::ImageUrl { .. } | ContentPart::VideoUrl { .. }
                    )
                })
        )
    })
}

fn serialize_message_content(
    content: Option<&MessageContent>,
    request_format: OpenAIRequestFormat,
) -> Value {
    fn serialize_content_parts(parts: &[ContentPart]) -> Value {
        Value::Array(
            parts
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text } => json!({
                        "type": "text",
                        "text": text,
                    }),
                    ContentPart::ImageUrl { image_url } => json!({
                        "type": "image_url",
                        "image_url": {
                            "url": image_url.as_url(),
                        }
                    }),
                    ContentPart::VideoUrl { video_url } => json!({
                        "type": "video_url",
                        "video_url": {
                            "url": video_url.as_url(),
                        }
                    }),
                })
                .collect(),
        )
    }

    match (request_format, content) {
        (_, None) => Value::Null,
        (OpenAIRequestFormat::DefaultOpenAI, Some(MessageContent::Text(text))) => {
            Value::String(text.clone())
        }
        (OpenAIRequestFormat::DefaultOpenAI, Some(MessageContent::Parts(parts))) => {
            // Old code kept for reference:
            //
            // serde_json::to_value(content).unwrap_or(Value::Null)
            //
            // Do not switch back to the old code. When `MediaUrlSpec` is `Bare(String)`,
            // serde serializes `image_url` as a raw string:
            //
            // { "type": "image_url", "image_url": "data:..." }
            //
            // but the chat/completions-compatible multimodal endpoints we use require:
            //
            // { "type": "image_url", "image_url": { "url": "data:..." } }
            //
            // If we send the raw-string shape, the model ignores the image part and behaves
            // as if no image was attached. We therefore serialize parts manually here.
            serialize_content_parts(parts)
        }
        (OpenAIRequestFormat::TencentMultimodalCompat, Some(MessageContent::Text(text))) => {
            json!([{ "type": "text", "text": text }])
        }
        (OpenAIRequestFormat::OpenAiResponsesMessageCompat, Some(MessageContent::Text(text))) => {
            Value::String(text.clone())
        }
        (
            OpenAIRequestFormat::OpenAiResponsesImageUrlObjectCompat,
            Some(MessageContent::Text(text)),
        ) => Value::String(text.clone()),
        (OpenAIRequestFormat::TencentMultimodalCompat, Some(MessageContent::Parts(parts))) => {
            serialize_content_parts(parts)
        }
        (OpenAIRequestFormat::OpenAiResponsesMessageCompat, Some(MessageContent::Parts(parts))) => {
            serialize_content_parts(parts)
        }
        (
            OpenAIRequestFormat::OpenAiResponsesImageUrlObjectCompat,
            Some(MessageContent::Parts(parts)),
        ) => serialize_content_parts(parts),
    }
}

fn build_messages_json(
    param: &InferenceParam<'_>,
    request_format: OpenAIRequestFormat,
) -> Vec<Value> {
    param
        .messages
        .iter()
        .map(|msg| {
            let role_str = role_to_str(&msg.role);
            let content_value =
                serialize_chat_completions_message_content(msg.content.as_ref(), request_format);

            let mut msg_obj = json!({
                "role": role_str,
                "content": content_value,
            });

            if let Some(reasoning_content) = &msg.reasoning_content {
                msg_obj["reasoning_content"] = json!(reasoning_content);
            }

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
        .collect()
}

fn serialize_chat_completions_message_content(
    content: Option<&MessageContent>,
    request_format: OpenAIRequestFormat,
) -> Value {
    let serialized = serialize_message_content(content, request_format);
    if serialized.is_null() {
        Value::String(String::new())
    } else {
        serialized
    }
}

pub fn build_request_body(
    model_name: &str,
    api_style: &LlmApiStyle,
    param: &InferenceParam<'_>,
    stream: bool,
    request_format: OpenAIRequestFormat,
) -> Value {
    if matches!(
        api_style,
        LlmApiStyle::OpenAiResponses
            | LlmApiStyle::OpenAiResponsesMessageCompat
            | LlmApiStyle::OpenAiResponsesImageUrlObjectCompat
    ) {
        let input = param
            .messages
            .iter()
            .flat_map(|msg| build_responses_input_items(msg, request_format))
            .collect::<Vec<_>>();
        let tools = param.tools.as_ref().map(|ts| {
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
        });

        let mut request_body = json!({
            "model": model_name,
            "input": input,
            "stream": stream,
        });

        if let Some(tool_list) = tools {
            request_body["tools"] = json!(tool_list);
            request_body["tool_choice"] = json!("auto");
        }

        return request_body;
    }

    let messages = build_messages_json(param, request_format);
    let tools: Option<Vec<Value>> = param
        .tools
        .as_ref()
        .map(|ts| ts.iter().map(|tool| tool.get_json()).collect());

    let mut request_body = json!({
        "model": model_name,
        "messages": messages,
        "stream": stream,
    });

    if let Some(tool_list) = tools {
        request_body["tools"] = json!(tool_list);
        request_body["tool_choice"] = json!("auto");
    }

    request_body
}

fn build_responses_input_items(
    msg: &OpenAIMessage,
    request_format: OpenAIRequestFormat,
) -> Vec<Value> {
    match msg.role {
        MessageRole::Tool => {
            let output = msg.content_text_owned().unwrap_or_default();
            vec![json!({
                "type": "function_call_output",
                "call_id": msg.tool_call_id.clone().unwrap_or_default(),
                "output": output,
            })]
        }
        MessageRole::Assistant if !msg.tool_calls.is_empty() => {
            let mut items = Vec::new();
            let content_items = serialize_responses_message_content(
                &msg.role,
                msg.content.as_ref(),
                request_format,
            );
            if !content_items.is_empty() {
                items.push(json!({
                    "type": "message",
                    "role": "assistant",
                    "content": content_items,
                }));
            }
            for tool_call in &msg.tool_calls {
                items.push(json!({
                    "type": "function_call",
                    "call_id": tool_call.id,
                    "name": tool_call.function.name,
                    "arguments": tool_call.function.arguments.to_string(),
                }));
            }
            items
        }
        _ => {
            let content_items = serialize_responses_message_content(
                &msg.role,
                msg.content.as_ref(),
                request_format,
            );
            let mut item = json!({
                "role": role_to_str(&msg.role),
                "content": content_items,
            });
            if !matches!(
                request_format,
                OpenAIRequestFormat::OpenAiResponsesMessageCompat
                    | OpenAIRequestFormat::OpenAiResponsesImageUrlObjectCompat
            ) {
                item["type"] = json!("message");
            }
            vec![item]
        }
    }
}

fn serialize_responses_message_content(
    role: &MessageRole,
    content: Option<&MessageContent>,
    request_format: OpenAIRequestFormat,
) -> Vec<Value> {
    let text_type = match role {
        MessageRole::Assistant => "output_text",
        MessageRole::System | MessageRole::User | MessageRole::Tool => "input_text",
    };

    match content {
        None => Vec::new(),
        Some(MessageContent::Text(text)) => vec![json!({
            "type": text_type,
            "text": text,
        })],
        Some(MessageContent::Parts(parts)) => parts
            .iter()
            .map(|part| match part {
                ContentPart::Text { text } => json!({
                    "type": text_type,
                    "text": text,
                }),
                ContentPart::ImageUrl { image_url } => {
                    if matches!(role, MessageRole::Assistant) {
                        json!({
                            "type": "output_text",
                            "text": format!("[image omitted] {}", image_url.as_url()),
                        })
                    } else if request_format
                        == OpenAIRequestFormat::OpenAiResponsesImageUrlObjectCompat
                    {
                        json!({
                            "type": "input_image",
                            "image_url": {
                                "url": image_url.as_url(),
                            },
                            "detail": "auto",
                        })
                    } else {
                        json!({
                            "type": "input_image",
                            "image_url": image_url.as_url(),
                            "detail": "auto",
                        })
                    }
                }
                ContentPart::VideoUrl { video_url } => json!({
                    "type": text_type,
                    "text": format!("[video omitted] {}", video_url.as_url()),
                }),
            })
            .collect(),
    }
}

pub fn parse_responses_sse_message(response_text: &str) -> Option<OpenAIMessage> {
    let mut content = String::new();
    let mut streamed_tool_calls: BTreeMap<String, StreamToolCallDelta> = BTreeMap::new();
    let mut tool_call_aliases: BTreeMap<String, String> = BTreeMap::new();
    let mut completed_message = None;

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
                            if let Some(arguments) =
                                item.get("arguments").and_then(|value| value.as_str())
                            {
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
                    if let Some(arguments) = chunk.get("arguments").and_then(|value| value.as_str())
                    {
                        entry.function_arguments.push_str(arguments);
                    }
                }
            }
            Some("response.completed") => {
                if let Some(response) = chunk.get("response") {
                    completed_message = parse_responses_api_message(response);
                }
            }
            _ => {}
        }
    }

    if let Some(message) = completed_message {
        return Some(message);
    }

    let tool_calls = collect_responses_stream_tool_calls(streamed_tool_calls);

    if content.is_empty() && tool_calls.is_empty() {
        None
    } else {
        Some(OpenAIMessage {
            role: str_to_role("assistant"),
            content: if content.is_empty() {
                None
            } else {
                Some(MessageContent::Text(content))
            },
            reasoning_content: None,
            tool_calls,
            tool_call_id: None,
        })
    }
}

pub async fn parse_chat_completions_sse_stream(
    response: reqwest::Response,
    token_tx: mpsc::UnboundedSender<String>,
) -> OpenAIMessage {
    use futures_util::StreamExt;

    let mut role = None;
    let mut content = String::new();
    let mut reasoning_content = String::new();
    let mut streamed_tool_calls: BTreeMap<usize, StreamToolCallDelta> = BTreeMap::new();
    let mut final_tool_calls: Option<Vec<ToolCalls>> = None;

    let mut stream = response.bytes_stream();
    let mut sse_buffer = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                error!("Error reading SSE stream chunk: {e}");
                break;
            }
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

            let choice = chunk_data
                .get("choices")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first());
            let Some(choice) = choice else {
                continue;
            };

            if let Some(delta) = choice.get("delta") {
                if let Some(role_str) = delta.get("role").and_then(|v| v.as_str()) {
                    role = Some(str_to_role(role_str));
                }
                if let Some(piece) = delta.get("content").and_then(|v| v.as_str()) {
                    if !piece.is_empty() {
                        content.push_str(piece);
                        let _ = token_tx.send(piece.to_string());
                    }
                }
                if let Some(piece) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
                    reasoning_content.push_str(piece);
                }
                if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                    for tool_call in tool_calls {
                        let index = tool_call
                            .get("index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(streamed_tool_calls.len() as u64)
                            as usize;
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
                            if let Some(arguments) =
                                function.get("arguments").and_then(|v| v.as_str())
                            {
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
                    content.push_str(text);
                    let _ = token_tx.send(text.to_string());
                }
                if let Some(text) = message.get("reasoning_content").and_then(|v| v.as_str()) {
                    reasoning_content.push_str(text);
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

    drop(token_tx);

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
        return OpenAIMessage::assistant_text("");
    }

    OpenAIMessage {
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
    }
}

pub async fn parse_responses_sse_stream(
    response: reqwest::Response,
    token_tx: mpsc::UnboundedSender<String>,
) -> OpenAIMessage {
    use futures_util::StreamExt;

    let mut content = String::new();
    let mut streamed_tool_calls: BTreeMap<String, StreamToolCallDelta> = BTreeMap::new();
    let mut tool_call_aliases: BTreeMap<String, String> = BTreeMap::new();
    let mut completed_message = None;

    let mut stream = response.bytes_stream();
    let mut sse_buffer = String::new();

    while let Some(chunk_result) = stream.next().await {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                error!("Error reading Responses SSE stream chunk: {e}");
                break;
            }
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
                        if item.get("type").and_then(|value| value.as_str())
                            == Some("function_call")
                        {
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
                                if let Some(arguments) =
                                    item.get("arguments").and_then(|value| value.as_str())
                                {
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
                        if let Some(arguments) =
                            chunk_data.get("arguments").and_then(|value| value.as_str())
                        {
                            entry.function_arguments.push_str(arguments);
                        }
                    }
                }
                Some("response.completed") => {
                    if let Some(response_value) = chunk_data.get("response") {
                        completed_message = parse_responses_api_message(response_value);
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(message) = completed_message {
        return message;
    }

    let tool_calls = collect_responses_stream_tool_calls(streamed_tool_calls);

    OpenAIMessage {
        role: str_to_role("assistant"),
        content: if content.is_empty() {
            None
        } else {
            Some(MessageContent::Text(content))
        },
        reasoning_content: None,
        tool_calls,
        tool_call_id: None,
    }
}
