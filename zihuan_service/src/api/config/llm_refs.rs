use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::system_config;
use log::{info, warn};
use zihuan_core::config::llm_refs::LlmRefConfig;
use zihuan_core::connection::utils::utilsdep_analyze::ConfigDependencyKind;
use zihuan_core::model_inference::model_config::ModelRefSpec;
use zihuan_core::model_inference::nn::embedding::embedding_runtime_manager::close_runtime_embedding_instances_for_config;

use super::{now_rfc3339, render_bad_request, render_internal_error, render_not_found};

#[derive(Deserialize)]
pub struct CreateLlmRefRequest {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub model: ModelRefSpec,
}

#[derive(Deserialize)]
pub struct UpdateLlmRefRequest {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub model: ModelRefSpec,
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
        config_id: String::new(),
        name: body.name,
        enabled: body.enabled,
        model: body.model,
        updated_at: now_rfc3339(),
    };
    let mut llm_ref = llm_ref;
    llm_ref.config_id = llm_ref.id.clone();
    llm_refs.push(llm_ref.clone());

    match system_config::save_llm_refs(llm_refs) {
        Ok(()) => {
            info!("[llm_refs] created LLM config '{}' (id={})", llm_ref.name, llm_ref.id);
            res.render(Json(llm_ref));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn update_llm_ref(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<std::sync::Arc<crate::api::state::AppState>>().unwrap().clone();
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
    llm_ref.model = body.model;
    llm_ref.updated_at = now_rfc3339();
    let response = llm_ref.clone();

    // A running role service keeps the old LLM config until restarted; report
    // the affected services so the frontend can offer to restart them.
    let affected_running_services = super::dependency_check::affected_running_services(
        &state,
        ConfigDependencyKind::LlmRef,
        &id,
    );

    match system_config::save_llm_refs(llm_refs) {
        Ok(()) => {
            info!(
                "[llm_refs] updated LLM config '{}' (id={}), {} running role service(s) affected",
                response.name,
                response.id,
                affected_running_services.len()
            );
            let _ = close_runtime_embedding_instances_for_config(&id);
            res.render(Json(serde_json::json!({
                "llm_ref": response,
                "affected_running_services": affected_running_services,
            })));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn delete_llm_ref(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<std::sync::Arc<crate::api::state::AppState>>().unwrap().clone();
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

    // Analyze before deleting: a deleted LLM config cannot be restarted onto
    // the services referencing it, so they are stopped after the delete.
    let affected_running_services = super::dependency_check::affected_running_services(
        &state,
        ConfigDependencyKind::LlmRef,
        &id,
    );

    match system_config::save_llm_refs(llm_refs) {
        Ok(()) => {
            let _ = close_runtime_embedding_instances_for_config(&id);
            for service in &affected_running_services {
                info!(
                    "[llm_refs] stopping running role service '{}' (id={}) after deleting LLM config '{}'",
                    service.name, service.id, id
                );
                if let Err(err) = state.role_service_manager.stop_role_service(&service.id).await {
                    warn!(
                        "[llm_refs] failed to stop role service '{}' after deleting LLM config '{}': {}",
                        service.name, id, err
                    );
                }
            }
            info!(
                "[llm_refs] deleted LLM config (id={}), {} running role service(s) affected",
                id,
                affected_running_services.len()
            );
            res.render(Json(serde_json::json!({
                "ok": true,
                "affected_running_services": affected_running_services,
            })));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[cfg(test)]
mod tests;
