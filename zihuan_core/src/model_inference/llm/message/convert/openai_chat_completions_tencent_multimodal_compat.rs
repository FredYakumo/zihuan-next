use crate::model_inference::llm::message::convert::openai_chat_completions::apply_reasoning_params;
use crate::model_inference::llm::{InferenceParam, LLMMessage, LLMMessageConvertStyle};
use crate::model_inference::model_config::{ReasoningEffort, ReasoningParamStyle, ThinkingType};
use serde_json::Value;

pub fn build_tencent_multimodal_chat_completions_request_body(
    model_name: &str,
    param: &InferenceParam<'_>,
    stream: bool,
    include_reasoning_content: bool,
    thinking_type: Option<&ThinkingType>,
    reasoning_effort: Option<&ReasoningEffort>,
    reasoning_param_style: ReasoningParamStyle,
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

    apply_reasoning_params(
        &mut request_body,
        thinking_type,
        reasoning_effort,
        reasoning_param_style,
    );

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
