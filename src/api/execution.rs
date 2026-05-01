use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{error, info};
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::state::AppState;
use super::state::TaskStatus;
use super::ws::{ServerMessage, WsBroadcast};
use crate::util::hyperparam_store;

// ─── Execute graph ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RerunTaskRequest {
    pub user_ip: Option<String>,
}

#[handler]
pub async fn execute_graph(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let broadcast_tx = depot.obtain::<WsBroadcast>().unwrap().clone();
    let graph_id = req.param::<String>("id").unwrap_or_default();

    let (graph_def, graph_name, file_path, is_workflow_set) = {
        let sessions = state.sessions.read().unwrap();
        let session = match sessions.get(&graph_id) {
            Some(s) => s,
            None => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(Json(serde_json::json!({"error": "Graph not found"})));
                return;
            }
        };
        let file_path = session.file_path.clone();
        (
            session.graph.clone(),
            graph_display_name(file_path.as_deref()),
            file_path.clone(),
            is_workflow_set_path(file_path.as_deref()),
        )
    };

    // Apply hyperparameter values
    let hp_values = if let Some(fp) = &file_path {
        hyperparam_store::load_hyperparameter_values(std::path::Path::new(fp), &graph_def)
    } else {
        Default::default()
    };

    let mut graph_def = graph_def;
    crate::api::graph_exec_helpers::apply_hyperparameter_bindings(&mut graph_def, &hp_values);

    let task_id = start_graph_task(
        Arc::clone(&state),
        broadcast_tx,
        graph_def,
        graph_name,
        graph_id,
        file_path,
        is_workflow_set,
        request_client_ip(req),
    );

    res.render(Json(serde_json::json!({"task_id": task_id})));
}

fn run_graph_blocking(
    definition: zihuan_node::graph_io::NodeGraphDefinition,
    stop_flag: Arc<AtomicBool>,
    task_id: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut graph = zihuan_node::registry::build_node_graph_from_definition(&definition)
        .map_err(|e| format!("Build graph failed: {e}"))?;
    graph.set_execution_task_id(Some(task_id.clone()));
    // Link the external stop flag to the graph's internal stop flag
    let graph_flag = graph.get_stop_flag();
    let flag_clone = Arc::clone(&stop_flag);
    std::thread::spawn(move || {
        loop {
            if flag_clone.load(std::sync::atomic::Ordering::Relaxed) {
                graph_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
    crate::log_forwarder::scope_task(task_id, || {
        graph.execute().map_err(|e| format!("Execution failed: {e}").into())
    })
}

// ─── Stop task ────────────────────────────────────────────────────────────────

#[handler]
pub async fn stop_task(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let task_id = req.param::<String>("task_id").unwrap_or_default();
    let ok = state.tasks.lock().unwrap().stop_task(&task_id);
    if !ok {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "Task not found"})));
        return;
    }
    res.render(Json(serde_json::json!({"ok": true})));
}

#[handler]
pub async fn rerun_task(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let broadcast_tx = depot.obtain::<WsBroadcast>().unwrap().clone();
    let task_id = req.param::<String>("task_id").unwrap_or_default();

    let body = req.parse_json::<RerunTaskRequest>().await.ok();
    let fallback_ip = request_client_ip(req);

    let (file_path, user_ip) = {
        let tasks = state.tasks.lock().unwrap();
        let Some(task) = tasks.get(&task_id) else {
            res.status_code(StatusCode::NOT_FOUND);
            res.render(Json(serde_json::json!({"error": "Task not found"})));
            return;
        };
        let Some(file_path) = task.file_path.clone() else {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": "This task cannot be rerun because it has no saved file path"})));
            return;
        };
        (file_path, body.and_then(|v| v.user_ip).or_else(|| fallback_ip.clone()))
    };

    let loaded = match zihuan_node::load_graph_definition_from_json_with_migration(&file_path) {
        Ok(loaded) => loaded,
        Err(e) => {
            res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
            res.render(Json(serde_json::json!({"error": format!("Failed to reload graph: {e}")})));
            return;
        }
    };

    let mut graph = loaded.graph;
    zihuan_node::ensure_positions(&mut graph);
    let session_id = format!("rerun-{}", Uuid::new_v4());
    let task_id = start_graph_task(
        Arc::clone(&state),
        broadcast_tx,
        graph,
        graph_display_name(Some(&file_path)),
        session_id,
        Some(file_path.clone()),
        is_workflow_set_path(Some(&file_path)),
        user_ip,
    );

    res.render(Json(serde_json::json!({"task_id": task_id})));
}

#[handler]
pub async fn get_task_logs(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let task_id = req.param::<String>("task_id").unwrap_or_default();

    let tasks = state.tasks.lock().unwrap();
    if tasks.get(&task_id).is_none() {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "Task not found"})));
        return;
    }

    match tasks.read_task_logs(&task_id) {
        Ok(entries) => res.render(Json(serde_json::json!({"entries": entries}))),
        Err(err) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({"error": err.to_string()})));
        }
    }
}

#[handler]
pub async fn clear_non_running_tasks(_req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let cleared = state.tasks.lock().unwrap().clear_non_running();
    res.render(Json(serde_json::json!({"ok": true, "cleared": cleared})));
}

// ─── List tasks ───────────────────────────────────────────────────────────────

#[handler]
pub async fn list_tasks(_req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let tasks = state.tasks.lock().unwrap();
    let entries = tasks.list();
    res.render(Json(entries));
}

fn start_graph_task(
    state: Arc<AppState>,
    broadcast_tx: WsBroadcast,
    graph_def: zihuan_node::graph_io::NodeGraphDefinition,
    graph_name: String,
    graph_session_id: String,
    file_path: Option<String>,
    is_workflow_set: bool,
    user_ip: Option<String>,
) -> String {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let task_id = state.tasks.lock().unwrap().add_task(
        graph_name.clone(),
        graph_session_id.clone(),
        file_path,
        is_workflow_set,
        user_ip,
        Arc::clone(&stop_flag),
    );

    let _ = broadcast_tx.send(ServerMessage::TaskStarted {
        task_id: task_id.clone(),
        graph_name,
        graph_session_id,
    });

    let state_clone = Arc::clone(&state);
    let broadcast_tx_clone = broadcast_tx.clone();
    let task_id_clone = task_id.clone();
    let stop_flag_check = Arc::clone(&stop_flag);
    tokio::spawn(async move {
        let task_id_for_exec = task_id_clone.clone();
        let result = tokio::task::spawn_blocking(move || {
            run_graph_blocking(graph_def, stop_flag, task_id_for_exec)
        })
        .await;

        let (status, success, error_message) = match result {
            Ok(Ok(())) if stop_flag_check.load(Ordering::Relaxed) => {
                info!("Graph execution stopped by user");
                (TaskStatus::Stopped, false, None)
            }
            Ok(Ok(())) => {
                info!("Graph execution completed successfully");
                (TaskStatus::Success, true, None)
            }
            Ok(Err(e)) if stop_flag_check.load(Ordering::Relaxed) => {
                let msg = e.to_string();
                info!("Graph execution stopped with interruption: {}", msg);
                (TaskStatus::Stopped, false, None)
            }
            Ok(Err(e)) => {
                let msg = e.to_string();
                error!("Graph execution error: {}", msg);
                (TaskStatus::Failed, false, Some(msg))
            }
            Err(e) if stop_flag_check.load(Ordering::Relaxed) => {
                info!("Graph execution join stopped: {}", e);
                (TaskStatus::Stopped, false, None)
            }
            Err(e) => {
                let msg = e.to_string();
                error!("Graph execution panicked: {}", msg);
                (TaskStatus::Failed, false, Some(msg))
            }
        };

        state_clone
            .tasks
            .lock()
            .unwrap()
            .finish_task(&task_id_clone, status.clone(), error_message.clone());

        match status {
            TaskStatus::Stopped => {
                match broadcast_tx_clone.send(ServerMessage::TaskStopped {
                    task_id: task_id_clone,
                }) {
                    Ok(n) => info!("Broadcast TaskStopped to {} receivers", n),
                    Err(e) => error!("Failed to broadcast TaskStopped: {}", e),
                }
            }
            _ => {
                match broadcast_tx_clone.send(ServerMessage::TaskFinished {
                    task_id: task_id_clone,
                    success,
                    error: error_message,
                }) {
                    Ok(n) => info!("Broadcast TaskFinished(success={}) to {} receivers", success, n),
                    Err(e) => error!("Failed to broadcast TaskFinished: {}", e),
                }
            }
        }
    });

    task_id
}

fn graph_display_name(file_path: Option<&str>) -> String {
    file_path
        .and_then(|path| std::path::Path::new(path).file_stem())
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Untitled".to_string())
}

fn is_workflow_set_path(file_path: Option<&str>) -> bool {
    file_path
        .map(|path| path.replace('\\', "/").starts_with("workflow_set/"))
        .unwrap_or(false)
}

fn request_client_ip(req: &Request) -> Option<String> {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| Some(req.remote_addr().to_string()))
}
