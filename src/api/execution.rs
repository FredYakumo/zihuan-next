use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use log::{error, info};
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;

use super::state::AppState;
use crate::util::hyperparam_store;

// ─── Execute graph ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ExecuteRequest {
    /// Optional hyperparameter overrides (name → JSON value)
    pub hyperparameter_overrides: Option<serde_json::Value>,
}

#[handler]
pub async fn execute_graph(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let graph_id = req.param::<String>("id").unwrap_or_default();

    let (graph_def, graph_name, file_path) = {
        let sessions = state.sessions.read().unwrap();
        let session = match sessions.get(&graph_id) {
            Some(s) => s,
            None => {
                res.status_code(StatusCode::NOT_FOUND);
                res.render(Json(serde_json::json!({"error": "Graph not found"})));
                return;
            }
        };
        (
            session.graph.clone(),
            session
                .file_path
                .as_deref()
                .map(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Untitled".to_string())
                })
                .unwrap_or_else(|| "Untitled".to_string()),
            session.file_path.clone(),
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

    let stop_flag = Arc::new(AtomicBool::new(false));
    let task_id = state.tasks.lock().unwrap().add_task(
        graph_name,
        graph_id.clone(),
        Arc::clone(&stop_flag),
    );

    // Spawn background execution
    let state_clone = Arc::clone(&state);
    let task_id_clone = task_id.clone();
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            run_graph_blocking(graph_def, stop_flag)
        })
        .await;

        let success = match result {
            Ok(Ok(())) => {
                info!("Graph execution completed successfully");
                true
            }
            Ok(Err(e)) => {
                error!("Graph execution error: {}", e);
                false
            }
            Err(e) => {
                error!("Graph execution panicked: {}", e);
                false
            }
        };

        state_clone
            .tasks
            .lock()
            .unwrap()
            .finish_task(&task_id_clone, success);
    });

    res.render(Json(serde_json::json!({"task_id": task_id})));
}

fn run_graph_blocking(
    definition: zihuan_node::graph_io::NodeGraphDefinition,
    stop_flag: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut graph = zihuan_node::registry::build_node_graph_from_definition(&definition)
        .map_err(|e| format!("Build graph failed: {e}"))?;
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
    graph.execute().map_err(|e| format!("Execution failed: {e}").into())
}

// ─── Stop task ────────────────────────────────────────────────────────────────

#[handler]
pub async fn stop_task(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let task_id = req.param::<String>("task_id").unwrap_or_default();
    state.tasks.lock().unwrap().stop_task(&task_id);
    res.render(Json(serde_json::json!({"ok": true})));
}

// ─── List tasks ───────────────────────────────────────────────────────────────

#[handler]
pub async fn list_tasks(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let tasks = state.tasks.lock().unwrap();
    let entries: Vec<&super::state::TaskEntry> = tasks.list();
    res.render(Json(entries));
}
