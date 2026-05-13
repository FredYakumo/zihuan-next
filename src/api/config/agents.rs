use std::sync::Arc;

use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use storage_handler::{ConnectionConfig, ConnectionKind, WeaviateCollectionSchema};
use uuid::Uuid;
use zihuan_core::task_context::{
    AgentTaskHandle, AgentTaskRequest, AgentTaskResult, AgentTaskRuntime, AgentTaskStatus,
};

use ims_bot_adapter::{fetch_login_info, parse_ims_bot_adapter_connection, qq_avatar_url};
use log::{info, warn};

use crate::api::state::{AppState, TaskStatus};
use crate::api::ws::{ServerMessage, WsBroadcast};
use crate::service::AgentRuntimeInfo;
use crate::system_config;
use zihuan_core::agent_config::QqChatAgentConfig;
use model_inference::system_config::{AgentConfig, AgentToolConfig, AgentType};

use super::{
    now_rfc3339, ok_response, render_bad_request, render_internal_error, render_not_found,
    render_unprocessable_entity,
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

struct DefaultAgentTaskRuntime {
    state: Arc<AppState>,
    broadcast_tx: WsBroadcast,
}

impl AgentTaskRuntime for DefaultAgentTaskRuntime {
    fn start_task(&self, request: AgentTaskRequest) -> Arc<AgentTaskHandle> {
        let task_id = self.state.tasks.lock().unwrap().add_agent_response_task(
            request.agent_id.clone(),
            request.task_name.clone(),
            request.user_ip.clone(),
        );

        let _ = self.broadcast_tx.send(ServerMessage::TaskStarted {
            task_id: task_id.clone(),
            graph_name: request.task_name,
            graph_session_id: request.agent_id,
        });

        let state = Arc::clone(&self.state);
        let broadcast_tx = self.broadcast_tx.clone();
        AgentTaskHandle::new(task_id.clone(), move |result: AgentTaskResult| {
            let status = match result.status.unwrap_or_else(|| {
                if result.error_message.is_some() {
                    AgentTaskStatus::Failed
                } else {
                    AgentTaskStatus::Success
                }
            }) {
                AgentTaskStatus::Success => TaskStatus::Success,
                AgentTaskStatus::Failed => TaskStatus::Failed,
                AgentTaskStatus::Stopped => TaskStatus::Stopped,
            };

            state.tasks.lock().unwrap().finish_task(
                &task_id,
                status.clone(),
                result.error_message.clone(),
                result.result_summary.clone(),
            );

            match status {
                TaskStatus::Stopped => {
                    let _ = broadcast_tx.send(ServerMessage::TaskStopped {
                        task_id: task_id.clone(),
                    });
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
}

pub fn build_agent_task_runtime(
    state: Arc<AppState>,
    broadcast_tx: WsBroadcast,
) -> Arc<dyn AgentTaskRuntime> {
    Arc::new(DefaultAgentTaskRuntime {
        state,
        broadcast_tx,
    })
}

pub async fn start_agent_runtime(
    state: Arc<AppState>,
    broadcast_tx: WsBroadcast,
    agent: AgentConfig,
    connections: Vec<ConnectionConfig>,
) {
    let agent_name = agent.name.clone();
    let on_finish: Box<dyn FnOnce(bool, Option<String>) + Send + 'static> =
        Box::new(move |success, error_message| {
            if !success {
                log::warn!(
                    "[agents] agent '{}' stopped with error: {}",
                    agent_name,
                    error_message.unwrap_or_else(|| "stopped".to_string())
                );
            }
        });

    let task_runtime = build_agent_task_runtime(state.clone(), broadcast_tx.clone());
    if let Err(err) = state
        .agent_manager
        .start_agent(agent, connections, Some(on_finish), Some(task_runtime))
        .await
    {
        log::error!("[agents] failed to start agent: {}", err);
    }
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
}

#[handler]
pub async fn list_agents(_req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot
        .obtain::<std::sync::Arc<crate::api::state::AppState>>()
        .unwrap();
    match system_config::load_agents() {
        Ok(agents) => {
            let connections = match system_config::load_connections() {
                Ok(connections) => connections,
                Err(err) => return render_internal_error(res, err),
            };

            let mut items = Vec::with_capacity(agents.len());
            for agent in agents {
                let qq_chat_profile = match &agent.agent_type {
                    AgentType::QqChat(config) => {
                        resolve_qq_chat_profile(&connections, config).await
                    }
                    AgentType::HttpStream(_) => None,
                };

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
    config: &QqChatAgentConfig,
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
    let response = agent.clone();

    match system_config::save_agents(agents) {
        Ok(()) => {
            info!(
                "[agents] updated agent '{}' (id={})",
                response.name, response.id
            );
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

    info!("[agents] starting agent '{}' (id={})", agent.name, id,);
    start_agent_runtime(state.clone(), broadcast_tx, agent, connections).await;
    res.render(Json(serde_json::json!({
        "ok": true,
        "runtime": state.agent_manager.runtime_info(&id),
    })));
}

#[handler]
pub async fn stop_agent(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot
        .obtain::<std::sync::Arc<crate::api::state::AppState>>()
        .unwrap();
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

fn validate_agent_connection_schemas(
    agent_type: &AgentType,
    connections: &[ConnectionConfig],
) -> Result<(), String> {
    let AgentType::QqChat(config) = agent_type else {
        return Ok(());
    };
    validate_weaviate_connection_schema(
        connections,
        config.weaviate_image_connection_id.as_deref(),
        WeaviateCollectionSchema::ImageSemantic,
        "weaviate_image_connection_id",
    )
}

fn validate_weaviate_connection_schema(
    connections: &[ConnectionConfig],
    connection_id: Option<&str>,
    expected_schema: WeaviateCollectionSchema,
    field_name: &str,
) -> Result<(), String> {
    let Some(connection_id) = connection_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let connection = connections
        .iter()
        .find(|item| item.id == connection_id || item.config_id == connection_id)
        .ok_or_else(|| format!("{field_name} '{}' not found", connection_id))?;
    let ConnectionKind::Weaviate(weaviate) = &connection.kind else {
        return Err(format!(
            "{field_name} '{}' is not a weaviate connection",
            connection.name
        ));
    };
    if weaviate.collection_schema != expected_schema {
        return Err(format!(
            "{} '{}' schema mismatch: expected {:?}, got {:?}",
            field_name, connection.name, expected_schema, weaviate.collection_schema
        ));
    }
    Ok(())
}
