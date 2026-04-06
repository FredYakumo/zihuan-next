pub mod execution;
pub mod file_io;
pub mod graph;
pub mod graph_exec_helpers;
pub mod hyperparams;
pub mod registry;
pub mod state;
pub mod ws;

use std::sync::Arc;

use rust_embed::RustEmbed;
use salvo::prelude::*;
use salvo::serve_static::static_embed;
use salvo::cors::Cors;

use state::AppState;
use ws::{WsBroadcast, ws_handler};

/// Embedded frontend assets (populated after `cd webui && pnpm run build`)
#[derive(RustEmbed)]
#[folder = "webui/dist/"]
struct WebAssets;

/// Build the Salvo router with all API endpoints and static file serving.
pub fn build_router(state: Arc<AppState>, broadcast: WsBroadcast) -> Router {
    // API routes
    let api = Router::new()
        // Registry
        .push(
            Router::with_path("registry")
                .push(Router::with_path("types").get(registry::get_registry))
                .push(Router::with_path("categories").get(registry::get_categories)),
        )
        // Graph management
        .push(
            Router::with_path("graphs")
                .get(graph::list_graphs)
                .post(graph::create_graph)
                .push(
                    Router::with_path("<id>")
                        .get(graph::get_graph)
                        .put(graph::put_graph)
                        .delete(graph::delete_graph)
                        .push(Router::with_path("nodes").post(graph::add_node))
                        .push(
                            Router::with_path("nodes/<node_id>")
                                .put(graph::update_node)
                                .delete(graph::delete_node),
                        )
                        .push(
                            Router::with_path("edges")
                                .post(graph::add_edge)
                                .delete(graph::delete_edge),
                        )
                        .push(Router::with_path("validate").post(graph::validate_graph))
                        .push(Router::with_path("execute").post(execution::execute_graph))
                        .push(Router::with_path("file/save").post(file_io::save_file))
                        .push(Router::with_path("file/download").get(file_io::download_graph))
                        .push(Router::with_path("hyperparameters")
                            .get(hyperparams::get_hyperparameters)
                            .put(hyperparams::update_hyperparameter_values))
                        .push(Router::with_path("variables")
                            .get(hyperparams::get_variables)
                            .put(hyperparams::update_variables)),
                ),
        )
        // Tasks
        .push(
            Router::with_path("tasks")
                .get(execution::list_tasks)
                .push(Router::with_path("<task_id>/stop").post(execution::stop_task)),
        )
        // File I/O (not graph-scoped)
        .push(Router::with_path("file/open").post(file_io::open_file))
        .push(Router::with_path("file/upload").post(file_io::upload_graph))
        // Workflows directory
        .push(Router::with_path("workflow_set").get(file_io::list_workflows))
        .push(Router::with_path("workflow_set/save").post(file_io::save_to_workflows))
        .push(Router::with_path("workflow_set/detailed").get(file_io::list_workflows_detailed))
        .push(Router::with_path("workflow_set/cover/<filename>").get(file_io::serve_workflow_cover));

    // Inject state into depot for all API handlers (REST + WebSocket)
    Router::new()
        .push(
            Router::with_path("api")
                .hoop(
                    salvo::affix_state::inject(Arc::clone(&state))
                        .inject(broadcast),
                )
                .push(Router::with_path("ws").goal(ws_handler))
                .push(api),
        )
        // Serve frontend static files (fallback SPA routing handled by static_embed)
        .push(Router::with_path("<**rest>").get(static_embed::<WebAssets>().fallback("index.html")))
}
