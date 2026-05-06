use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::system_config;
use log::{info, warn};
use zihuan_llm::system_config::{AgentConfig, AgentType, LlmRefConfig, LlmServiceConfig};
use zihuan_service::AgentRuntimeStatus;

use super::{
    now_rfc3339, ok_response, render_bad_request, render_internal_error, render_not_found,
};

#[derive(Deserialize)]
pub struct CreateLlmRefRequest {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub llm: LlmServiceConfig,
}

#[derive(Deserialize)]
pub struct UpdateLlmRefRequest {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub llm: LlmServiceConfig,
}

#[handler]
pub async fn list_llm_refs(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    match system_config::load_llm_refs() {
        Ok(llm_refs) => res.render(Json(llm_refs)),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn create_llm_ref(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let body: CreateLlmRefRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let mut llm_refs = match system_config::load_llm_refs() {
        Ok(llm_refs) => llm_refs,
        Err(err) => return render_internal_error(res, err),
    };

    let llm_ref = LlmRefConfig {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        enabled: body.enabled,
        llm: body.llm,
        updated_at: now_rfc3339(),
    };
    llm_refs.push(llm_ref.clone());

    match system_config::save_llm_refs(llm_refs) {
        Ok(()) => {
            info!(
                "[llm_refs] created LLM config '{}' (id={})",
                llm_ref.name, llm_ref.id
            );
            res.render(Json(llm_ref));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn update_llm_ref(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot
        .obtain::<std::sync::Arc<crate::api::state::AppState>>()
        .unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let body: UpdateLlmRefRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let mut llm_refs = match system_config::load_llm_refs() {
        Ok(llm_refs) => llm_refs,
        Err(err) => return render_internal_error(res, err),
    };

    let Some(llm_ref) = llm_refs.iter_mut().find(|item| item.id == id) else {
        return render_not_found(res, "LLM ref not found");
    };

    llm_ref.name = body.name;
    llm_ref.enabled = body.enabled;
    llm_ref.llm = body.llm;
    llm_ref.updated_at = now_rfc3339();
    let response = llm_ref.clone();

    match system_config::save_llm_refs(llm_refs) {
        Ok(()) => {
            info!(
                "[llm_refs] updated LLM config '{}' (id={})",
                response.name, response.id
            );
            hot_reload_agents_for_llm_ref(state.as_ref(), &id).await;
            res.render(Json(response));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn delete_llm_ref(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let id = req.param::<String>("id").unwrap_or_default();
    let mut llm_refs = match system_config::load_llm_refs() {
        Ok(llm_refs) => llm_refs,
        Err(err) => return render_internal_error(res, err),
    };
    let before = llm_refs.len();
    llm_refs.retain(|item| item.id != id);

    if before == llm_refs.len() {
        return render_not_found(res, "LLM ref not found");
    }

    match system_config::save_llm_refs(llm_refs) {
        Ok(()) => {
            info!("[llm_refs] deleted LLM config (id={})", id);
            res.render(Json(ok_response()));
        }
        Err(err) => render_internal_error(res, err),
    }
}

async fn hot_reload_agents_for_llm_ref(state: &crate::api::state::AppState, llm_ref_id: &str) {
    let agents = match system_config::load_agents() {
        Ok(agents) => agents,
        Err(err) => {
            warn!(
                "[llm_refs] updated llm_ref={} but failed to load agents for hot reload: {}",
                llm_ref_id, err
            );
            return;
        }
    };

    let connections = match system_config::load_connections() {
        Ok(connections) => connections,
        Err(err) => {
            warn!(
                "[llm_refs] updated llm_ref={} but failed to load connections for hot reload: {}",
                llm_ref_id, err
            );
            return;
        }
    };

    for agent in agents
        .into_iter()
        .filter(|agent| agent_uses_llm_ref(agent, llm_ref_id))
    {
        let runtime = state.agent_manager.runtime_info(&agent.id);
        if runtime.status != AgentRuntimeStatus::Running {
            continue;
        }

        info!(
            "[llm_refs] hot reloading running agent '{}' after llm_ref update: {}",
            agent.name, llm_ref_id
        );

        if let Err(err) = state
            .agent_manager
            .start_agent(agent.clone(), connections.clone(), None, None)
            .await
        {
            warn!(
                "[llm_refs] failed to hot reload agent '{}' for llm_ref={}: {}",
                agent.name, llm_ref_id, err
            );
        }
    }
}

fn agent_uses_llm_ref(agent: &AgentConfig, llm_ref_id: &str) -> bool {
    match &agent.agent_type {
        AgentType::QqChat(config) => config.llm_ref_id.as_deref() == Some(llm_ref_id),
        AgentType::HttpStream(config) => config.llm_ref_id.as_deref() == Some(llm_ref_id),
    }
}
