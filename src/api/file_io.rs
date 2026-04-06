use std::sync::Arc;

use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;

use super::state::AppState;


#[derive(Deserialize)]
pub struct OpenFileRequest {
    pub path: String,
}

#[handler]
pub async fn open_file(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let body: OpenFileRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    let result = zihuan_node::load_graph_definition_from_json_with_migration(&body.path);
    match result {
        Ok(loaded) => {
            let zihuan_node::LoadedGraphDefinition { mut graph, migrated } = loaded;
            zihuan_node::ensure_positions(&mut graph);
            let session_id = uuid::Uuid::new_v4().to_string();
            let session = super::state::GraphSession::new(session_id.clone(), graph, Some(body.path));
            let mut sessions = state.sessions.write().unwrap();
            sessions.insert(session_id.clone(), session);
            res.render(Json(serde_json::json!({
                "session_id": session_id,
                "migrated": migrated,
            })));
        }
        Err(e) => {
            res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
        }
    }
}


#[derive(Deserialize)]
pub struct SaveFileRequest {
    pub path: Option<String>,
}

#[handler]
pub async fn save_file(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let graph_id = req.param::<String>("id").unwrap_or_default();
    let body: SaveFileRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    let mut sessions = state.sessions.write().unwrap();
    let session = match sessions.get_mut(&graph_id) {
        Some(s) => s,
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
            return;
        }
    };

    let save_path = body
        .path
        .clone()
        .or_else(|| session.file_path.clone());

    let save_path = match save_path {
        Some(p) => p,
        None => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({
                "error": "No file path provided and graph has no saved path"
            })));
            return;
        }
    };

    match serde_json::to_string_pretty(&session.graph) {
        Ok(json_str) => match std::fs::write(&save_path, json_str) {
            Ok(()) => {
                session.file_path = Some(save_path.clone());
                session.dirty = false;
                res.render(Json(serde_json::json!({"ok": true, "path": save_path})));
            }
            Err(e) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Json(serde_json::json!({"error": e.to_string()})));
            }
        },
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
        }
    }
}


#[handler]
pub async fn upload_graph(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let body_bytes = match req.payload().await {
        Ok(b) => b,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    let graph: zihuan_node::graph_io::NodeGraphDefinition =
        match serde_json::from_slice(&body_bytes) {
            Ok(v) => v,
            Err(e) => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(Json(serde_json::json!({"error": e.to_string()})));
                return;
            }
        };

    let mut graph = graph;
    zihuan_node::ensure_positions(&mut graph);
    let session_id = uuid::Uuid::new_v4().to_string();
    let session = super::state::GraphSession::new(session_id.clone(), graph, None);
    state
        .sessions
        .write()
        .unwrap()
        .insert(session_id.clone(), session);

    res.render(Json(serde_json::json!({"session_id": session_id})));
}


#[handler]
pub async fn download_graph(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    let sessions = state.sessions.read().unwrap();
    match sessions.get(&id) {
        Some(s) => {
            let json = serde_json::to_string_pretty(&s.graph).unwrap_or_default();
            let filename = s
                .file_path
                .as_deref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "graph.json".to_string());
            res.add_header(
                "Content-Disposition",
                format!("attachment; filename=\"{}\"", filename),
                true,
            )
            .unwrap();
            res.add_header("Content-Type", "application/json", true).unwrap();
            res.write_body(json.into_bytes());
        }
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}
