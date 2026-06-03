use serde_json::Value;
use tokio::sync::mpsc;
use zihuan_core::llm::{InferenceParam, LLMMessage};

use super::openai_responses::{
    build_responses_request_body_for_style, parse_responses_response, parse_responses_sse_response,
    parse_responses_sse_stream_response, ResponsesRequestStyle,
};

pub fn build_responses_image_url_object_compat_request_body(
    model_name: &str,
    param: &InferenceParam<'_>,
    stream: bool,
    include_reasoning_content: bool,
) -> Value {
    build_responses_request_body_for_style(
        model_name,
        param,
        stream,
        ResponsesRequestStyle::ImageUrlObjectCompat,
        include_reasoning_content,
    )
}

pub fn parse_responses_image_url_object_compat_response(api_resp: &Value) -> Option<LLMMessage> {
    parse_responses_response(api_resp)
}

pub fn parse_responses_image_url_object_compat_sse_response(
    response_text: &str,
) -> Option<LLMMessage> {
    parse_responses_sse_response(response_text)
}

pub async fn parse_responses_image_url_object_compat_sse_stream_response(
    response: reqwest::Response,
    token_tx: mpsc::UnboundedSender<String>,
) -> LLMMessage {
    parse_responses_sse_stream_response(response, token_tx).await
}
