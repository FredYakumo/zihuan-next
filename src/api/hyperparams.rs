use std::sync::Arc;

use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};

use super::state::AppState;
use crate::util::hyperparam_store;

// ─── Hyperparameters ──────────────────────────────────────────────────────────

#[handler]
pub async fn get_hyperparameters(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let sessions = state.sessions.read().unwrap();
    match sessions.get(&id) {
        Some(s) => {
            let values = s
                .file_path
                .as_deref()
                .map(|fp| {
                    hyperparam_store::load_hyperparameter_values(std::path::Path::new(fp), &s.graph)
                })
                .unwrap_or_default();

            res.render(Json(serde_json::json!({
                "hyperparameters": s.graph.hyperparameters,
                "hyperparameter_groups": s.graph.hyperparameter_groups,
                "values": values,
            })));
        }
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateHyperparametersRequest {
    pub values: std::collections::HashMap<String, serde_json::Value>,
}

#[handler]
pub async fn update_hyperparameter_values(
    req: &mut Request,
    res: &mut Response,
    depot: &mut Depot,
) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let body: UpdateHyperparametersRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    let sessions = state.sessions.read().unwrap();
    let session = match sessions.get(&id) {
        Some(s) => s,
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
            return;
        }
    };

    if let Some(fp) = &session.file_path {
        match hyperparam_store::save_hyperparameter_values(
            std::path::Path::new(fp),
            &session.graph,
            &body.values,
        ) {
            Ok(()) => res.render(Json(serde_json::json!({"ok": true}))),
            Err(e) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Json(serde_json::json!({"error": e.to_string()})));
            }
        }
    } else {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({
            "error": "Graph has no file path; save the graph first before persisting hyperparameters"
        })));
    }
}

// ─── Variables ────────────────────────────────────────────────────────────────

#[handler]
pub async fn get_variables(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let sessions = state.sessions.read().unwrap();
    match sessions.get(&id) {
        Some(s) => res.render(Json(&s.graph.variables)),
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateVariablesRequest {
    pub variables: Vec<zihuan_graph_engine::graph_io::GraphVariable>,
}

#[handler]
pub async fn update_variables(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let body: UpdateVariablesRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    let mut sessions = state.sessions.write().unwrap();
    match sessions.get_mut(&id) {
        Some(s) => {
            s.graph.variables = body.variables;
            s.dirty = true;
            res.render(Json(serde_json::json!({"ok": true})));
        }
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}
