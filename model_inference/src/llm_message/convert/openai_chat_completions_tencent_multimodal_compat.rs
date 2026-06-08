use serde_json::Value;
use zihuan_core::llm::{InferenceParam, LLMMessage, LLMMessageConvertStyle};

pub fn build_tencent_multimodal_chat_completions_request_body(
    model_name: &str,
    param: &InferenceParam<'_>,
    stream: bool,
    include_reasoning_content: bool,
) -> Value {
    let mut request_body = serde_json::json!({
        "model": model_name,
        "messages": LLMMessage::convert_list(
            param.messages,
            LLMMessageConvertStyle::OpenAiChatCompletionsTencentMultimodalCompat,
            include_reasoning_content,
        ),
        "stream": stream,
    });

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
