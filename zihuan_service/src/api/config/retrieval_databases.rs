use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

use crate::api::config::render_bad_request;
use crate::api::setup_wizard::ExecuteSetupResponse;
use crate::api::state::AppState;
use crate::setup_orchestrator::{
    build_search_only_config, generate_retrieval_install_command, validate_detailed_config,
    DetailedInstallMethod, DetailedSearchSetupConfig, SetupOrchestrator,
};

#[derive(Deserialize)]
pub struct RetrievalDatabaseInstallRequest {
    pub install_method: DetailedInstallMethod,
    pub search: DetailedSearchSetupConfig,
}

/// Returns the response for an invalid install request, or `Ok(())` when the
/// request describes a single retrieval database that can be installed.
fn validate_install_request(
    request: &RetrievalDatabaseInstallRequest,
    res: &mut Response,
) -> Result<(), ()> {
    let config = build_search_only_config(request.search.clone(), request.install_method.clone());
    if let Err(error) = validate_detailed_config(&config) {
        render_bad_request(res, error);
        return Err(());
    }
    Ok(())
}

#[handler]
pub async fn post_retrieval_install_command(req: &mut Request, res: &mut Response) {
    let body: RetrievalDatabaseInstallRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };
    if validate_install_request(&body, res).is_err() {
        return;
    }

    match generate_retrieval_install_command(&body.search, &body.install_method) {
        Ok(command) => res.render(Json(command)),
        Err(error) => render_bad_request(res, error),
    }
}

/// Starts a background install task for a single retrieval database. Progress
/// is streamed over the shared task channel via `GET /api/setup/progress`.
#[handler]
pub async fn post_retrieval_install(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let body: RetrievalDatabaseInstallRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };
    if validate_install_request(&body, res).is_err() {
        return;
    }

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
    state.setup_tasks.lock().unwrap().insert(task_id.clone(), progress_tx);

    let task_id_for_spawn = task_id.clone();
    tokio::spawn(async move {
        let result = orchestrator
            .run_retrieval_database_install(body.search, body.install_method)
            .await;
        match result {
            Ok(connection) => {
                log::info!(
                    "[setup_orchestrator] retrieval database task {} finished",
                    task_id_for_spawn
                );
                orchestrator.emit_finished_with_connection(
                    "Retrieval database setup complete!",
                    connection,
                );
            }
            Err(err) => {
                log::error!(
                    "[setup_orchestrator] retrieval database task {} failed: {}",
                    task_id_for_spawn,
                    err
                );
                orchestrator.emit("failed", "error", &err, None);
            }
        }
        // Keep the broadcast channel alive for 60s so late SSE clients can
        // still connect, mirroring the setup wizard task lifecycle.
        tokio::time::sleep(Duration::from_secs(60)).await;
        let _ = state.setup_tasks.lock().unwrap().remove(&task_id_for_spawn);
    });

    res.render(Json(ExecuteSetupResponse { accepted: true, task_id }));
}
