use chrono::Local;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{error, info};
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use uuid::Uuid;

use super::state::AppState;
use super::state::{TaskLogEntry, TaskStatus};
use super::ws::{ServerMessage, WsBroadcast};

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
    definition: zihuan_graph_engine::graph_io::NodeGraphDefinition,
    runtime_inline_values: Vec<crate::api::graph_exec_helpers::RuntimeInlineValue>,
    stop_flag: Arc<AtomicBool>,
    task_id: String,
    broadcast_tx: WsBroadcast,
    graph_session_id: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let preview_node_ids: HashSet<String> = definition
        .nodes
        .iter()
        .filter(|n| n.node_type == "qq_message_preview")
        .map(|n| n.id.clone())
        .collect();

    let mut graph = zihuan_graph_engine::registry::build_node_graph_from_definition(&definition)
        .map_err(|e| format!("Build graph failed: {e}"))?;
    crate::api::graph_exec_helpers::inject_runtime_inline_values(
        &mut graph,
        &runtime_inline_values,
    );
    graph.set_execution_task_id(Some(task_id.clone()));

    if !preview_node_ids.is_empty() {
        let tx = broadcast_tx.clone();
        let task = task_id.clone();
        let session = graph_session_id.clone();
        let ids = Arc::new(preview_node_ids);
        graph.set_execution_callback(move |node_id, inputs, _outputs| {
            if !ids.contains(node_id) {
                return;
            }
            let Some(value) = inputs.get("messages") else {
                return;
            };
            let Ok(json) = serde_json::to_value(value) else {
                return;
            };
            let _ = tx.send(ServerMessage::NodePreviewQQMessages {
                task_id: task.clone(),
                graph_session_id: session.clone(),
                node_id: node_id.to_string(),
                messages: json,
            });
        });
    }

    // Link the external stop flag to the graph's internal stop flag
    let graph_flag = graph.get_stop_flag();
    let flag_clone = Arc::clone(&stop_flag);
    std::thread::spawn(move || loop {
        if flag_clone.load(std::sync::atomic::Ordering::Relaxed) {
            graph_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    });
    crate::log_forwarder::scope_task(task_id, || {
        graph
            .execute()
            .map_err(|e| format!("Execution failed: {e}").into())
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
        (
            file_path,
            body.and_then(|v| v.user_ip).or_else(|| fallback_ip.clone()),
        )
    };

    let loaded =
        match zihuan_graph_engine::load_graph_definition_from_json_with_migration(&file_path) {
            Ok(loaded) => loaded,
            Err(e) => {
                res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
                res.render(Json(
                    serde_json::json!({"error": format!("Failed to reload graph: {e}")}),
                ));
                return;
            }
        };

    let mut graph = loaded.graph;
    zihuan_graph_engine::ensure_positions(&mut graph);
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

    // Optional query params: date (YYYY-MM-DD prefix), limit (usize), offset (usize).
    let filter_date: Option<String> = req.query("date");
    let limit: Option<usize> = req.query::<usize>("limit");
    let offset: usize = req.query::<usize>("offset").unwrap_or(0);

    let tasks = state.tasks.lock().unwrap();
    if tasks.get(&task_id).is_none() {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(serde_json::json!({"error": "Task not found"})));
        return;
    }

    match tasks.read_task_logs(&task_id) {
        Ok(all_entries) => {
            let filtered: Vec<_> = all_entries
                .into_iter()
                .filter(|e| {
                    if let Some(date) = &filter_date {
                        e.timestamp.starts_with(date.as_str())
                    } else {
                        true
                    }
                })
                .collect();
            let total = filtered.len();
            let page: Vec<_> = filtered
                .into_iter()
                .skip(offset)
                .take(limit.unwrap_or(usize::MAX))
                .collect();
            res.render(Json(serde_json::json!({
                "entries": page,
                "total": total,
                "offset": offset,
                "limit": limit,
            })));
        }
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
    graph_def: zihuan_graph_engine::graph_io::NodeGraphDefinition,
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
        file_path.clone(),
        is_workflow_set,
        user_ip,
        Arc::clone(&stop_flag),
    );

    let _ = broadcast_tx.send(ServerMessage::TaskStarted {
        task_id: task_id.clone(),
        graph_name,
        graph_session_id: graph_session_id.clone(),
    });

    let state_clone = Arc::clone(&state);
    let broadcast_tx_clone = broadcast_tx.clone();
    let task_id_clone = task_id.clone();
    let stop_flag_check = Arc::clone(&stop_flag);
    tokio::spawn(async move {
        let prepared = match crate::api::graph_exec_helpers::prepare_execution_context(
            graph_def,
            file_path.as_deref().map(std::path::Path::new),
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(err) => {
                let detailed = format!("Failed to prepare graph execution: {err}");
                let summary = summarize_graph_error(&detailed);
                append_task_error_detail(&state_clone, &task_id_clone, &detailed);
                state_clone.tasks.lock().unwrap().finish_task(
                    &task_id_clone,
                    TaskStatus::Failed,
                    Some(summary.clone()),
                );
                let _ = broadcast_tx_clone.send(ServerMessage::TaskFinished {
                    task_id: task_id_clone,
                    success: false,
                    error: Some(summary),
                });
                return;
            }
        };

        let runtime_inline_values = prepared.runtime_inline_values;
        let prepared_graph_def = prepared.definition;
        let background_tasks = prepared.background_tasks;
        let task_id_for_exec = task_id_clone.clone();
        let broadcast_tx_for_exec = broadcast_tx.clone();
        let session_for_exec = graph_session_id;
        let result = tokio::task::spawn_blocking(move || {
            run_graph_blocking(
                prepared_graph_def,
                runtime_inline_values,
                stop_flag,
                task_id_for_exec,
                broadcast_tx_for_exec,
                session_for_exec,
            )
        })
        .await;

        for task in background_tasks {
            task.abort();
        }

        let (status, success, error_message, detailed_error_message) = match result {
            Ok(Ok(())) if stop_flag_check.load(Ordering::Relaxed) => {
                info!("Graph execution stopped by user");
                (TaskStatus::Stopped, false, None, None)
            }
            Ok(Ok(())) => {
                info!("Graph execution completed successfully");
                (TaskStatus::Success, true, None, None)
            }
            Ok(Err(e)) if stop_flag_check.load(Ordering::Relaxed) => {
                let msg = e.to_string();
                info!("Graph execution stopped with interruption: {}", msg);
                (TaskStatus::Stopped, false, None, None)
            }
            Ok(Err(e)) => {
                let detailed = e.to_string();
                let summary = summarize_graph_error(&detailed);
                let stack_preview = summarize_backtrace_preview(&detailed, 6);
                if stack_preview.is_empty() {
                    error!("Graph execution error: {}", summary);
                } else {
                    error!("Graph execution error: {}\n{}", summary, stack_preview);
                }
                (TaskStatus::Failed, false, Some(summary), Some(detailed))
            }
            Err(e) if stop_flag_check.load(Ordering::Relaxed) => {
                info!("Graph execution join stopped: {}", e);
                (TaskStatus::Stopped, false, None, None)
            }
            Err(e) => {
                let detailed = format!("Graph execution panicked: {e}");
                let summary = summarize_graph_error(&detailed);
                let stack_preview = summarize_backtrace_preview(&detailed, 6);
                if stack_preview.is_empty() {
                    error!("Graph execution panicked: {}", summary);
                } else {
                    error!("Graph execution panicked: {}\n{}", summary, stack_preview);
                }
                (TaskStatus::Failed, false, Some(summary), Some(detailed))
            }
        };

        if let Some(detailed) = detailed_error_message.as_ref() {
            append_task_error_detail(&state_clone, &task_id_clone, detailed);
        }

        state_clone.tasks.lock().unwrap().finish_task(
            &task_id_clone,
            status.clone(),
            error_message.clone(),
        );

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
                    Ok(n) => info!(
                        "Broadcast TaskFinished(success={}) to {} receivers",
                        success, n
                    ),
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

fn summarize_graph_error(message: &str) -> String {
    message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "[DEBUG_BACKTRACE]")
        .unwrap_or("Graph execution failed")
        .to_string()
}

fn summarize_backtrace_preview(message: &str, max_lines: usize) -> String {
    let Some((_, backtrace)) = message.split_once("[DEBUG_BACKTRACE]") else {
        return String::new();
    };

    backtrace
        .lines()
        .map(str::trim_end)
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && (trimmed.contains(".rs:")
                    || trimmed.starts_with("at ")
                    || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()))
        })
        .take(max_lines)
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn append_task_error_detail(state: &AppState, task_id: &str, detailed: &str) {
    let _ = state.tasks.lock().unwrap().append_task_log(
        task_id,
        &TaskLogEntry {
            timestamp: Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string(),
            level: "ERROR_DETAIL".to_string(),
            message: detailed.to_string(),
        },
    );
}
