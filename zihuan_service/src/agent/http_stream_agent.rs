use std::sync::Arc;

use log::info;
use model_inference::system_config::{
    load_agents, load_llm_refs, AgentConfig, AgentType, HttpStreamAgentConfig,
};
use salvo::http::header::{AUTHORIZATION, CONTENT_TYPE};
use salvo::http::{HeaderValue, StatusCode};
use salvo::prelude::*;
use storage_handler::{
    build_weaviate_ref, build_web_search_engine_ref, AgentMemoryAccessContext, ConnectionConfig,
    WeaviateCollectionSchema,
};
use tokio::task::JoinHandle;
use zihuan_agent::brain::BrainTool;
use zihuan_core::command::{
    CommandChannel, CommandContext, NewConversationRequest, SideEffectContext,
};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::{MessageRole, OpenAIMessage};
use zihuan_core::rag::WebSearchEngineRef;
use zihuan_core::runtime::block_async;
use zihuan_core::task_context::{
    AgentTaskRequest, AgentTaskResult, AgentTaskRuntime, AgentTaskStatus,
};
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;

use zihuan_graph_engine::data_value::EXECUTION_TASK_ID;

use super::inference::{infer_agent_response, resolve_agent_model_name};
use super::inference::{InferenceToolContext, InferenceToolProvider};
use super::tool_definitions::build_enabled_tool_definitions;
use super::tools::build_info_brain_tools;
use super::{AgentManager, AgentRuntimeState, AgentRuntimeStatus};
use crate::resource_resolver::{
    build_llm_model, resolve_llm_service_config, resolve_local_embedding_model_name,
};
use model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;

#[derive(Clone)]
struct HttpStreamRuntimeState {
    owner_agent: AgentConfig,
    task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
    task_db_connection_id: Option<String>,
}

struct HttpStreamCommandSideEffectContext {
    command_context: CommandContext,
}

impl SideEffectContext for HttpStreamCommandSideEffectContext {
    fn command_context(&self) -> &CommandContext {
        &self.command_context
    }

    fn start_new_conversation(&self, _request: &NewConversationRequest) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, serde::Deserialize)]
struct ChatCompletionsRequest {
    #[serde(default)]
    model: Option<String>,
    messages: Vec<zihuan_core::llm::OpenAIMessage>,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    agent_id: Option<String>,
}

enum HttpStreamCompletion {
    Json(serde_json::Value),
    Sse(String),
}

#[derive(Clone, Default)]
struct HttpStreamLoadedInferenceResources {
    web_search_engine_ref: Option<Arc<WebSearchEngineRef>>,
    default_tools_enabled: std::collections::HashMap<String, bool>,
    weaviate_memory_ref: Option<Arc<zihuan_core::weaviate::WeaviateRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
    memory_llm: Option<Arc<dyn LLMBase>>,
}

pub struct HttpStreamInferenceToolProvider {
    resources: HttpStreamLoadedInferenceResources,
    tool_definitions: Vec<BrainToolDefinition>,
}

impl InferenceToolProvider for HttpStreamInferenceToolProvider {
    fn build_default_tools(&self, _context: &InferenceToolContext) -> Vec<Box<dyn BrainTool>> {
        build_info_brain_tools(
            &self.resources.default_tools_enabled,
            self.resources.web_search_engine_ref.clone(),
            None,
            None,
            None,
            self.resources.weaviate_memory_ref.clone(),
            self.resources.embedding_model.clone(),
            self.resources.memory_llm.clone(),
            AgentMemoryAccessContext::default(),
            String::new(),
        )
    }

    fn tool_definitions(&self) -> Vec<BrainToolDefinition> {
        self.tool_definitions.clone()
    }
}

pub fn load_inference_tool_provider(
    agent: &AgentConfig,
    config: &HttpStreamAgentConfig,
    connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    Ok(Arc::new(HttpStreamInferenceToolProvider {
        resources: load_http_stream_resources(config, connections),
        tool_definitions: build_enabled_tool_definitions(&agent.tools)?,
    }))
}

fn load_http_stream_resources(
    config: &HttpStreamAgentConfig,
    connections: &[ConnectionConfig],
) -> HttpStreamLoadedInferenceResources {
    let web_search_engine_connection_id = config
        .web_search_engine_connection_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let web_search_engine_ref = build_web_search_engine_ref(
        web_search_engine_connection_id,
        connections,
    )
    .unwrap_or_else(|error| {
        log::warn!("[inference][http_stream] web search engine connection unavailable: {error}");
        None
    });

    let weaviate_memory_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(
            config
                .weaviate_memory_connection_id
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
            connections,
            Some(WeaviateCollectionSchema::AgentMemory),
        )
    })
    .unwrap_or_else(|error| {
        log::warn!("[inference][http_stream] weaviate memory connection unavailable: {error}");
        None
    });

    let llm_refs = load_llm_refs().unwrap_or_default();
    let embedding_model = config
        .embedding_model_ref_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .and_then(|model_ref_id| {
            match resolve_local_embedding_model_name(Some(model_ref_id), &llm_refs, "http_stream") {
                Ok(Some(_)) => block_async(
                    RuntimeEmbeddingModelManager::shared()
                        .get_or_create_embedding_model(model_ref_id),
                )
                .ok(),
                Ok(None) => None,
                Err(error) => {
                    log::warn!("[inference][http_stream] embedding model ref unavailable: {error}");
                    None
                }
            }
        });

    let memory_llm = config
        .llm_ref_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|llm_ref_id| resolve_llm_service_config(Some(llm_ref_id), &llm_refs, "http_stream"))
        .transpose()
        .ok()
        .flatten()
        .and_then(|llm_config| build_llm_model(&llm_config).ok());

    HttpStreamLoadedInferenceResources {
        web_search_engine_ref,
        default_tools_enabled: config.default_tools_enabled.clone(),
        weaviate_memory_ref,
        embedding_model,
        memory_llm,
    }
}

pub async fn spawn(
    manager: &AgentManager,
    agent: AgentConfig,
    config: HttpStreamAgentConfig,
    on_finish: super::OnFinishShared,
    task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
) -> Result<JoinHandle<()>> {
    validate_http_stream_config(&config)?;
    let acceptor = salvo::conn::TcpListener::new(config.bind.clone())
        .try_bind()
        .await
        .map_err(|err| {
            Error::StringError(format!(
                "failed to bind HTTP stream agent '{}' on {}: {}",
                agent.name, config.bind, err
            ))
        })?;

    let task_db_connection_id = Some(config.task_db_connection_id.trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let runtime_state = Arc::new(HttpStreamRuntimeState {
        owner_agent: agent.clone(),
        task_runtime,
        task_db_connection_id,
    });
    let auth_token = normalize_optional_token(config.api_key.clone());
    let router = Router::new()
        .hoop(salvo::affix_state::inject(runtime_state))
        .push(
            Router::with_path("v1/chat/completions")
                .hoop(HttpStreamAuth::new(auth_token))
                .post(http_stream_chat_completions),
        );
    let service = salvo::Service::new(router);
    let manager = manager.clone();
    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();

    Ok(tokio::spawn(async move {
        info!(
            "[service] starting HTTP stream agent '{}' on {}",
            agent_name, config.bind
        );
        salvo::Server::new(acceptor).serve(service).await;
        info!("[service] HTTP stream agent '{}' stopped", agent_name);
        manager.update_state(
            &agent_id,
            AgentRuntimeState {
                instance_id: None,
                status: AgentRuntimeStatus::Stopped,
                started_at: None,
                last_error: None,
            },
        );
        if let Some(cb) = on_finish.lock().unwrap().take() {
            cb(true, None);
        }
    }))
}

struct HttpStreamAuth {
    api_key: Option<String>,
}

impl HttpStreamAuth {
    fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl Handler for HttpStreamAuth {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let Some(expected) = self.api_key.as_ref() else {
            ctrl.call_next(req, depot, res).await;
            return;
        };

        let provided = bearer_token(req.headers().get(AUTHORIZATION));
        if provided != Some(expected.as_str()) {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Text::Plain("Unauthorized"));
            ctrl.skip_rest();
            return;
        }

        ctrl.call_next(req, depot, res).await;
    }
}

#[handler]
async fn http_stream_chat_completions(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let runtime = depot
        .obtain::<Arc<HttpStreamRuntimeState>>()
        .expect("http stream runtime state missing");
    let body: ChatCompletionsRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => {
            render_http_stream_error(
                res,
                StatusCode::BAD_REQUEST,
                format!("invalid request body: {err}"),
            );
            return;
        }
    };

    if body.messages.is_empty() {
        render_http_stream_error(
            res,
            StatusCode::BAD_REQUEST,
            "messages must not be empty".to_string(),
        );
        return;
    }

    let request_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(req.remote_addr().to_string()));
    let model_hint = body.model.clone();
    let task_name = format!(
        "处理HTTP流请求[{}]",
        request_ip.as_deref().unwrap_or("unknown")
    );
    let task_handle = runtime.task_runtime.as_ref().map(|task_runtime| {
        task_runtime.start_task(AgentTaskRequest {
            task_name,
            agent_id: runtime.owner_agent.id.clone(),
            agent_name: runtime.owner_agent.name.clone(),
            user_ip: request_ip.clone(),
            owner_id: None,
            task_db_connection_id: runtime.task_db_connection_id.clone(),
        })
    });

    let task_id = task_handle.as_ref().map(|handle| handle.task_id.clone());
    let provided_api_key = bearer_token(req.headers().get(AUTHORIZATION)).map(ToOwned::to_owned);
    let result = match task_id {
        Some(task_id) => {
            EXECUTION_TASK_ID
                .scope(
                    task_id,
                    execute_http_stream_completion(runtime.as_ref(), body, provided_api_key),
                )
                .await
        }
        None => execute_http_stream_completion(runtime.as_ref(), body, provided_api_key).await,
    };

    match result {
        Ok(HttpStreamCompletion::Json(value)) => res.render(Json(value)),
        Ok(HttpStreamCompletion::Sse(body)) => {
            res.headers_mut().insert(
                CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream; charset=utf-8"),
            );
            res.render(Text::Plain(body));
        }
        Err(err) => {
            let error_message = err.to_string();
            if let Some(task_handle) = task_handle {
                task_handle.finish(AgentTaskResult {
                    status: Some(AgentTaskStatus::Failed),
                    result_summary: Some(format!("HTTP 流请求失败: {error_message}")),
                    error_message: Some(error_message.clone()),
                });
            }
            render_http_stream_error(res, StatusCode::UNPROCESSABLE_ENTITY, error_message);
            return;
        }
    }
    if let Some(task_handle) = task_handle {
        task_handle.finish(AgentTaskResult {
            status: Some(AgentTaskStatus::Success),
            result_summary: Some(format!(
                "已完成 HTTP 流请求，模型={}",
                model_hint.unwrap_or_else(|| "default".to_string())
            )),
            error_message: None,
        });
    }
}

async fn execute_http_stream_completion(
    runtime: &HttpStreamRuntimeState,
    request: ChatCompletionsRequest,
    provided_api_key: Option<String>,
) -> Result<HttpStreamCompletion> {
    let ChatCompletionsRequest {
        model,
        mut messages,
        stream,
        agent_id,
    } = request;
    let llm_refs = load_llm_refs()?;
    let agents = load_agents()?;
    let target_agent = resolve_http_stream_target_agent(runtime, &agents, agent_id.as_deref())?;
    let model_name = resolve_agent_model_name(&target_agent, &llm_refs)?;
    let completion_id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());
    let created = chrono::Utc::now().timestamp();

    if let Some(command_registry) = crate::command::global_command_registry() {
        let raw_user_text = messages
            .iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::User))
            .and_then(OpenAIMessage::content_text_owned);

        if let Some(raw_user_text) = raw_user_text {
            let command_context = CommandContext {
                agent_type: "http_stream".to_string(),
                agent_id: target_agent.id.clone(),
                caller_id: http_stream_command_caller_id(provided_api_key.as_deref()),
                channel: CommandChannel::HttpStream {
                    api_key: mask_http_stream_api_key(provided_api_key.as_deref()),
                },
            };

            if let Some(dispatch_result) =
                command_registry.dispatch(&command_context, &raw_user_text)
            {
                let side_effect_context = HttpStreamCommandSideEffectContext {
                    command_context: command_context.clone(),
                };
                for effect in &dispatch_result.result.side_effects {
                    effect.execute(&side_effect_context)?;
                }

                if let Some(passthrough_text) = dispatch_result.passthrough_text {
                    if dispatch_result.result.inject_to_llm {
                        messages.push(OpenAIMessage::assistant_text(dispatch_result.result.reply));
                        messages.push(OpenAIMessage::user(passthrough_text));
                    } else {
                        messages = vec![OpenAIMessage::user(passthrough_text)];
                    }
                } else {
                    let final_message = OpenAIMessage::assistant_text(dispatch_result.result.reply);
                    let model_name = model.unwrap_or(model_name);
                    if stream {
                        return Ok(HttpStreamCompletion::Sse(build_sse_response(
                            &completion_id,
                            created,
                            &model_name,
                            &final_message,
                        )));
                    }

                    return Ok(HttpStreamCompletion::Json(serde_json::json!({
                        "id": completion_id,
                        "object": "chat.completion",
                        "created": created,
                        "model": model_name,
                        "choices": [{
                            "index": 0,
                            "message": final_message,
                            "finish_reason": "stop"
                        }]
                    })));
                }
            }
        }
    }

    let final_message = infer_agent_response(&target_agent, &llm_refs, messages)?;

    let model_name = model.unwrap_or(model_name);
    if stream {
        Ok(HttpStreamCompletion::Sse(build_sse_response(
            &completion_id,
            created,
            &model_name,
            &final_message,
        )))
    } else {
        Ok(HttpStreamCompletion::Json(serde_json::json!({
            "id": completion_id,
            "object": "chat.completion",
            "created": created,
            "model": model_name,
            "choices": [{
                "index": 0,
                "message": final_message,
                "finish_reason": "stop"
            }]
        })))
    }
}

fn resolve_http_stream_target_agent(
    runtime: &HttpStreamRuntimeState,
    agents: &[AgentConfig],
    requested_agent_id: Option<&str>,
) -> Result<AgentConfig> {
    if let Some(agent_id) = requested_agent_id {
        return agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .cloned()
            .ok_or_else(|| Error::ValidationError(format!("agent '{}' not found", agent_id)))
            .and_then(ensure_http_stream_target_agent_enabled);
    }

    if let Some(agent) = agents.iter().find(|agent| {
        agent.is_default && agent.enabled && matches!(&agent.agent_type, AgentType::HttpStream(_))
    }) {
        return Ok(agent.clone());
    }

    ensure_http_stream_target_agent_enabled(runtime.owner_agent.clone())
}

fn ensure_http_stream_target_agent_enabled(agent: AgentConfig) -> Result<AgentConfig> {
    if !agent.enabled {
        return Err(Error::ValidationError(format!(
            "agent '{}' is disabled",
            agent.name
        )));
    }
    if !matches!(&agent.agent_type, AgentType::HttpStream(_)) {
        return Err(Error::ValidationError(format!(
            "agent '{}' is not an http_stream agent",
            agent.name
        )));
    }
    Ok(agent)
}

fn build_sse_response(
    completion_id: &str,
    created: i64,
    model_name: &str,
    final_message: &zihuan_core::llm::OpenAIMessage,
) -> String {
    let mut chunks = Vec::new();
    chunks.push(serde_json::json!({
        "id": completion_id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model_name,
        "choices": [{
            "index": 0,
            "delta": { "role": "assistant" },
            "finish_reason": serde_json::Value::Null
        }]
    }));

    let content = final_message.content_text_owned().unwrap_or_default();
    for piece in split_stream_chunks(&content) {
        chunks.push(serde_json::json!({
            "id": completion_id,
            "object": "chat.completion.chunk",
            "created": created,
            "model": model_name,
            "choices": [{
                "index": 0,
                "delta": { "content": piece },
                "finish_reason": serde_json::Value::Null
            }]
        }));
    }

    chunks.push(serde_json::json!({
        "id": completion_id,
        "object": "chat.completion.chunk",
        "created": created,
        "model": model_name,
        "choices": [{
            "index": 0,
            "delta": {},
            "finish_reason": "stop"
        }]
    }));

    let mut body = chunks
        .into_iter()
        .map(|chunk| format!("data: {}\n\n", chunk))
        .collect::<String>();
    body.push_str("data: [DONE]\n\n");
    body
}

fn split_stream_chunks(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    let chars = content.chars().collect::<Vec<_>>();
    chars
        .chunks(64)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

fn render_http_stream_error(res: &mut Response, status: StatusCode, message: String) {
    res.status_code(status);
    res.render(Json(serde_json::json!({
        "error": {
            "message": message,
            "type": "invalid_request_error"
        }
    })));
}

fn bearer_token(value: Option<&HeaderValue>) -> Option<&str> {
    value
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(str::trim)
}

fn normalize_optional_token(value: Option<String>) -> Option<String> {
    value
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

fn http_stream_command_caller_id(api_key: Option<&str>) -> String {
    api_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "http_stream_anonymous".to_string())
}

fn mask_http_stream_api_key(api_key: Option<&str>) -> String {
    let Some(api_key) = api_key.map(str::trim).filter(|value| !value.is_empty()) else {
        return "anonymous".to_string();
    };

    let chars = api_key.chars().collect::<Vec<_>>();
    if chars.len() <= 8 {
        let prefix: String = chars.iter().take(2).collect();
        let suffix: String = chars.iter().skip(chars.len().saturating_sub(2)).collect();
        return format!("{prefix}***{suffix}");
    }

    let prefix: String = chars.iter().take(4).collect();
    let suffix: String = chars.iter().skip(chars.len() - 4).collect();
    format!("{prefix}***{suffix}")
}

fn validate_http_stream_config(config: &HttpStreamAgentConfig) -> Result<()> {
    if config.bind.trim().is_empty() {
        return Err(Error::ValidationError(
            "http_stream bind must not be empty".to_string(),
        ));
    }
    let has_llm_ref = config
        .llm_ref_id
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());
    if !has_llm_ref {
        return Err(Error::ValidationError(
            "http_stream must define llm_ref_id".to_string(),
        ));
    }
    Ok(())
}
