use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;

use super::{ok_response, render_bad_request, render_internal_error};

/// Returns the list of registered commands from the in-memory CommandRegistry.
#[handler]
pub async fn get_registered_commands(_req: &mut Request, res: &mut Response) {
    let Some(registry) = zihuan_service::command::global_command_registry() else {
        return render_internal_error(res, "command registry not initialized");
    };

    let commands: Vec<serde_json::Value> = registry
        .list_commands()
        .iter()
        .map(|def| {
            serde_json::json!({
                "name": def.name,
                "aliases": def.aliases,
                "description": def.description,
                "scope": def.scope,
                "accepted_arg_count": def.accepted_arg_count,
            })
        })
        .collect();

    res.render(Json(commands));
}

#[derive(Deserialize)]
pub struct UserPermissionRequest {
    connection_id: String,
    agent_id: String,
    user_id: String,
    access_level: u32,
}

#[handler]
pub async fn get_user_permission(req: &mut Request, res: &mut Response) {
    let connection_id = req.query::<String>("connection_id").unwrap_or_default();
    let agent_id = req.query::<String>("agent_id").unwrap_or_default();
    let user_id = req.query::<String>("user_id").unwrap_or_default();
    if agent_id.is_empty() || user_id.is_empty() || connection_id.is_empty() {
        return render_bad_request(
            res,
            "connection_id, agent_id and user_id are required".to_string(),
        );
    }
    let result = async {
        let connections = zihuan_core::storage::load_connections()?;
        let connection = zihuan_core::storage::build_relational_db_connection_for_connection(
            &connection_id,
            &connections,
        )
        .await?;
        zihuan_core::storage::user_permission::access_level(&connection, &agent_id, &user_id).await
    }
    .await;
    match result {
        Ok(level) => res.render(Json(
            serde_json::json!({"agent_id":agent_id,"user_id":user_id,"access_level":level}),
        )),
        Err(error) => render_internal_error(res, error),
    }
}

#[handler]
pub async fn set_user_permission(req: &mut Request, res: &mut Response) {
    let body: UserPermissionRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_bad_request(res, error.to_string()),
    };
    if body.agent_id.is_empty() || body.user_id.is_empty() || body.connection_id.is_empty() {
        return render_bad_request(
            res,
            "connection_id, agent_id and user_id are required".to_string(),
        );
    }
    let result = async {
        let connections = zihuan_core::storage::load_connections()?;
        let connection = zihuan_core::storage::build_relational_db_connection_for_connection(
            &body.connection_id,
            &connections,
        )
        .await?;
        zihuan_core::storage::user_permission::set_access_level(
            &connection,
            &body.agent_id,
            &body.user_id,
            body.access_level,
        )
        .await
    }
    .await;
    match result {
        Ok(()) => res.render(Json(ok_response())),
        Err(error) => render_internal_error(res, error),
    }
}
