pub mod chat;
pub mod config;
pub mod execution;
pub mod explorer;
pub mod file_io;
pub mod graph;
pub mod graph_exec_helpers;
pub mod hyperparams;
pub mod log;
pub mod registry;
pub mod settings;
pub mod state;
pub mod themes;
pub mod ws;

use std::sync::Arc;

use rust_embed::RustEmbed;
use salvo::http::{HeaderMap, Method};
use salvo::prelude::*;
use salvo::serve_static::static_embed;

use state::AppState;
use ws::{ws_handler, WsBroadcast};

/// Embedded frontend assets (populated after `cd webui && pnpm run build`)
#[derive(RustEmbed)]
#[folder = "webui/dist/"]
struct WebAssets;

/// Build the Salvo router with all API endpoints and static file serving.
pub fn build_router(
    state: Arc<AppState>,
    broadcast: WsBroadcast,
    canonical_local_origin: Option<String>,
) -> Router {
    // API routes
    let api = Router::new()
        // Registry
        .push(
            Router::with_path("registry")
                .push(Router::with_path("types").get(registry::get_registry))
                .push(Router::with_path("categories").get(registry::get_categories)),
        )
        .push(
            Router::with_path("system")
                .push(
                    Router::with_path("connections")
                        .get(config::connections::list_connections)
                        .push(
                            Router::with_path("active-bot-adapters")
                                .get(config::connections::list_active_bot_adapters),
                        )
                        .push(
                            Router::with_path("runtime-instances")
                                .get(config::connections::list_runtime_instances)
                                .push(
                                    Router::with_path("<instance_id>/close")
                                        .post(config::connections::close_runtime_instance),
                                ),
                        )
                        .post(config::connections::create_connection)
                        .push(
                            Router::with_path("<id>")
                                .put(config::connections::update_connection)
                                .delete(config::connections::delete_connection),
                        ),
                )
                .push(
                    Router::with_path("llm-refs")
                        .get(config::llm_refs::list_llm_refs)
                        .post(config::llm_refs::create_llm_ref)
                        .push(
                            Router::with_path("<id>")
                                .put(config::llm_refs::update_llm_ref)
                                .delete(config::llm_refs::delete_llm_ref),
                        ),
                )
                .push(
                    Router::with_path("agents")
                        .get(config::agents::list_agents)
                        .post(config::agents::create_agent)
                        .push(
                            Router::with_path("<id>")
                                .put(config::agents::update_agent)
                                .delete(config::agents::delete_agent)
                                .push(Router::with_path("start").post(config::agents::start_agent))
                                .push(Router::with_path("stop").post(config::agents::stop_agent)),
                        ),
                ),
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
                        .push(
                            Router::with_path("hyperparameters")
                                .get(hyperparams::get_hyperparameters)
                                .put(hyperparams::update_hyperparameter_values),
                        )
                        .push(
                            Router::with_path("variables")
                                .get(hyperparams::get_variables)
                                .put(hyperparams::update_variables),
                        )
                        .push(
                            Router::with_path("metadata")
                                .get(graph::get_metadata)
                                .put(graph::update_metadata),
                        ),
                ),
        )
        // Tasks
        .push(
            Router::with_path("tasks")
                .get(execution::list_tasks)
                .delete(execution::clear_non_running_tasks)
                .push(Router::with_path("delete-batch").post(execution::delete_tasks))
                .push(Router::with_path("<task_id>").delete(execution::delete_task))
                .push(Router::with_path("<task_id>/stop").post(execution::stop_task))
                .push(Router::with_path("<task_id>/rerun").post(execution::rerun_task))
                .push(Router::with_path("<task_id>/logs").get(execution::get_task_logs)),
        )
        // File I/O (not graph-scoped)
        .push(Router::with_path("file/open").post(file_io::open_file))
        .push(Router::with_path("file/upload").post(file_io::upload_graph))
        .push(Router::with_path("file/upload-image").post(file_io::upload_image))
        .push(Router::with_path("models/text-embedding").get(file_io::list_text_embedding_models))
        .push(Router::with_path("uploaded-images/<**rest>").get(file_io::serve_uploaded_image))
        // Frontend log forwarding
        .push(Router::with_path("log").post(log::frontend_log))
        // Chat
        .push(
            Router::with_path("chat")
                .push(Router::with_path("stream").post(chat::stream_chat))
                .push(Router::with_path("sessions").get(chat::list_chat_sessions))
                .push(Router::with_path("sessions/<session_id>").delete(chat::delete_chat_session))
                .push(
                    Router::with_path("sessions/<session_id>/messages")
                        .get(chat::get_chat_session_messages),
                ),
        )
        // Workflows directory
        .push(Router::with_path("workflow_set").get(file_io::list_workflows))
        .push(Router::with_path("workflow_set/save").post(file_io::save_to_workflows))
        .push(Router::with_path("workflow_set/detailed").get(file_io::list_workflows_detailed))
        .push(Router::with_path("workflow_set/cover/<filename>").get(file_io::serve_workflow_cover))
        // Themes
        .push(
            Router::with_path("themes")
                .get(themes::list_themes)
                .push(Router::with_path("<name>").get(themes::get_theme)),
        )
        // Settings
        .push(Router::with_path("settings/storage-info").get(settings::get_storage_info))
        // Data Explorer
        .push(
            Router::with_path("explorer")
                .push(Router::with_path("mysql").get(explorer::query_mysql))
                .push(Router::with_path("redis").get(explorer::query_redis))
                .push(Router::with_path("weaviate").get(explorer::query_weaviate))
                .push(Router::with_path("rustfs").get(explorer::query_rustfs)),
        );

    // Inject state into depot for all API handlers (REST + WebSocket)
    let mut router = Router::new();
    if let Some(origin) = canonical_local_origin {
        router = router.hoop(CanonicalLocalRedirect::new(origin));
    }

    router
        .push(
            Router::with_path("api")
                .hoop(salvo::affix_state::inject(Arc::clone(&state)).inject(broadcast))
                .push(Router::with_path("ws").goal(ws_handler))
                .push(api),
        )
        // Serve frontend static files (fallback SPA routing handled by static_embed)
        .push(Router::with_path("<**rest>").get(static_embed::<WebAssets>().fallback("index.html")))
}

struct CanonicalLocalRedirect {
    canonical_origin: String,
}

impl CanonicalLocalRedirect {
    fn new(canonical_origin: String) -> Self {
        Self { canonical_origin }
    }
}

#[async_trait]
impl Handler for CanonicalLocalRedirect {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        if matches!(*req.method(), Method::GET | Method::HEAD)
            && should_redirect_local_host(req.headers(), &self.canonical_origin)
        {
            let path_and_query = req
                .uri()
                .path_and_query()
                .map(|value| value.as_str())
                .unwrap_or("/");
            res.render(Redirect::temporary(format!(
                "{}{}",
                self.canonical_origin, path_and_query
            )));
            ctrl.skip_rest();
            return;
        }

        ctrl.call_next(req, depot, res).await;
    }
}

fn should_redirect_local_host(headers: &HeaderMap, canonical_local_origin: &str) -> bool {
    let Some(host_header) = headers.get("host").and_then(|value| value.to_str().ok()) else {
        return false;
    };

    let Some(request_host) = normalize_host(host_header) else {
        return false;
    };
    let Some(canonical_host) = canonical_host(canonical_local_origin) else {
        return false;
    };

    !is_trusted_loopback_host(&request_host) && request_host != canonical_host
}

fn canonical_host(origin: &str) -> Option<String> {
    origin
        .split_once("://")
        .map(|(_, remainder)| remainder)
        .and_then(normalize_host)
}

fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim();
    if host.is_empty() {
        return None;
    }

    if host.starts_with('[') {
        return host
            .split_once(']')
            .map(|(addr, _)| format!("{addr}]").to_ascii_lowercase());
    }

    match host.split_once(':') {
        Some((name, _)) => Some(name.to_ascii_lowercase()),
        None => Some(host.to_ascii_lowercase()),
    }
}

fn is_trusted_loopback_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]")
}
