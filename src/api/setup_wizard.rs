use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::api::config::{now_rfc3339, ok_response, render_internal_error};
use crate::api::state::AppState;
use crate::setup_orchestrator::{
    LlmSetupConfig, NapCatSetupConfig, SetupOptions, SetupOrchestrator, SetupRole,
};
use zihuan_core::setup_wizard::{clear_setup_wizard_state, load_setup_wizard_state, save_setup_wizard_state};

#[derive(Deserialize, Clone)]
pub struct ExecuteSetupRequest {
    pub mode: SetupMode,
    #[serde(default)]
    pub role: Option<SetupRole>,
    #[serde(default)]
    pub options: SetupOptions,
    #[serde(default)]
    pub llm_config: Option<LlmSetupConfig>,
    #[serde(default)]
    pub napcat_config: Option<NapCatSetupConfig>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum SetupMode {
    RoleBased,
    Detailed,
    Skip,
}

#[derive(Serialize)]
pub struct ExecuteSetupResponse {
    pub accepted: bool,
    pub task_id: String,
}

#[handler]
pub async fn get_setup_status(_req: &mut Request, res: &mut Response) {
    match load_setup_wizard_state() {
        Ok(state) => res.render(Json(state)),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn post_skip_setup(_req: &mut Request, res: &mut Response) {
    let mut state = match load_setup_wizard_state() {
        Ok(state) => state,
        Err(err) => return render_internal_error(res, err),
    };
    state.skipped = true;
    state.completed_at = Some(now_rfc3339());
    match save_setup_wizard_state(&state) {
        Ok(()) => res.render(Json(ok_response())),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn post_reset_setup(_req: &mut Request, res: &mut Response) {
    match clear_setup_wizard_state() {
        Ok(()) => res.render(Json(ok_response())),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn get_environment_info(_req: &mut Request, res: &mut Response) {
    let env = crate::setup_orchestrator::detect_environment().await;
    res.render(Json(env));
}

#[handler]
pub async fn post_execute_setup(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let body: ExecuteSetupRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({ "error": err.to_string() })));
            return;
        }
    };

    if matches!(body.mode, SetupMode::Skip) {
        let mut state = match load_setup_wizard_state() {
            Ok(state) => state,
            Err(err) => return render_internal_error(res, err),
        };
        state.skipped = true;
        state.completed_at = Some(now_rfc3339());
        match save_setup_wizard_state(&state) {
            Ok(()) => res.render(Json(ok_response())),
            Err(err) => render_internal_error(res, err),
        }
        return;
    }

    let role = match body.role {
        Some(role) => role,
        None => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({ "error": "role is required for role-based setup" })));
            return;
        }
    };

    let llm_config = match body.llm_config {
        Some(config) => config,
        None => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({ "error": "llm_config is required" })));
            return;
        }
    };

    let state = match depot.obtain::<Arc<AppState>>() {
        Ok(state) => state.clone(),
        Err(_) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({ "error": "failed to obtain app state" })));
            return;
        }
    };

    let task_id = uuid::Uuid::new_v4().to_string();
    let (progress_tx, _progress_rx) = tokio::sync::broadcast::channel(256);
    let orchestrator = SetupOrchestrator::new(task_id.clone(), progress_tx.clone());

    {
        let mut guard = state.setup_tasks.lock().unwrap();
        guard.insert(task_id.clone(), progress_tx);
    }

    let napcat_config = body.napcat_config;
    let options = body.options;
    let task_id_for_spawn = task_id.clone();

    tokio::spawn(async move {
        let result = orchestrator.run(role, options, llm_config, napcat_config).await;
        if let Err(err) = result {
            log::warn!("[setup_orchestrator] task {} failed: {}", task_id_for_spawn, err);
        }
        // Keep the broadcast channel alive for 60s so late SSE clients can still connect.
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        let _ = state.setup_tasks.lock().unwrap().remove(&task_id_for_spawn);
    });

    res.render(Json(ExecuteSetupResponse { accepted: true, task_id }));
}

#[handler]
pub async fn stream_setup_progress(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let task_id = req.query::<String>("task_id").unwrap_or_default();

    let state = match depot.obtain::<Arc<AppState>>() {
        Ok(state) => state.clone(),
        Err(_) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({ "error": "failed to obtain app state" })));
            return;
        }
    };

    let mut rx = {
        let guard = state.setup_tasks.lock().unwrap();
        match guard.get(&task_id) {
            Some(tx) => tx.subscribe(),
            None => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(Json(serde_json::json!({ "error": "task not found" })));
                return;
            }
        }
    };

    let (mut sender, body) = salvo::http::ResBody::channel();
    res.headers_mut().insert("content-type", "text/event-stream".parse().unwrap());
    res.headers_mut().insert("cache-control", "no-cache".parse().unwrap());
    res.body = body;

    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let data = format!("data: {}\n\n", serde_json::to_string(&event).unwrap_or_default());
            if sender.send_data(data).await.is_err() {
                break;
            }
            if event.step == "finished" || event.status == "error" {
                break;
            }
        }
    });
}
