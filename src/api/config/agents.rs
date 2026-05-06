use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use storage_handler::{ConnectionConfig, ConnectionKind};
use uuid::Uuid;

use ims_bot_adapter::{fetch_login_info, parse_ims_bot_adapter_connection, qq_avatar_url};
use log::{info, warn};

use crate::api::state::{AppState, TaskStatus};
use crate::api::ws::{ServerMessage, WsBroadcast};
use crate::service::AgentRuntimeInfo;
use crate::system_config;
use zihuan_llm::system_config::{AgentConfig, AgentToolConfig, AgentType, QqChatAgentConfig};

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

/// Register the agent as a task entry, start it, and wire up lifecycle callbacks.
/// The stop_flag in the task entry is watched: when set, `agent_manager.stop_agent()` is called.
pub async fn start_agent_with_task(
    state: Arc<AppState>,
    broadcast_tx: WsBroadcast,
    agent: AgentConfig,
    connections: Vec<ConnectionConfig>,
    user_ip: Option<String>,
) {
    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();

    let stop_flag = Arc::new(AtomicBool::new(false));
    let task_id = state.tasks.lock().unwrap().add_agent_task(
        agent_id.clone(),
        agent_name.clone(),
        user_ip,
        Arc::clone(&stop_flag),
    );

    let _ = broadcast_tx.send(ServerMessage::TaskStarted {
        task_id: task_id.clone(),
        graph_name: agent_name.clone(),
        graph_session_id: agent_id.clone(),
    });

    // Watcher: when stop_flag is set via /api/tasks/{id}/stop, forward to agent_manager.
    let agent_stopped = Arc::new(AtomicBool::new(false));
    {
        let state_w = Arc::clone(&state);
        let agent_id_w = agent_id.clone();
        let stop_flag_w = Arc::clone(&stop_flag);
        let agent_stopped_w = Arc::clone(&agent_stopped);
        tokio::spawn(async move {
            loop {
                if agent_stopped_w.load(Ordering::Relaxed) {
                    break;
                }
                if stop_flag_w.load(Ordering::Relaxed) {
                    let _ = state_w.agent_manager.stop_agent(&agent_id_w).await;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        });
    }

    let state_finish = Arc::clone(&state);
    let broadcast_finish = broadcast_tx.clone();
    let task_id_finish = task_id.clone();
    let on_finish: Box<dyn FnOnce(bool, Option<String>) + Send + 'static> =
        Box::new(move |success, error_message| {
            agent_stopped.store(true, Ordering::Relaxed);
            let status = if success {
                TaskStatus::Success
            } else if error_message.is_some() {
                TaskStatus::Failed
            } else {
                TaskStatus::Stopped
            };
            state_finish.tasks.lock().unwrap().finish_task(
                &task_id_finish,
                status,
                error_message.clone(),
            );
            let _ = broadcast_finish.send(ServerMessage::TaskFinished {
                task_id: task_id_finish,
                success,
                error: error_message,
            });
        });

    if let Err(err) = state
        .agent_manager
        .start_agent(agent, connections, Some(on_finish), Some(task_id.clone()))
        .await
    {
        state.tasks.lock().unwrap().finish_task(
            &task_id,
            TaskStatus::Failed,
            Some(err.to_string()),
        );
        let _ = broadcast_tx.send(ServerMessage::TaskFinished {
            task_id,
            success: false,
            error: Some(err.to_string()),
        });
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

    let agent = AgentConfig {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        agent_type: body.agent_type,
        enabled: body.enabled,
        auto_start: body.auto_start,
        is_default: body.is_default,
        updated_at: now_rfc3339(),
        tools: body.tools,
    };
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

    let user_ip = req
        .header::<String>("x-forwarded-for")
        .or_else(|| Some(req.remote_addr().to_string()));
    info!(
        "[agents] starting agent '{}' (id={}) from {}",
        agent.name,
        id,
        user_ip.as_deref().unwrap_or("unknown")
    );
    start_agent_with_task(state.clone(), broadcast_tx, agent, connections, user_ip).await;
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
