use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zihuan_core::command::{CommandPermission, PermissionRule};
use zihuan_core::config::{ConfigKind, StoredConfigRecord};

use super::{
    now_rfc3339, ok_response, render_bad_request, render_internal_error, render_not_found,
};

// DTOs — Data Transfer Objects for the command-permission REST API.
//
// ## Purpose
//
// Defines the request/response shapes for CRUD operations on per-command
// permission rules. These types are the API contract between the frontend
// admin panel and the backend config storage.
//
// ## Design
//
// - `CommandPermissionDto` is the read model, projected from `StoredConfigRecord`
//   via `From`. It flattens the stored JSON spec into a flat struct for JSON
//   serialisation.
// - `CreateCommandPermissionRequest` / `UpdateCommandPermissionRequest` share the
//   same shape (intentionally duplicated to allow future divergence). Both default
//   `rules` to an empty vec and `enabled` to `false`.
// - Field names in the request DTOs match the JSON keys expected by the admin UI;
//   deserialisation uses `#[serde(default)]` for safe partial updates.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPermissionDto {
    pub config_id: String,
    pub command_name: String,
    pub rules: Vec<PermissionRule>,
    pub enabled: bool,
    pub updated_at: String,
}

impl From<&StoredConfigRecord> for CommandPermissionDto {
    fn from(record: &StoredConfigRecord) -> Self {
        let cmd: CommandPermission =
            serde_json::from_value(record.spec.clone()).unwrap_or(CommandPermission {
                command_name: String::new(),
                rules: vec![PermissionRule::Everyone],
                enabled: true,
            });
        Self {
            config_id: record.config_id.clone(),
            command_name: cmd.command_name,
            rules: cmd.rules,
            enabled: cmd.enabled,
            updated_at: record.updated_at.clone(),
        }
    }
}

#[derive(Deserialize)]
pub struct CreateCommandPermissionRequest {
    pub command_name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<PermissionRule>,
}

#[derive(Deserialize)]
pub struct UpdateCommandPermissionRequest {
    pub command_name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub rules: Vec<PermissionRule>,
}

/// Command policies are persisted in UserPermission. Mutations also update the
/// running registry, so changes apply immediately without a service restart.
#[handler]
pub async fn list_command_permissions(_req: &mut Request, res: &mut Response) {
    let records = match zihuan_core::storage::user_permission::command_records() {
        Ok(records) => records,
        Err(error) => return render_internal_error(res, error),
    };
    let permissions: Vec<CommandPermissionDto> =
        records.iter().map(CommandPermissionDto::from).collect();

    res.render(Json(permissions));
}

#[handler]
pub async fn create_command_permission(req: &mut Request, res: &mut Response) {
    let body: CreateCommandPermissionRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let permission = CommandPermission {
        command_name: body.command_name,
        rules: if body.rules.is_empty() {
            vec![PermissionRule::Everyone]
        } else {
            body.rules
        },
        enabled: body.enabled,
    };

    let record = StoredConfigRecord {
        config_id: Uuid::new_v4().to_string(),
        kind: ConfigKind::CommandPermission,
        name: permission.command_name.clone(),
        enabled: body.enabled,
        updated_at: now_rfc3339(),
        spec: serde_json::to_value(&permission).unwrap_or_default(),
    };

    let saved = record.clone();
    if let Err(error) = zihuan_core::storage::user_permission::save_command_record(&record) {
        return render_internal_error(res, error);
    }
    if let Some(registry) = zihuan_service::command::global_command_registry() {
        registry.set_permissions(
            &permission.command_name,
            if permission.enabled {
                permission.rules
            } else {
                vec![PermissionRule::Everyone]
            },
        );
    }

    let dto = CommandPermissionDto::from(&saved);
    res.render(Json(dto));
}

#[handler]
pub async fn update_command_permission(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    let body: UpdateCommandPermissionRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let permission = CommandPermission {
        command_name: body.command_name.clone(),
        rules: if body.rules.is_empty() {
            vec![PermissionRule::Everyone]
        } else {
            body.rules.clone()
        },
        enabled: body.enabled,
    };

    let mut records = match zihuan_core::storage::user_permission::command_records() {
        Ok(records) => records,
        Err(error) => return render_internal_error(res, error),
    };
    let Some(record) = records.iter_mut().find(|record| record.config_id == id) else {
        return render_not_found(res, "command permission not found");
    };
    let old_name = record.name.clone();
    record.name = permission.command_name.clone();
    record.enabled = body.enabled;
    record.updated_at = now_rfc3339();
    record.spec = serde_json::to_value(&permission).unwrap_or_default();
    if let Err(error) = zihuan_core::storage::user_permission::save_command_record(record) {
        return render_internal_error(res, error);
    }
    if let Some(registry) = zihuan_service::command::global_command_registry() {
        registry.set_permissions(&old_name, vec![PermissionRule::Everyone]);
        registry.set_permissions(
            &permission.command_name,
            if permission.enabled {
                permission.rules
            } else {
                vec![PermissionRule::Everyone]
            },
        );
    }
    res.render(Json(CommandPermissionDto::from(&*record)));
}

#[handler]
pub async fn delete_command_permission(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();

    let records = match zihuan_core::storage::user_permission::command_records() {
        Ok(records) => records,
        Err(error) => return render_internal_error(res, error),
    };
    let name = records
        .iter()
        .find(|record| record.config_id == id)
        .map(|record| record.name.clone());
    match zihuan_core::storage::user_permission::delete_command_record(&id) {
        Ok(true) => {
            if let (Some(registry), Some(name)) =
                (zihuan_service::command::global_command_registry(), name)
            {
                registry.set_permissions(&name, vec![PermissionRule::Everyone]);
            }
            res.render(Json(ok_response()));
        }
        Ok(false) => render_not_found(res, "command permission not found"),
        Err(error) => render_internal_error(res, error),
    }
}

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
