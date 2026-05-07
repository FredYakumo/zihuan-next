use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::system_config;
use ims_bot_adapter::{
    close_runtime_bot_adapter_instance, list_active_bot_adapter_connection_ids,
    list_runtime_bot_adapter_instances, parse_ims_bot_adapter_connection,
    sync_enabled_bot_adapters,
};
use log::info;
use storage_handler::{
    close_runtime_storage_instance, list_runtime_storage_instances, ConnectionConfig,
    ConnectionKind,
};

use super::{
    now_rfc3339, ok_response, render_bad_request, render_internal_error, render_not_found,
};

#[derive(Deserialize)]
pub struct CreateConnectionRequest {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub kind: ConnectionKind,
}

#[derive(Deserialize)]
pub struct UpdateConnectionRequest {
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub kind: ConnectionKind,
}

fn validate_connection(kind: &ConnectionKind) -> Result<(), String> {
    match kind {
        ConnectionKind::BotAdapter(bot) => {
            let bot = parse_ims_bot_adapter_connection(bot).map_err(|err| err.to_string())?;
            if bot.bot_server_url.trim().is_empty() {
                return Err("ims_bot_adapter.bot_server_url must not be empty".to_string());
            }
            if matches!(bot.adapter_server_url.as_deref().map(str::trim), Some("")) {
                return Err("ims_bot_adapter.adapter_server_url must not be empty".to_string());
            }
            Ok(())
        }
        ConnectionKind::Tavily(tavily) => {
            if tavily.api_token.trim().is_empty() {
                return Err("tavily.api_token must not be empty".to_string());
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[handler]
pub async fn list_connections(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    match system_config::load_connections() {
        Ok(connections) => res.render(Json(connections)),
        Err(err) => render_internal_error(res, err),
    }
}

#[derive(serde::Serialize)]
pub struct ActiveBotAdapterInfo {
    pub connection_id: String,
    pub config_id: String,
    pub name: String,
    pub ws_url: String,
}

#[handler]
pub async fn list_active_bot_adapters(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    match system_config::load_connections() {
        Ok(connections) => {
            let active_ids = list_active_bot_adapter_connection_ids();
            let items: Vec<ActiveBotAdapterInfo> = active_ids
                .into_iter()
                .filter_map(|connection_id| {
                    let connection = connections.iter().find(|item| item.id == connection_id)?;
                    let ConnectionKind::BotAdapter(raw) = &connection.kind else {
                        return None;
                    };
                    let parsed = parse_ims_bot_adapter_connection(raw).ok()?;
                    Some(ActiveBotAdapterInfo {
                        connection_id: connection.id.clone(),
                        config_id: connection.canonical_config_id().to_string(),
                        name: connection.name.clone(),
                        ws_url: parsed.bot_server_url,
                    })
                })
                .collect();
            res.render(Json(items));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn create_connection(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let body: CreateConnectionRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    if let Err(err) = validate_connection(&body.kind) {
        return render_bad_request(res, err);
    }

    let mut connections = match system_config::load_connections() {
        Ok(connections) => connections,
        Err(err) => return render_internal_error(res, err),
    };

    let connection = ConnectionConfig {
        id: Uuid::new_v4().to_string(),
        config_id: String::new(),
        name: body.name,
        enabled: body.enabled,
        kind: body.kind,
        updated_at: now_rfc3339(),
    };
    let mut connection = connection;
    connection.config_id = connection.id.clone();
    connections.push(connection.clone());

    match system_config::save_connections(connections) {
        Ok(()) => {
            let refreshed = system_config::load_connections().unwrap_or_default();
            sync_enabled_bot_adapters(&refreshed).await;
            info!(
                "[connections] created connection '{}' (id={})",
                connection.name, connection.id
            );
            res.render(Json(connection));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn update_connection(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let id = req.param::<String>("id").unwrap_or_default();
    let body: UpdateConnectionRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    if let Err(err) = validate_connection(&body.kind) {
        return render_bad_request(res, err);
    }

    let mut connections = match system_config::load_connections() {
        Ok(connections) => connections,
        Err(err) => return render_internal_error(res, err),
    };

    let Some(connection) = connections.iter_mut().find(|item| item.id == id) else {
        return render_not_found(res, "Connection not found");
    };

    connection.name = body.name;
    connection.config_id = connection.id.clone();
    connection.enabled = body.enabled;
    connection.kind = body.kind;
    connection.updated_at = now_rfc3339();
    let response = connection.clone();

    match system_config::save_connections(connections) {
        Ok(()) => {
            let refreshed = system_config::load_connections().unwrap_or_default();
            sync_enabled_bot_adapters(&refreshed).await;
            info!(
                "[connections] updated connection '{}' (id={})",
                response.name, response.id
            );
            res.render(Json(response));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn delete_connection(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let id = req.param::<String>("id").unwrap_or_default();
    let mut connections = match system_config::load_connections() {
        Ok(connections) => connections,
        Err(err) => return render_internal_error(res, err),
    };
    let before = connections.len();
    connections.retain(|item| item.id != id);

    if before == connections.len() {
        return render_not_found(res, "Connection not found");
    }

    match system_config::save_connections(connections) {
        Ok(()) => {
            let refreshed = system_config::load_connections().unwrap_or_default();
            sync_enabled_bot_adapters(&refreshed).await;
            info!("[connections] deleted connection (id={})", id);
            res.render(Json(ok_response()));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[derive(serde::Serialize)]
pub struct RuntimeInstancesResponse {
    pub items: Vec<zihuan_core::connection_manager::RuntimeConnectionInstanceSummary>,
    pub total: usize,
    pub page: usize,
    pub page_size: usize,
}

#[handler]
pub async fn list_runtime_instances(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let page = req.query::<usize>("page").unwrap_or(1).max(1);
    let page_size = req.query::<usize>("page_size").unwrap_or(20).clamp(1, 200);

    match (
        list_runtime_storage_instances(),
        list_runtime_bot_adapter_instances().await,
    ) {
        (Ok(mut storage), Ok(mut bot)) => {
            storage.append(&mut bot);
            storage.sort_by(|a, b| b.started_at.cmp(&a.started_at));
            let total = storage.len();
            let start = (page - 1) * page_size;
            let items = storage
                .into_iter()
                .skip(start)
                .take(page_size)
                .collect::<Vec<_>>();
            res.render(Json(RuntimeInstancesResponse {
                items,
                total,
                page,
                page_size,
            }));
        }
        (Err(err), _) | (_, Err(err)) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn close_runtime_instance(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let instance_id = req.param::<String>("instance_id").unwrap_or_default();
    if instance_id.trim().is_empty() {
        return render_bad_request(res, "instance_id is required".to_string());
    }

    match close_runtime_storage_instance(&instance_id) {
        Ok(true) => {
            res.render(Json(ok_response()));
            return;
        }
        Ok(false) => {}
        Err(err) => {
            render_internal_error(res, err);
            return;
        }
    }

    match close_runtime_bot_adapter_instance(&instance_id).await {
        Ok(true) => res.render(Json(ok_response())),
        Ok(false) => render_not_found(res, "Runtime connection instance not found"),
        Err(err) => render_internal_error(res, err),
    }
}
