use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use uuid::Uuid;

use log::info;
use storage_handler::{ConnectionConfig, ConnectionKind};
use ims_bot_adapter::parse_ims_bot_adapter_connection;
use crate::system_config;

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
            let bot = parse_ims_bot_adapter_connection(bot)
                .map_err(|err| err.to_string())?;
            if bot.bot_server_url.trim().is_empty() {
                return Err("ims_bot_adapter.bot_server_url must not be empty".to_string());
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
        name: body.name,
        enabled: body.enabled,
        kind: body.kind,
        updated_at: now_rfc3339(),
    };
    connections.push(connection.clone());

    match system_config::save_connections(connections) {
        Ok(()) => {
            info!("[connections] created connection '{}' (id={})", connection.name, connection.id);
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
    connection.enabled = body.enabled;
    connection.kind = body.kind;
    connection.updated_at = now_rfc3339();
    let response = connection.clone();

    match system_config::save_connections(connections) {
        Ok(()) => {
            info!("[connections] updated connection '{}' (id={})", response.name, response.id);
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
            info!("[connections] deleted connection (id={})", id);
            res.render(Json(ok_response()));
        }
        Err(err) => render_internal_error(res, err),
    }
}
