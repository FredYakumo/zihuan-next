use std::sync::Arc;

use chrono::Utc;
use salvo::http::header::CONTENT_TYPE;
use salvo::http::{HeaderValue, StatusCode};
use salvo::prelude::*;
use salvo::writing::Json;
use sha2::{Digest, Sha256};
use zihuan_core::agent::resource_resolver::build_llm_model;
use zihuan_core::inference::system_config::{load_llm_refs, LlmRefConfig, ModelRefSpec};
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{InferenceParam, LLMMessage, MessageRole};
use zihuan_core::system_config::{GlobalSettingsSection, ModelHttpApiKey};

#[derive(serde::Deserialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    tools: Option<Vec<OpenAiTool>>,
}

#[derive(serde::Deserialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    type_name: String,
    function: OpenAiFunction,
}

#[derive(serde::Deserialize)]
struct OpenAiFunction {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    parameters: serde_json::Value,
}

#[derive(Debug)]
struct OpenAiFunctionTool {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

impl FunctionTool for OpenAiFunctionTool {
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { &self.description }
    fn parameters(&self) -> serde_json::Value { self.parameters.clone() }
    fn call(&self, _arguments: serde_json::Value) -> zihuan_core::error::Result<serde_json::Value> { Ok(serde_json::Value::Null) }
}

#[handler]
pub async fn list_models(req: &mut Request, res: &mut Response) {
    if !authorize(req) {
        return render_error(res, StatusCode::UNAUTHORIZED, "invalid_api_key", "Invalid API key");
    }
    let Ok(settings) = zihuan_core::system_config::load_section::<GlobalSettingsSection>() else {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Failed to load settings");
    };
    let Ok(llm_refs) = load_llm_refs() else {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Failed to load models");
    };
    let data = public_models(&settings.model_http_service.public_model_config_ids, &llm_refs)
        .into_iter()
        .map(|item| serde_json::json!({ "id": item.1, "object": "model", "created": 0, "owned_by": "zihuan-next" }))
        .collect::<Vec<_>>();
    res.render(Json(serde_json::json!({ "object": "list", "data": data })));
}

#[handler]
pub async fn chat_completions(req: &mut Request, res: &mut Response) {
    if !authorize(req) {
        return render_error(res, StatusCode::UNAUTHORIZED, "invalid_api_key", "Invalid API key");
    }
    let body: ChatCompletionsRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, "invalid_request_error", &error.to_string()),
    };
    if body.messages.is_empty() {
        return render_error(res, StatusCode::BAD_REQUEST, "invalid_request_error", "messages must not be empty");
    }
    let Ok(settings) = zihuan_core::system_config::load_section::<GlobalSettingsSection>() else {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Failed to load settings");
    };
    let Ok(llm_refs) = load_llm_refs() else {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, "server_error", "Failed to load models");
    };
    let Some((config, _)) = public_models(&settings.model_http_service.public_model_config_ids, &llm_refs)
        .into_iter()
        .find(|(_, model_name)| *model_name == body.model) else {
        return render_error(res, StatusCode::NOT_FOUND, "invalid_request_error", "The requested model is not available");
    };
    let ModelRefSpec::ChatLlm { llm } = &config.model else { unreachable!() };
    let llm = match build_llm_model(llm) {
        Ok(model) => model,
        Err(error) => return render_error(res, StatusCode::UNPROCESSABLE_ENTITY, "server_error", &error.to_string()),
    };
    let messages = match parse_openai_messages(body.messages) {
        Ok(messages) => messages,
        Err(message) => return render_error(res, StatusCode::BAD_REQUEST, "invalid_request_error", &message),
    };
    let tools = body.tools.as_ref().map(|items| {
        items.iter().filter(|item| item.type_name == "function").map(|item| {
            Arc::new(OpenAiFunctionTool {
                name: item.function.name.clone(),
                description: item.function.description.clone(),
                parameters: item.function.parameters.clone(),
            }) as Arc<dyn FunctionTool>
        }).collect::<Vec<_>>()
    });
    let param = InferenceParam { messages: &messages, tools: tools.as_ref() };
    let message = llm.inference(&param);
    let completion_id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    let created = Utc::now().timestamp();
    if body.stream {
        res.headers_mut().insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream; charset=utf-8"));
        res.render(Text::Plain(build_sse_response(&completion_id, created, &body.model, &message)));
        return;
    }
    res.render(Json(serde_json::json!({
        "id": completion_id,
        "object": "chat.completion",
        "created": created,
        "model": body.model,
        "choices": [{
            "index": 0,
            "message": openai_message(&message),
            "finish_reason": if message.tool_calls.is_empty() { "stop" } else { "tool_calls" }
        }],
        "usage": message.usage.as_ref().map(openai_usage),
    })));
}

fn parse_openai_messages(messages: Vec<serde_json::Value>) -> Result<Vec<LLMMessage>, String> {
    messages.into_iter().map(|value| {
        let role = value.get("role").and_then(serde_json::Value::as_str).ok_or_else(|| "message.role is required".to_string())?;
        let content = value.get("content").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
        let mut message = match role {
            "system" | "developer" => LLMMessage::system(content),
            "user" => LLMMessage::user(content),
            "assistant" => LLMMessage::assistant_text(content),
            "tool" => LLMMessage::tool_result(value.get("tool_call_id").and_then(serde_json::Value::as_str).unwrap_or_default(), content),
            _ => return Err(format!("unsupported message role '{role}'")),
        };
        if let Some(tool_calls) = value.get("tool_calls").and_then(serde_json::Value::as_array) {
            message.tool_calls = tool_calls.iter().filter_map(|call| {
                Some(zihuan_core::llm::tooling::ToolCalls {
                    id: call.get("id")?.as_str()?.to_string(),
                    type_name: call.get("type").and_then(serde_json::Value::as_str).unwrap_or("function").to_string(),
                    function: zihuan_core::llm::tooling::ToolCallsFuncSpec {
                        name: call.get("function")?.get("name")?.as_str()?.to_string(),
                        arguments: call.get("function")?.get("arguments").and_then(serde_json::Value::as_str).and_then(|value| serde_json::from_str(value).ok()).unwrap_or(serde_json::Value::Null),
                    },
                })
            }).collect();
        }
        Ok(message)
    }).collect()
}

fn authorize(req: &Request) -> bool {
    let Some(token) = req.headers().get("authorization").and_then(|value| value.to_str().ok()).and_then(|value| value.strip_prefix("Bearer ")) else { return false; };
    let Ok(settings) = zihuan_core::system_config::load_section::<GlobalSettingsSection>() else { return false; };
    if !settings.model_http_service.enabled { return false; }
    let hash = hex::encode(Sha256::digest(token.trim().as_bytes()));
    settings.model_http_service.api_keys.iter().any(|key| api_key_is_valid(key, &hash))
}

fn api_key_is_valid(key: &ModelHttpApiKey, hash: &str) -> bool {
    key.enabled && key.secret_hash == hash && key.expires_at.is_none_or(|expires_at| expires_at > Utc::now())
}

fn public_models<'a>(ids: &[String], models: &'a [LlmRefConfig]) -> Vec<(&'a LlmRefConfig, String)> {
    models.iter().filter_map(|item| {
        if !item.enabled || !ids.iter().any(|id| id == &item.id) { return None; }
        match &item.model {
            ModelRefSpec::ChatLlm { .. } => Some((item, item.name.clone())),
            ModelRefSpec::TextEmbeddingLocal { .. } => None,
        }
    }).collect()
}

fn openai_message(message: &LLMMessage) -> serde_json::Value {
    let mut value = serde_json::json!({
        "role": match message.role { MessageRole::System => "system", MessageRole::User => "user", MessageRole::Assistant => "assistant", MessageRole::Tool => "tool" },
        "content": message.content_text_owned(),
    });
    if !message.tool_calls.is_empty() {
        value["tool_calls"] = serde_json::json!(message.tool_calls.iter().map(|call| serde_json::json!({
            "id": call.id, "type": call.type_name, "function": { "name": call.function.name, "arguments": call.function.arguments.to_string() }
        })).collect::<Vec<_>>());
    }
    value
}

fn openai_usage(usage: &zihuan_core::llm::TokenUsage) -> serde_json::Value {
    serde_json::json!({ "prompt_tokens": usage.prompt_tokens.unwrap_or(0), "completion_tokens": usage.completion_tokens.unwrap_or(0), "total_tokens": usage.total_tokens.unwrap_or(0) })
}

fn build_sse_response(id: &str, created: i64, model: &str, message: &LLMMessage) -> String {
    let mut chunks = vec![serde_json::json!({ "id": id, "object": "chat.completion.chunk", "created": created, "model": model, "choices": [{ "index": 0, "delta": { "role": "assistant" }, "finish_reason": serde_json::Value::Null }] })];
    if !message.tool_calls.is_empty() {
        chunks.push(serde_json::json!({ "id": id, "object": "chat.completion.chunk", "created": created, "model": model, "choices": [{ "index": 0, "delta": { "tool_calls": message.tool_calls.iter().enumerate().map(|(index, call)| serde_json::json!({ "index": index, "id": call.id, "type": call.type_name, "function": { "name": call.function.name, "arguments": call.function.arguments.to_string() } })).collect::<Vec<_>>() }, "finish_reason": serde_json::Value::Null }] }));
    } else if let Some(content) = message.content_text_owned() {
        for part in content.chars().collect::<Vec<_>>().chunks(64) {
            let content = part.iter().collect::<String>();
            chunks.push(serde_json::json!({ "id": id, "object": "chat.completion.chunk", "created": created, "model": model, "choices": [{ "index": 0, "delta": { "content": content }, "finish_reason": serde_json::Value::Null }] }));
        }
    }
    chunks.push(serde_json::json!({ "id": id, "object": "chat.completion.chunk", "created": created, "model": model, "choices": [{ "index": 0, "delta": {}, "finish_reason": if message.tool_calls.is_empty() { "stop" } else { "tool_calls" } }] }));
    let mut response = chunks.into_iter().map(|chunk| format!("data: {chunk}\n\n")).collect::<String>();
    response.push_str("data: [DONE]\n\n");
    response
}

fn render_error(res: &mut Response, status: StatusCode, kind: &str, message: &str) {
    res.status_code(status);
    res.render(Json(serde_json::json!({ "error": { "message": message, "type": kind } })));
}
