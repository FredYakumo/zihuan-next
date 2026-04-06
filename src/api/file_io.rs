use std::sync::Arc;

use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};

use super::state::AppState;

// ─── Workflows directory helpers ──────────────────────────────────────────────

/// Return a sorted list of `.json` filenames in the `workflow_set/` directory.
#[handler]
pub async fn list_workflows(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let dir = std::path::Path::new("workflow_set");
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            let mut names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("json") {
                        p.file_name()
                            .and_then(|s| s.to_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            names.sort();
            res.render(Json(serde_json::json!({ "files": names })));
        }
        Err(_) => {
            // Directory absent — return empty list
            res.render(Json(serde_json::json!({ "files": [] })));
        }
    }
}

/// WorkflowInfo returned by the detailed listing endpoint.
#[derive(Serialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub file: String,
    pub cover_url: Option<String>,
}

/// Return detailed workflow listing: name, json filename, optional cover URL.
#[handler]
pub async fn list_workflows_detailed(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let dir = std::path::Path::new("workflow_set");
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            res.render(Json(serde_json::json!({ "workflows": [] })));
            return;
        }
    };

    let mut workflows: Vec<WorkflowInfo> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                return None;
            }
            let stem = p.file_stem()?.to_str()?.to_string();
            let file = p.file_name()?.to_str()?.to_string();

            // Check for same-named cover image
            let cover_url = ["jpg", "jpeg", "png", "webp"].iter().find_map(|ext| {
                let img = dir.join(format!("{}.{}", stem, ext));
                if img.exists() {
                    Some(format!("/api/workflow_set/cover/{}.{}", stem, ext))
                } else {
                    None
                }
            });

            Some(WorkflowInfo { name: stem, file, cover_url })
        })
        .collect();

    workflows.sort_by(|a, b| a.name.cmp(&b.name));
    res.render(Json(serde_json::json!({ "workflows": workflows })));
}

/// Serve a cover image from the workflow_set/ directory.
#[handler]
pub async fn serve_workflow_cover(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let filename = req.param::<String>("filename").unwrap_or_default();
    // Security: reject any path traversal attempts
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        res.status_code(StatusCode::BAD_REQUEST);
        return;
    }
    let path = std::path::Path::new("workflow_set").join(&filename);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let mime = match ext.to_ascii_lowercase().as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "webp" => "image/webp",
                _ => "application/octet-stream",
            };
            res.headers_mut().insert(
                salvo::http::header::CONTENT_TYPE,
                mime.parse().unwrap(),
            );
            res.write_body(bytes).ok();
        }
        Err(_) => {
            res.status_code(StatusCode::NOT_FOUND);
        }
    }
}

#[derive(Deserialize)]
pub struct SaveToWorkflowsRequest {
    pub graph_id: String,
    pub name: String,
}

/// Save a session's graph into the `workflow_set/` directory.
#[handler]
pub async fn save_to_workflows(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let body: SaveToWorkflowsRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({ "error": e.to_string() })));
            return;
        }
    };

    // Serialise the graph under read lock, then release it before writing.
    let json_str = {
        let sessions = state.sessions.read().unwrap();
        match sessions.get(&body.graph_id) {
            Some(s) => match serde_json::to_string_pretty(&s.graph) {
                Ok(s) => s,
                Err(e) => {
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    res.render(Json(serde_json::json!({ "error": e.to_string() })));
                    return;
                }
            },
            None => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(Json(serde_json::json!({ "error": "Graph not found" })));
                return;
            }
        }
    };

    if let Err(e) = std::fs::create_dir_all("workflow_set") {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({ "error": e.to_string() })));
        return;
    }

    let filename = if body.name.ends_with(".json") {
        body.name.clone()
    } else {
        format!("{}.json", body.name)
    };
    let path = format!("workflow_set/{}", filename);

    match std::fs::write(&path, json_str) {
        Ok(()) => {
            let mut sessions = state.sessions.write().unwrap();
            if let Some(s) = sessions.get_mut(&body.graph_id) {
                s.file_path = Some(path.clone());
                s.dirty = false;
            }
            res.render(Json(serde_json::json!({ "ok": true, "path": path })));
        }
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({ "error": e.to_string() })));
        }
    }
}


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
    let body_bytes = match req.payload_with_max_size(usize::MAX).await {
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
