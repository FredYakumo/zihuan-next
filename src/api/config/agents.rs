//! REST API handlers for agent management.
//!
//! This module provides CRUD endpoints for agent configurations (create, read, update, delete),
//! start/stop lifecycle control, and runtime status queries. It also defines the
//! [`DefaultAgentTaskRuntime`] — the in-process implementation of [`AgentTaskRuntime`] that tracks
//! background agent tasks, reports progress, and broadcasts status changes to WebSocket clients.

use std::collections::HashMap;
use std::sync::Arc;

use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use sqlx;
use storage_handler::{mysql, sqlite, ConnectionConfig, ConnectionKind, WeaviateCollectionSchema};
use uuid::Uuid;
use zihuan_core::task_context::{
    AgentTaskHandle, AgentTaskInfo, AgentTaskRequest, AgentTaskResult, AgentTaskRuntime, AgentTaskStatus,
};

use ims_bot_adapter::{
    fetch_login_info, fetch_login_info_via_adapter_connection, parse_ims_bot_adapter_connection, qq_avatar_url,
};
use log::{info, warn};
use zihuan_service::agent::qq_chat_agent_service_ignore_store::{
    create_ignore_rule, delete_ignore_rule, list_ignore_rules, update_ignore_rule, QqChatAgentServiceIgnoreRuleUpsert,
};

use crate::api::state::{AppState, TaskStatus};
use crate::api::ws::{ServerMessage, WsBroadcast};
use crate::system_config;
use model_inference::system_config::load_llm_refs;
use model_inference::system_config::{AgentConfig, AgentToolConfig, AgentType, LlmRefConfig};
use zihuan_core::agent_config::QqChatAgentServiceConfig;
use zihuan_core::error::{Error as CoreError, Result as CoreResult};
use zihuan_service::AgentRuntimeInfo;

use super::{
    now_rfc3339, ok_response, render_bad_request, render_internal_error, render_not_found, render_unprocessable_entity,
};

#[derive(Serialize)]
struct AgentWithRuntime {
    #[serde(flatten)]
    agent: AgentConfig,
    runtime: AgentRuntimeInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    qq_chat_profile: Option<QqChatProfile>,
}

#[derive(Serialize)]
struct QqChatProfile {
    bot_user_id: Option<String>,
    bot_nickname: Option<String>,
    bot_avatar_url: Option<String>,
}

#[derive(Deserialize)]
pub struct IgnoreRuleMutationRequest {
    #[serde(default)]
    pub sender_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
}

struct DefaultAgentTaskRuntime {
    state: Arc<AppState>,
    broadcast_tx: WsBroadcast,
    background_tasks: Arc<std::sync::Mutex<HashMap<String, AgentTaskInfo>>>,
}

impl AgentTaskRuntime for DefaultAgentTaskRuntime {
    fn start_task(&self, request: AgentTaskRequest) -> Arc<AgentTaskHandle> {
        let task_id = self.state.tasks.lock().unwrap().add_agent_response_task(
            request.agent_id.clone(),
            request.task_name.clone(),
            request.user_ip.clone(),
            request.owner_id.clone(),
            request.task_db_connection_id.clone(),
        );

        let _ = self.broadcast_tx.send(ServerMessage::TaskStarted {
            task_id: task_id.clone(),
            graph_name: request.task_name.clone(),
            graph_session_id: request.agent_id.clone(),
        });

        let state = Arc::clone(&self.state);
        let broadcast_tx = self.broadcast_tx.clone();
        let bg_tasks = Arc::clone(&self.background_tasks);
        let owner = request.owner_id.clone();
        let created_at = chrono::Local::now();

        {
            let mut bg = self.background_tasks.lock().unwrap();
            bg.insert(
                task_id.clone(),
                AgentTaskInfo {
                    task_id: task_id.clone(),
                    task_name: request.task_name,
                    owner_id: owner,
                    agent_id: request.agent_id,
                    status: AgentTaskStatus::Running,
                    created_at,
                    finished_at: None,
                    progress: vec![],
                    result_summary: None,
                    error_message: None,
                },
            );
        }

        AgentTaskHandle::new(task_id.clone(), move |result: AgentTaskResult| {
            let status = result.status.unwrap_or_else(|| {
                if result.error_message.is_some() {
                    AgentTaskStatus::Failed
                } else {
                    AgentTaskStatus::Success
                }
            });

            let task_status = match status {
                AgentTaskStatus::Success => TaskStatus::Success,
                AgentTaskStatus::Failed => TaskStatus::Failed,
                AgentTaskStatus::Stopped => TaskStatus::Stopped,
                AgentTaskStatus::Running => TaskStatus::Running,
            };

            state.tasks.lock().unwrap().finish_task(
                &task_id,
                task_status.clone(),
                result.error_message.clone(),
                result.result_summary.clone(),
            );

            {
                let mut bg = bg_tasks.lock().unwrap();
                if let Some(info) = bg.get_mut(&task_id) {
                    info.status = status;
                    info.finished_at = Some(chrono::Local::now());
                    info.result_summary = result.result_summary.clone();
                    info.error_message = result.error_message.clone();
                }
            }

            match task_status {
                TaskStatus::Stopped => {
                    let _ = broadcast_tx.send(ServerMessage::TaskStopped { task_id: task_id.clone() });
                }
                TaskStatus::Success => {
                    let _ = broadcast_tx.send(ServerMessage::TaskFinished {
                        task_id: task_id.clone(),
                        success: true,
                        error: None,
                    });
                }
                TaskStatus::Failed => {
                    let _ = broadcast_tx.send(ServerMessage::TaskFinished {
                        task_id: task_id.clone(),
                        success: false,
                        error: result.error_message,
                    });
                }
                TaskStatus::Running => {}
            }
        })
    }

    fn spawn_task(
        &self,
        request: AgentTaskRequest,
        runner: Box<dyn FnOnce() + Send + 'static>,
    ) -> Arc<AgentTaskHandle> {
        let handle = self.start_task(request);
        let handle_clone = Arc::clone(&handle);
        tokio::spawn(async move {
            runner();
        });
        handle_clone
    }

    fn query_task(&self, task_id: &str) -> Option<AgentTaskInfo> {
        self.background_tasks.lock().unwrap().get(task_id).cloned()
    }

    fn list_tasks(&self, owner_id: &str) -> Vec<AgentTaskInfo> {
        self.background_tasks
            .lock()
            .unwrap()
            .values()
            .filter(|info| info.owner_id.as_deref() == Some(owner_id))
            .cloned()
            .collect()
    }

    fn append_task_progress(&self, task_id: &str, message: String) {
        if let Some(info) = self.background_tasks.lock().unwrap().get_mut(task_id) {
            info.progress.push(message.clone());
        }
        self.state.tasks.lock().unwrap().append_task_progress(task_id, message);
    }

    fn cancel_task(&self, task_id: &str) -> bool {
        self.state.tasks.lock().unwrap().stop_task(task_id)
    }
}

pub fn build_agent_task_runtime(state: Arc<AppState>, broadcast_tx: WsBroadcast) -> Arc<dyn AgentTaskRuntime> {
    if let Some(existing) = zihuan_service::command::global_task_runtime() {
        return existing;
    }
    let runtime: Arc<dyn AgentTaskRuntime> = Arc::new(DefaultAgentTaskRuntime {
        state,
        broadcast_tx,
        background_tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
    });
    zihuan_service::command::set_global_task_runtime(Arc::clone(&runtime));
    runtime
}

pub async fn start_agent_runtime(
    state: Arc<AppState>,
    broadcast_tx: WsBroadcast,
    agent: AgentConfig,
    connections: Vec<ConnectionConfig>,
) -> CoreResult<()> {
    let agent_name = agent.name.clone();
    let on_finish: Box<dyn FnOnce(bool, Option<String>) + Send + 'static> = Box::new(move |success, error_message| {
        if !success {
            log::warn!(
                "[agents] agent '{}' stopped with error: {}",
                agent_name,
                error_message.unwrap_or_else(|| "stopped".to_string())
            );
        }
    });

    let task_runtime = build_agent_task_runtime(state.clone(), broadcast_tx.clone());
    state
        .agent_manager
        .start_agent(agent, connections, Some(on_finish), Some(task_runtime))
        .await
}

#[derive(Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub is_default: bool,
    pub agent_type: AgentType,
    #[serde(default)]
    pub tools: Vec<AgentToolConfig>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateAgentRequest {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub is_default: bool,
    pub agent_type: AgentType,
    #[serde(default)]
    pub tools: Vec<AgentToolConfig>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[handler]
pub async fn list_agents(_req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<std::sync::Arc<crate::api::state::AppState>>().unwrap();
    match system_config::load_agents() {
        Ok(agents) => {
            let connections = match system_config::load_connections() {
                Ok(connections) => connections,
                Err(err) => return render_internal_error(res, err),
            };

            let mut items = Vec::with_capacity(agents.len());
            for mut agent in agents {
                let qq_chat_profile = match &agent.agent_type {
                    AgentType::QqChat(config) => resolve_qq_chat_profile(&connections, config).await,
                    AgentType::HttpStream(_) | AgentType::Workspace(_) => None,
                };

                // Clear avatar_url for QQ Chat agents (they use bot_avatar_url from qq_chat_profile)
                if matches!(agent.agent_type, AgentType::QqChat(_)) {
                    agent.avatar_url = None;
                }

                items.push(AgentWithRuntime {
                    runtime: state.agent_manager.runtime_info(&agent.id),
                    agent,
                    qq_chat_profile,
                });
            }

            res.render(Json(items));
        }
        Err(err) => render_internal_error(res, err),
    }
}

async fn resolve_qq_chat_profile(
    connections: &[ConnectionConfig],
    config: &QqChatAgentServiceConfig,
) -> Option<QqChatProfile> {
    let connection = connections
        .iter()
        .find(|item| item.id == config.ims_bot_adapter_connection_id)?;
    let ConnectionKind::BotAdapter(raw) = &connection.kind else {
        return None;
    };

    let bot_connection = parse_ims_bot_adapter_connection(raw).ok()?;
    let fallback_user_id = bot_connection
        .qq_id
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    match fetch_login_info_via_adapter_connection(&connection.id).await {
        Ok(info) => Some(QqChatProfile {
            bot_user_id: Some(info.user_id.clone()),
            bot_nickname: if info.nickname.trim().is_empty() {
                None
            } else {
                Some(info.nickname)
            },
            bot_avatar_url: qq_avatar_url(&info.user_id),
        }),
        Err(adapter_err) => {
            warn!(
                "[agents] failed to fetch bot login info via adapter for connection '{}': {}; falling back to direct fetch",
                connection.id, adapter_err
            );
            match fetch_login_info(&bot_connection).await {
                Ok(info) => Some(QqChatProfile {
                    bot_user_id: Some(info.user_id.clone()),
                    bot_nickname: if info.nickname.trim().is_empty() {
                        None
                    } else {
                        Some(info.nickname)
                    },
                    bot_avatar_url: qq_avatar_url(&info.user_id),
                }),
                Err(err) => {
                    warn!(
                        "[agents] failed to fetch bot login info for connection '{}': {}",
                        connection.id, err
                    );
                    fallback_user_id.map(|user_id| QqChatProfile {
                        bot_user_id: Some(user_id.clone()),
                        bot_nickname: None,
                        bot_avatar_url: qq_avatar_url(&user_id),
                    })
                }
            }
        }
    }
}

fn resolve_qq_chat_agent_service_config<'a>(
    agents: &'a [AgentConfig],
    agent_id: &str,
) -> Result<&'a QqChatAgentServiceConfig, String> {
    let agent = agents
        .iter()
        .find(|item| item.id == agent_id)
        .ok_or_else(|| "Agent not found".to_string())?;
    let AgentType::QqChat(config) = &agent.agent_type else {
        return Err("Agent is not a QQ Chat Agent Service".to_string());
    };
    Ok(config)
}

async fn resolve_agent_rdb_connection(agent_id: &str) -> CoreResult<zihuan_core::data_refs::RelationalDbConnection> {
    let agents = system_config::load_agents()?;
    let config =
        resolve_qq_chat_agent_service_config(&agents, agent_id).map_err(|err| zihuan_core::string_error!("{}", err))?;
    let rdb_id = config
        .resolved_rdb_id()
        .ok_or_else(|| zihuan_core::string_error!("QQ Chat Agent Service '{}' has no rdb_id configured", agent_id))?;
    let connections = system_config::load_connections()?;
    storage_handler::build_relational_db_connection_for_connection(rdb_id, &connections).await
}

fn render_ignore_rule_error(res: &mut Response, err: CoreError) {
    match err {
        CoreError::ValidationError(message) => render_unprocessable_entity(res, message),
        CoreError::StringError(message) => {
            if message.eq_ignore_ascii_case("agent not found") {
                render_not_found(res, &message);
            } else {
                render_unprocessable_entity(res, message);
            }
        }
        CoreError::StaticStrError(message) => render_unprocessable_entity(res, message.to_string()),
        other => render_internal_error(res, other),
    }
}

#[handler]
pub async fn list_agent_ignore_rules(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let id = req.param::<String>("id").unwrap_or_default();
    match resolve_agent_rdb_connection(&id).await {
        Ok(connection) => match list_ignore_rules(&connection, &id).await {
            Ok(items) => res.render(Json(items)),
            Err(err) => render_ignore_rule_error(res, err),
        },
        Err(err) => render_ignore_rule_error(res, err),
    }
}

#[handler]
pub async fn create_agent_ignore_rule(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let id = req.param::<String>("id").unwrap_or_default();
    let body: IgnoreRuleMutationRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let payload = QqChatAgentServiceIgnoreRuleUpsert {
        sender_id: body.sender_id,
        group_id: body.group_id,
    };
    match resolve_agent_rdb_connection(&id).await {
        Ok(connection) => match create_ignore_rule(&connection, &id, &payload).await {
            Ok(item) => res.render(Json(item)),
            Err(err) => render_ignore_rule_error(res, err),
        },
        Err(err) => render_ignore_rule_error(res, err),
    }
}

#[handler]
pub async fn update_agent_ignore_rule(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let id = req.param::<String>("id").unwrap_or_default();
    let rule_id = req.param::<i64>("rule_id").unwrap_or_default();
    let body: IgnoreRuleMutationRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let payload = QqChatAgentServiceIgnoreRuleUpsert {
        sender_id: body.sender_id,
        group_id: body.group_id,
    };
    match resolve_agent_rdb_connection(&id).await {
        Ok(connection) => match update_ignore_rule(&connection, &id, rule_id, &payload).await {
            Ok(item) => res.render(Json(item)),
            Err(err) => render_ignore_rule_error(res, err),
        },
        Err(err) => render_ignore_rule_error(res, err),
    }
}

#[handler]
pub async fn delete_agent_ignore_rule(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let id = req.param::<String>("id").unwrap_or_default();
    let rule_id = req.param::<i64>("rule_id").unwrap_or_default();
    match resolve_agent_rdb_connection(&id).await {
        Ok(connection) => match delete_ignore_rule(&connection, &id, rule_id).await {
            Ok(()) => res.render(Json(ok_response())),
            Err(err) => render_ignore_rule_error(res, err),
        },
        Err(err) => render_ignore_rule_error(res, err),
    }
}

#[handler]
pub async fn create_agent(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let body: CreateAgentRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let mut agents = match system_config::load_agents() {
        Ok(agents) => agents,
        Err(err) => return render_internal_error(res, err),
    };

    if let Err(message) = validate_default_agent_flag(&agents, None, body.is_default) {
        return render_unprocessable_entity(res, message);
    }
    let connections = match system_config::load_connections() {
        Ok(connections) => connections,
        Err(err) => return render_internal_error(res, err),
    };
    if let Err(message) = validate_agent_connection_schemas(&body.agent_type, &connections) {
        return render_unprocessable_entity(res, message);
    }
    let llm_refs = match load_llm_refs() {
        Ok(llm_refs) => llm_refs,
        Err(err) => return render_internal_error(res, err),
    };
    if let Err(message) = validate_qq_chat_agent_service_llms(&body.agent_type, &llm_refs, &body.name) {
        return render_unprocessable_entity(res, message);
    }

    let agent = AgentConfig {
        id: Uuid::new_v4().to_string(),
        config_id: String::new(),
        name: body.name,
        agent_type: body.agent_type,
        enabled: body.enabled,
        auto_start: body.auto_start,
        is_default: body.is_default,
        updated_at: now_rfc3339(),
        tools: body.tools,
        avatar_url: body.avatar_url.filter(|v| !v.is_empty()),
    };
    let mut agent = agent;
    agent.config_id = agent.id.clone();
    agents.push(agent.clone());

    match system_config::save_agents(agents) {
        Ok(()) => {
            info!("[agents] created agent '{}' (id={})", agent.name, agent.id);
            res.render(Json(agent));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn update_agent(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let id = req.param::<String>("id").unwrap_or_default();
    let body: UpdateAgentRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let mut agents = match system_config::load_agents() {
        Ok(agents) => agents,
        Err(err) => return render_internal_error(res, err),
    };

    if let Err(message) = validate_default_agent_flag(&agents, Some(id.as_str()), body.is_default) {
        return render_unprocessable_entity(res, message);
    }
    let connections = match system_config::load_connections() {
        Ok(connections) => connections,
        Err(err) => return render_internal_error(res, err),
    };
    if let Err(message) = validate_agent_connection_schemas(&body.agent_type, &connections) {
        return render_unprocessable_entity(res, message);
    }
    let llm_refs = match load_llm_refs() {
        Ok(llm_refs) => llm_refs,
        Err(err) => return render_internal_error(res, err),
    };
    if let Err(message) = validate_qq_chat_agent_service_llms(&body.agent_type, &llm_refs, &body.name) {
        return render_unprocessable_entity(res, message);
    }

    let Some(agent) = agents.iter_mut().find(|item| item.id == id) else {
        return render_not_found(res, "Agent not found");
    };

    agent.name = body.name;
    agent.agent_type = body.agent_type;
    agent.enabled = body.enabled;
    agent.auto_start = body.auto_start;
    agent.is_default = body.is_default;
    agent.updated_at = now_rfc3339();
    agent.tools = body.tools;
    agent.avatar_url = body.avatar_url.filter(|v| !v.is_empty());
    let response = agent.clone();

    match system_config::save_agents(agents) {
        Ok(()) => {
            info!("[agents] updated agent '{}' (id={})", response.name, response.id);
            res.render(Json(response));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn start_agent(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap().clone();
    let broadcast_tx = depot.obtain::<WsBroadcast>().unwrap().clone();
    let id = req.param::<String>("id").unwrap_or_default();
    let agents = match system_config::load_agents() {
        Ok(agents) => agents,
        Err(err) => return render_internal_error(res, err),
    };
    let Some(agent) = agents.into_iter().find(|item| item.id == id) else {
        return render_not_found(res, "Agent not found");
    };
    let connections = match system_config::load_connections() {
        Ok(connections) => connections,
        Err(err) => return render_internal_error(res, err),
    };
    if let Err(message) = validate_agent_connection_schemas(&agent.agent_type, &connections) {
        return render_unprocessable_entity(res, message);
    }
    let llm_refs = match load_llm_refs() {
        Ok(llm_refs) => llm_refs,
        Err(err) => return render_internal_error(res, err),
    };
    if let Err(message) = validate_qq_chat_agent_service_llms(&agent.agent_type, &llm_refs, &agent.name) {
        return render_unprocessable_entity(res, message);
    }

    info!("[agents] starting agent '{}' (id={})", agent.name, id,);
    if let Err(err) = start_agent_runtime(state.clone(), broadcast_tx, agent.clone(), connections).await {
        log::error!("[agents] failed to start agent '{}' (id={}): {}", agent.name, id, err);
        return render_internal_error(res, err);
    }
    res.render(Json(serde_json::json!({
        "ok": true,
        "runtime": state.agent_manager.runtime_info(&id),
    })));
}

#[handler]
pub async fn stop_agent(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<std::sync::Arc<crate::api::state::AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    info!("[agents] stopping agent (id={})", id);
    match state.agent_manager.stop_agent(&id).await {
        Ok(()) => res.render(Json(serde_json::json!({
            "ok": true,
            "runtime": state.agent_manager.runtime_info(&id),
        }))),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn delete_agent(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let id = req.param::<String>("id").unwrap_or_default();
    let mut agents = match system_config::load_agents() {
        Ok(agents) => agents,
        Err(err) => return render_internal_error(res, err),
    };
    let before = agents.len();
    agents.retain(|item| item.id != id);

    if before == agents.len() {
        return render_not_found(res, "Agent not found");
    }

    match system_config::save_agents(agents) {
        Ok(()) => {
            info!("[agents] deleted agent (id={})", id);
            res.render(Json(ok_response()));
        }
        Err(err) => render_internal_error(res, err),
    }
}

fn validate_default_agent_flag(
    agents: &[AgentConfig],
    current_id: Option<&str>,
    new_is_default: bool,
) -> Result<(), String> {
    if !new_is_default {
        return Ok(());
    }

    let has_other_default = agents
        .iter()
        .any(|agent| agent.is_default && current_id.map(|id| agent.id != id).unwrap_or(true));

    if has_other_default {
        Err("Only one default agent can be enabled at a time".to_string())
    } else {
        Ok(())
    }
}

fn validate_agent_connection_schemas(agent_type: &AgentType, connections: &[ConnectionConfig]) -> Result<(), String> {
    match agent_type {
        AgentType::QqChat(config) => {
            validate_rdb_connection(connections, config.resolved_rdb_id())?;
            validate_weaviate_connection_schema(
                connections,
                config.weaviate_image_connection_id.as_deref(),
                WeaviateCollectionSchema::ImageSemantic,
                "weaviate_image_connection_id",
            )?;
            validate_weaviate_connection_schema(
                connections,
                config.weaviate_memory_connection_id.as_deref(),
                WeaviateCollectionSchema::AgentMemory,
                "weaviate_memory_connection_id",
            )?;
            Ok(())
        }
        AgentType::HttpStream(config) => validate_weaviate_connection_schema(
            connections,
            config.weaviate_memory_connection_id.as_deref(),
            WeaviateCollectionSchema::AgentMemory,
            "weaviate_memory_connection_id",
        ),
        AgentType::Workspace(_) => Ok(()),
    }
}

fn validate_qq_chat_agent_service_llms(
    agent_type: &AgentType,
    llm_refs: &[LlmRefConfig],
    agent_name: &str,
) -> Result<(), String> {
    match agent_type {
        AgentType::QqChat(config) => {
            let llm_ref_id = config
                .llm_ref_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("agent '{}' is missing llm_ref_id", agent_name))?;
            let resolved_llm_ref_id = config
                .image_understand_llm_ref_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(llm_ref_id);

            let llm_ref = llm_refs
                .iter()
                .find(|item| item.id == resolved_llm_ref_id || item.config_id == resolved_llm_ref_id)
                .ok_or_else(|| {
                    format!("agent '{}' references missing llm_ref '{}'", agent_name, resolved_llm_ref_id)
                })?;
            if !llm_ref.enabled {
                return Err(format!("agent '{}' references disabled llm_ref '{}'", agent_name, llm_ref.name));
            }
            match &llm_ref.model {
                model_inference::system_config::ModelRefSpec::ChatLlm { llm } => {
                    if llm.supports_multimodal_input {
                        Ok(())
                    } else if config
                        .image_understand_llm_ref_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_some()
                    {
                        Err(format!(
                            "image_understand_llm_ref_id '{}' does not support multimodal input",
                            llm_ref.name
                        ))
                    } else {
                        Err(format!(
                            "main llm_ref_id '{}' does not support multimodal input; please choose a multimodal model for image_understand_llm_ref_id",
                            llm_ref.name
                        ))
                    }
                }
                model_inference::system_config::ModelRefSpec::TextEmbeddingLocal { .. } => Err(format!(
                    "agent '{}' references non-chat model_ref '{}' as image_understand_llm_ref_id",
                    agent_name, llm_ref.name
                )),
            }?;

            validate_chat_llm_ref(
                llm_refs,
                config
                    .natural_language_reply_llm_ref_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                agent_name,
                "natural_language_reply_llm_ref_id",
            )?;

            validate_embedding_model_ref(llm_refs, config.embedding_model_ref_id.as_deref(), agent_name)
        }
        AgentType::HttpStream(config) => {
            validate_embedding_model_ref(llm_refs, config.embedding_model_ref_id.as_deref(), agent_name)
        }
        AgentType::Workspace(config) => validate_chat_llm_ref(
            llm_refs,
            config.llm_ref_id.as_deref().map(str::trim).filter(|value| !value.is_empty()),
            agent_name,
            "llm_ref_id",
        ),
    }
}

fn validate_chat_llm_ref(
    llm_refs: &[LlmRefConfig],
    llm_ref_id: Option<&str>,
    agent_name: &str,
    field_name: &str,
) -> Result<(), String> {
    let llm_ref_id = llm_ref_id.ok_or_else(|| format!("agent '{}' is missing {}", agent_name, field_name))?;
    let llm_ref = llm_refs
        .iter()
        .find(|item| item.id == llm_ref_id || item.config_id == llm_ref_id)
        .ok_or_else(|| format!("agent '{}' references missing {} '{}'", agent_name, field_name, llm_ref_id))?;
    if !llm_ref.enabled {
        return Err(format!(
            "agent '{}' references disabled {} '{}'",
            agent_name, field_name, llm_ref.name
        ));
    }
    match llm_ref.model {
        model_inference::system_config::ModelRefSpec::ChatLlm { .. } => Ok(()),
        model_inference::system_config::ModelRefSpec::TextEmbeddingLocal { .. } => Err(format!(
            "agent '{}' references non-chat model_ref '{}' as {}",
            agent_name, llm_ref.name, field_name
        )),
    }
}

fn validate_embedding_model_ref(
    llm_refs: &[LlmRefConfig],
    embedding_model_ref_id: Option<&str>,
    agent_name: &str,
) -> Result<(), String> {
    let Some(embedding_model_ref_id) = embedding_model_ref_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };

    let llm_ref = llm_refs
        .iter()
        .find(|item| item.id == embedding_model_ref_id || item.config_id == embedding_model_ref_id)
        .ok_or_else(|| {
            format!(
                "agent '{}' references missing embedding_model_ref '{}'",
                agent_name, embedding_model_ref_id
            )
        })?;
    if !llm_ref.enabled {
        return Err(format!(
            "agent '{}' references disabled embedding model_ref '{}'",
            agent_name, llm_ref.name
        ));
    }
    match llm_ref.model {
        model_inference::system_config::ModelRefSpec::TextEmbeddingLocal { .. } => Ok(()),
        model_inference::system_config::ModelRefSpec::ChatLlm { .. } => Err(format!(
            "agent '{}' references chat model_ref '{}' as embedding_model_ref_id",
            agent_name, llm_ref.name
        )),
    }
}

fn validate_rdb_connection(connections: &[ConnectionConfig], connection_id: Option<&str>) -> Result<(), String> {
    let Some(connection_id) = connection_id else {
        return Ok(());
    };
    let connection = connections
        .iter()
        .find(|item| item.id == connection_id || item.config_id == connection_id)
        .ok_or_else(|| format!("rdb_id '{}' not found", connection_id))?;
    if !matches!(connection.kind, ConnectionKind::Mysql(_) | ConnectionKind::Sqlite(_)) {
        return Err(format!("rdb_id '{}' is not a MySQL or SQLite connection", connection.name));
    }
    Ok(())
}

fn validate_weaviate_connection_schema(
    connections: &[ConnectionConfig],
    connection_id: Option<&str>,
    expected_schema: WeaviateCollectionSchema,
    field_name: &str,
) -> Result<(), String> {
    let Some(connection_id) = connection_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let connection = connections
        .iter()
        .find(|item| item.id == connection_id || item.config_id == connection_id)
        .ok_or_else(|| format!("{field_name} '{}' not found", connection_id))?;
    let ConnectionKind::Weaviate(weaviate) = &connection.kind else {
        return Err(format!("{field_name} '{}' is not a weaviate connection", connection.name));
    };
    if weaviate.collection_schema != expected_schema {
        return Err(format!(
            "{} '{}' schema mismatch: expected {:?}, got {:?}",
            field_name, connection.name, expected_schema, weaviate.collection_schema
        ));
    }
    Ok(())
}

// Avatar upload and retrieval handlers

use salvo::http::form::FormData;
use salvo::http::StatusCode;

#[derive(Serialize)]
struct AvatarUploadResponse {
    avatar_id: String,
}

/// Upload avatar image - stores in database and returns avatar_id
#[handler]
pub async fn upload_avatar(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap().clone();

    // Parse multipart form data
    let form_data = match req.form_data().await {
        Ok(data) => data,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(e.to_string());
            return;
        }
    };

    // Get file from form data
    let file = match form_data.files.get("file") {
        Some(file) => file,
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render("Missing file field");
            return;
        }
    };

    // Validate mime type
    let mime_type = file
        .content_type()
        .map(|m| m.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    if !mime_type.starts_with("image/") {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render("Only image files are allowed");
        return;
    }

    // Read file content
    let image_data = match std::fs::read(file.path()) {
        Ok(data) => data,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(format!("Failed to read file: {}", e));
            return;
        }
    };

    // Validate file size (30MB)
    const MAX_SIZE: usize = 30 * 1024 * 1024;
    if image_data.len() > MAX_SIZE {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render("File size exceeds 30MB limit");
        return;
    }

    // Generate avatar ID
    let avatar_id = Uuid::new_v4().to_string();

    // Save to database
    match save_avatar_to_db(&state, &avatar_id, "", &mime_type, &image_data).await {
        Ok(()) => {
            info!("[avatar] uploaded avatar id={}", avatar_id);
            res.render(Json(AvatarUploadResponse { avatar_id }));
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(format!("Failed to save avatar: {}", e));
        }
    }
}

/// Get avatar image by ID
#[handler]
pub async fn get_avatar(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap().clone();
    let avatar_id = req.param::<String>("avatar_id").unwrap_or_default();

    if avatar_id.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render("Missing avatar_id");
        return;
    }

    match load_avatar_from_db(&state, &avatar_id).await {
        Ok(Some(avatar)) => {
            res.add_header("Content-Type", avatar.mime_type, true).ok();
            res.write_body(avatar.image_data).ok();
        }
        Ok(None) => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render("Avatar not found");
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(format!("Failed to load avatar: {}", e));
        }
    }
}

/// Delete avatar by agent ID (called when agent is deleted or avatar is changed)
async fn delete_avatar_by_agent_id(state: &Arc<AppState>, agent_id: &str) -> Result<(), String> {
    // Get task_db_connection_id from connections
    let connections = system_config::load_connections().map_err(|e| e.to_string())?;
    let task_db_connection_id = connections
        .iter()
        .find(|c| matches!(c.kind, ConnectionKind::Mysql(_) | ConnectionKind::Sqlite(_)))
        .map(|c| c.config_id.clone());

    let Some(db_id) = task_db_connection_id else {
        // No database connection, skip
        return Ok(());
    };

    // Build database connection
    let db_config = connections
        .iter()
        .find(|c| c.config_id == db_id)
        .ok_or_else(|| format!("Database connection '{}' not found", db_id))?;

    // Execute delete query based on connection type
    match &db_config.kind {
        ConnectionKind::Mysql(mysql) => {
            let mysql_ref = storage_handler::mysql::build_mysql_ref(&mysql.url)
                .await
                .map_err(|e| format!("Failed to build MySQL ref: {}", e))?;
            let pool =
                storage_handler::mysql::get_pool(&mysql_ref).ok_or_else(|| "Failed to get MySQL pool".to_string())?;
            sqlx::query("DELETE FROM agent_avatar WHERE agent_id = ?")
                .bind(agent_id)
                .execute(pool)
                .await
                .map_err(|e| format!("Failed to delete avatar: {}", e))?;
        }
        ConnectionKind::Sqlite(sqlite) => {
            let db_path = std::path::Path::new(&sqlite.path);
            let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
            let pool = sqlx::sqlite::SqlitePool::connect(&db_url)
                .await
                .map_err(|e| format!("Failed to connect to SQLite: {}", e))?;
            sqlx::query("DELETE FROM agent_avatar WHERE agent_id = ?")
                .bind(agent_id)
                .execute(&pool)
                .await
                .map_err(|e| format!("Failed to delete avatar: {}", e))?;
        }
        _ => {}
    }

    Ok(())
}

/// Save avatar to database
async fn save_avatar_to_db(
    _state: &Arc<AppState>,
    avatar_id: &str,
    agent_id: &str,
    mime_type: &str,
    image_data: &[u8],
) -> Result<(), String> {
    // Get first available database connection
    let connections = system_config::load_connections().map_err(|e| e.to_string())?;
    let db_config = connections
        .iter()
        .find(|c| matches!(c.kind, ConnectionKind::Mysql(_) | ConnectionKind::Sqlite(_)));

    let Some(db_config) = db_config else {
        return Err("No database connection available for avatar storage".to_string());
    };

    // Execute insert query based on connection type
    match &db_config.kind {
        ConnectionKind::Mysql(mysql) => {
            let mysql_ref = mysql::build_mysql_ref(&mysql.url)
                .await
                .map_err(|e| format!("Failed to build MySQL ref: {}", e))?;
            let pool = mysql::get_pool(&mysql_ref).ok_or_else(|| "Failed to get MySQL pool".to_string())?;
            sqlx::query(
                "INSERT INTO agent_avatar (id, agent_id, mime_type, image_data, created_at, updated_at) 
                 VALUES (?, ?, ?, ?, NOW(), NOW()) 
                 ON DUPLICATE KEY UPDATE 
                 agent_id = VALUES(agent_id), 
                 mime_type = VALUES(mime_type), 
                 image_data = VALUES(image_data), 
                 updated_at = NOW()",
            )
            .bind(avatar_id)
            .bind(agent_id)
            .bind(mime_type)
            .bind(image_data)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to save avatar: {}", e))?;
        }
        ConnectionKind::Sqlite(sqlite) => {
            let db_path = std::path::Path::new(&sqlite.path);
            let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
            let pool = sqlx::sqlite::SqlitePool::connect(&db_url)
                .await
                .map_err(|e| format!("Failed to connect to SQLite: {}", e))?;
            sqlx::query(
                "INSERT INTO agent_avatar (id, agent_id, mime_type, image_data, created_at, updated_at) 
                 VALUES (?, ?, ?, ?, datetime('now'), datetime('now')) 
                 ON CONFLICT(id) DO UPDATE SET 
                 agent_id = excluded.agent_id, 
                 mime_type = excluded.mime_type, 
                 image_data = excluded.image_data, 
                 updated_at = datetime('now')",
            )
            .bind(avatar_id)
            .bind(agent_id)
            .bind(mime_type)
            .bind(image_data)
            .execute(&pool)
            .await
            .map_err(|e| format!("Failed to save avatar: {}", e))?;
        }
        _ => return Err("Unsupported database type".to_string()),
    }

    Ok(())
}

/// Avatar data structure
struct AvatarData {
    mime_type: String,
    image_data: Vec<u8>,
}

/// Load avatar from database
async fn load_avatar_from_db(_state: &Arc<AppState>, avatar_id: &str) -> Result<Option<AvatarData>, String> {
    // Get first available database connection
    let connections = system_config::load_connections().map_err(|e| e.to_string())?;
    let db_config = connections
        .iter()
        .find(|c| matches!(c.kind, ConnectionKind::Mysql(_) | ConnectionKind::Sqlite(_)));

    let Some(db_config) = db_config else {
        return Ok(None);
    };

    // Execute select query based on connection type
    let result: Option<(String, Vec<u8>)> = match &db_config.kind {
        ConnectionKind::Mysql(mysql) => {
            let mysql_ref = mysql::build_mysql_ref(&mysql.url)
                .await
                .map_err(|e| format!("Failed to build MySQL ref: {}", e))?;
            let pool = mysql::get_pool(&mysql_ref).ok_or_else(|| "Failed to get MySQL pool".to_string())?;
            sqlx::query_as::<_, (String, Vec<u8>)>("SELECT mime_type, image_data FROM agent_avatar WHERE id = ?")
                .bind(avatar_id)
                .fetch_optional(pool)
                .await
                .map_err(|e| format!("Failed to load avatar: {}", e))?
        }
        ConnectionKind::Sqlite(sqlite) => {
            let db_path = std::path::Path::new(&sqlite.path);
            let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
            let pool = sqlx::sqlite::SqlitePool::connect(&db_url)
                .await
                .map_err(|e| format!("Failed to connect to SQLite: {}", e))?;
            sqlx::query_as::<_, (String, Vec<u8>)>("SELECT mime_type, image_data FROM agent_avatar WHERE id = ?")
                .bind(avatar_id)
                .fetch_optional(&pool)
                .await
                .map_err(|e| format!("Failed to load avatar: {}", e))?
        }
        _ => return Ok(None),
    };

    Ok(result.map(|(mime_type, image_data)| AvatarData { mime_type, image_data }))
}
