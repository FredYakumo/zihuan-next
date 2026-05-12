use std::path::Path;
use std::sync::Arc;

use chrono::{Datelike, Utc};
use log::{info, warn};
use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use storage_handler::{load_connections, ObjectStorageConfig};
use zihuan_llm::system_config::{load_agents, AgentConfig, AgentToolType, NodeGraphToolConfig};

use super::state::AppState;
use super::ws::WsBroadcast;

const IMAGE_UPLOAD_MAX_BYTES: usize = 16 * 1024 * 1024;
const LOCAL_IMAGE_UPLOAD_DIR: &str = "uploaded_images";
const TEXT_EMBEDDING_MODEL_DIR: &str = "models/text_embedding";

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

#[handler]
pub async fn list_text_embedding_models(
    _req: &mut Request,
    res: &mut Response,
    _depot: &mut Depot,
) {
    let dir = std::path::Path::new(TEXT_EMBEDDING_MODEL_DIR);
    let models = match std::fs::read_dir(dir) {
        Ok(entries) => {
            let mut names: Vec<String> = entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| {
                    let path = entry.path();
                    if !path.is_dir() {
                        return None;
                    }

                    let has_required_files = ["config.json", "tokenizer.json", "model.safetensors"]
                        .iter()
                        .all(|name| path.join(name).is_file());
                    if !has_required_files {
                        return None;
                    }

                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name.to_string())
                })
                .collect();
            names.sort();
            names
        }
        Err(_) => Vec::new(),
    };

    res.render(Json(serde_json::json!({ "models": models })));
}

/// WorkflowInfo returned by the detailed listing endpoint.
#[derive(Serialize)]
pub struct WorkflowInfo {
    pub name: String,
    pub file: String,
    pub cover_url: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub inputs: Vec<zihuan_graph_engine::function_graph::FunctionPortDef>,
    pub outputs: Vec<zihuan_graph_engine::function_graph::FunctionPortDef>,
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

            let (display_name, description, version, inputs, outputs) =
                zihuan_graph_engine::load_graph_definition_from_json_with_migration(&p)
                    .ok()
                    .map(|loaded| {
                        let graph = loaded.graph;
                        (
                            graph.metadata.name,
                            graph.metadata.description,
                            graph.metadata.version,
                            graph.graph_inputs,
                            graph.graph_outputs,
                        )
                    })
                    .unwrap_or((None, None, None, Vec::new(), Vec::new()));

            Some(WorkflowInfo {
                name: stem,
                file,
                cover_url,
                display_name,
                description,
                version,
                inputs,
                outputs,
            })
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
            res.headers_mut()
                .insert(salvo::http::header::CONTENT_TYPE, mime.parse().unwrap());
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

async fn hot_reload_agents_for_saved_graph(
    state: Arc<AppState>,
    broadcast_tx: WsBroadcast,
    saved_path: &str,
) {
    let agents = match load_agents() {
        Ok(agents) => agents,
        Err(err) => {
            warn!(
                "[workflow_save] saved graph '{}' but failed to load agents for hot reload: {}",
                saved_path, err
            );
            return;
        }
    };

    let connections = match load_connections() {
        Ok(connections) => connections,
        Err(err) => {
            warn!(
                "[workflow_save] saved graph '{}' but failed to load connections for hot reload: {}",
                saved_path, err
            );
            return;
        }
    };

    let normalized_saved_path = normalize_graph_path(saved_path);
    let saved_workflow_name = workflow_set_name_from_path(&normalized_saved_path);

    for agent in agents
        .into_iter()
        .filter(|agent| agent_uses_saved_graph(agent, &normalized_saved_path, saved_workflow_name))
    {
        let runtime = state.agent_manager.runtime_info(&agent.id);
        if runtime.status != zihuan_service::AgentRuntimeStatus::Running {
            continue;
        }

        info!(
            "[workflow_save] hot reloading running agent '{}' after graph save: {}",
            agent.name, saved_path
        );
        crate::api::config::agents::start_agent_runtime(
            state.clone(),
            broadcast_tx.clone(),
            agent,
            connections.clone(),
        )
        .await;
    }
}

fn agent_uses_saved_graph(
    agent: &AgentConfig,
    normalized_saved_path: &str,
    saved_workflow_name: Option<&str>,
) -> bool {
    agent.tools.iter().filter(|tool| tool.enabled).any(|tool| {
        let AgentToolType::NodeGraph(config) = &tool.tool_type;
        match config {
            NodeGraphToolConfig::FilePath { path, .. } => {
                normalize_graph_path(path) == normalized_saved_path
            }
            NodeGraphToolConfig::WorkflowSet { name, .. } => {
                saved_workflow_name.is_some_and(|workflow_name| workflow_name == name.trim())
            }
            NodeGraphToolConfig::InlineGraph { .. } => false,
        }
    })
}

fn normalize_graph_path(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

fn workflow_set_name_from_path(path: &str) -> Option<&str> {
    let normalized = path.strip_prefix("workflow_set/")?;
    Path::new(normalized).file_stem()?.to_str()
}

/// Save a session's graph into the `workflow_set/` directory.
#[handler]
pub async fn save_to_workflows(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let broadcast_tx = depot.obtain::<WsBroadcast>().unwrap().clone();
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
            {
                let mut sessions = state.sessions.write().unwrap();
                if let Some(s) = sessions.get_mut(&body.graph_id) {
                    s.file_path = Some(path.clone());
                    s.dirty = false;
                }
            }
            hot_reload_agents_for_saved_graph(state.clone(), broadcast_tx, &path).await;
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

    let result = zihuan_graph_engine::load_graph_definition_from_json_with_migration(&body.path);
    match result {
        Ok(loaded) => {
            let zihuan_graph_engine::LoadedGraphDefinition {
                mut graph,
                migrated,
            } = loaded;
            zihuan_graph_engine::graph_boundary::sync_root_graph_io(&mut graph);
            zihuan_graph_engine::ensure_positions(&mut graph);
            let session_id = uuid::Uuid::new_v4().to_string();
            let session =
                super::state::GraphSession::new(session_id.clone(), graph, Some(body.path));
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
    let broadcast_tx = depot.obtain::<WsBroadcast>().unwrap().clone();
    let graph_id = req.param::<String>("id").unwrap_or_default();
    let body: SaveFileRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    let (save_path, json_str) = {
        let mut sessions = state.sessions.write().unwrap();
        let session = match sessions.get_mut(&graph_id) {
            Some(s) => s,
            None => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(Json(serde_json::json!({"error": "Graph not found"})));
                return;
            }
        };

        let save_path = body.path.clone().or_else(|| session.file_path.clone());

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

        let json_str = match serde_json::to_string_pretty(&session.graph) {
            Ok(json_str) => json_str,
            Err(e) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Json(serde_json::json!({"error": e.to_string()})));
                return;
            }
        };

        (save_path, json_str)
    };

    match std::fs::write(&save_path, json_str) {
        Ok(()) => {
            {
                let mut sessions = state.sessions.write().unwrap();
                if let Some(session) = sessions.get_mut(&graph_id) {
                    session.file_path = Some(save_path.clone());
                    session.dirty = false;
                }
            }
            hot_reload_agents_for_saved_graph(state.clone(), broadcast_tx, &save_path).await;
            res.render(Json(serde_json::json!({"ok": true, "path": save_path})));
        }
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

    let graph: zihuan_graph_engine::graph_io::NodeGraphDefinition =
        match serde_json::from_slice(&body_bytes) {
            Ok(v) => v,
            Err(e) => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(Json(serde_json::json!({"error": e.to_string()})));
                return;
            }
        };

    let mut graph = graph;
    zihuan_graph_engine::graph_boundary::sync_root_graph_io(&mut graph);
    zihuan_graph_engine::ensure_positions(&mut graph);
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
            res.add_header("Content-Type", "application/json", true)
                .unwrap();
            res.write_body(json.into_bytes()).ok();
        }
        None => {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Graph not found"})));
        }
    }
}

#[derive(Serialize)]
pub struct UploadImageResponse {
    pub url: String,
    pub key: String,
    pub name: String,
}

#[handler]
pub async fn upload_image(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let content_type = req
        .headers()
        .get(salvo::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
        .unwrap_or_default();
    let mime = content_type
        .split(';')
        .next()
        .map(|part| part.trim().to_ascii_lowercase())
        .unwrap_or_default();

    if !mime.starts_with("image/") {
        res.status_code(StatusCode::UNSUPPORTED_MEDIA_TYPE);
        res.render(Json(serde_json::json!({
            "error": format!("expected image/* Content-Type, got '{}'", content_type)
        })));
        return;
    }

    let file_name_query = req.query::<String>("name");
    let raw_file_name = file_name_query
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let ext = mime.split('/').nth(1).unwrap_or("bin");
            format!("image.{ext}")
        });
    let safe_file_name = sanitize_upload_file_name(&raw_file_name);

    let bytes = match req.payload_with_max_size(IMAGE_UPLOAD_MAX_BYTES).await {
        Ok(b) => b.clone(),
        Err(e) => {
            res.status_code(StatusCode::PAYLOAD_TOO_LARGE);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    if bytes.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({"error": "empty request body"})));
        return;
    }

    let now = Utc::now();
    let rel_dir = format!("{}/{:02}", now.format("%Y"), now.month());
    let file_name = format!("{}_{}", uuid::Uuid::new_v4(), safe_file_name);

    if let Some(storage) = ObjectStorageConfig::from_env() {
        let key = format!("manual-uploads/{}/{}", rel_dir, file_name);
        match storage.put_object(&key, &mime, &bytes).await {
            Ok(url) => {
                res.render(Json(UploadImageResponse {
                    url,
                    key,
                    name: safe_file_name,
                }));
            }
            Err(e) => {
                res.status_code(StatusCode::BAD_GATEWAY);
                res.render(Json(serde_json::json!({"error": e.to_string()})));
            }
        }
        return;
    }

    let local_dir = std::path::Path::new(LOCAL_IMAGE_UPLOAD_DIR).join(&rel_dir);
    if let Err(e) = std::fs::create_dir_all(&local_dir) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({
            "error": format!("create upload dir failed: {e}")
        })));
        return;
    }
    let local_path = local_dir.join(&file_name);
    if let Err(e) = std::fs::write(&local_path, &bytes) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({
            "error": format!("write upload failed: {e}")
        })));
        return;
    }
    let key = format!("{}/{}", rel_dir, file_name);
    let url = format!("/api/uploaded-images/{}", key);
    res.render(Json(UploadImageResponse {
        url,
        key,
        name: safe_file_name,
    }));
}

#[handler]
pub async fn serve_uploaded_image(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let rel_path = req.param::<String>("rest").unwrap_or_default();
    if rel_path.is_empty()
        || rel_path.contains("..")
        || rel_path.starts_with('/')
        || rel_path.starts_with('\\')
    {
        res.status_code(StatusCode::BAD_REQUEST);
        return;
    }
    let path = std::path::Path::new(LOCAL_IMAGE_UPLOAD_DIR).join(&rel_path);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let mime = match ext.to_ascii_lowercase().as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "webp" => "image/webp",
                "gif" => "image/gif",
                "bmp" => "image/bmp",
                _ => "application/octet-stream",
            };
            res.headers_mut()
                .insert(salvo::http::header::CONTENT_TYPE, mime.parse().unwrap());
            res.write_body(bytes).ok();
        }
        Err(_) => {
            res.status_code(StatusCode::NOT_FOUND);
        }
    }
}

fn sanitize_upload_file_name(name: &str) -> String {
    let trimmed = name.trim();
    let base = std::path::Path::new(trimmed)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(trimmed);
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "image.bin".to_string()
    } else {
        cleaned
    }
}
