use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zihuan_core::command::{CommandPermission, PermissionRule};
use zihuan_core::config::{ConfigKind, ConfigRepository, FsConfigRepository, StoredConfigRecord};

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

/// Handlers — Salvo endpoint handlers for command-permission CRUD.
///
/// ## Purpose
///
/// Exposes REST endpoints that allow the admin UI to list, create, update, and
/// delete command-permission records. These operate on the file-system config
/// repository (`FsConfigRepository`).
///
/// ## Design
///
/// - **List** (`list_command_permissions`) reads `ConfigRoot.command_permissions`
///   and projects each record to `CommandPermissionDto`.
/// - **Create** (`create_command_permission`) parses the JSON body, wraps it in a
///   `StoredConfigRecord` with a generated UUID, appends to the root, and persists.
/// - Each handler follows the same pattern: load root from `FsConfigRepository`,
///   mutate the in-memory copy, save back. Conflicts between concurrent writes are
///   not handled (acceptable for low-frequency admin use).
/// - Error responses use the shared `render_*` helpers from the parent module for
///   consistent error formatting.

#[handler]
pub async fn list_command_permissions(_req: &mut Request, res: &mut Response) {
    let repo = FsConfigRepository::default();
    let root = match repo.load_root() {
        Ok(root) => root,
        Err(err) => return render_internal_error(res, err),
    };

    let permissions: Vec<CommandPermissionDto> = root
        .configs
        .command_permissions
        .iter()
        .map(CommandPermissionDto::from)
        .collect();

    res.render(Json(permissions));
}

#[handler]
pub async fn create_command_permission(req: &mut Request, res: &mut Response) {
    let body: CreateCommandPermissionRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let repo = FsConfigRepository::default();
    let mut root = match repo.load_root() {
        Ok(root) => root,
        Err(err) => return render_internal_error(res, err),
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

    root.configs.command_permissions.push(record.clone());

    match repo.save_root(&root) {
        Ok(()) => {
            let dto = CommandPermissionDto::from(&record);
            res.render(Json(dto));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn update_command_permission(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    let body: UpdateCommandPermissionRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };

    let repo = FsConfigRepository::default();
    let mut root = match repo.load_root() {
        Ok(root) => root,
        Err(err) => return render_internal_error(res, err),
    };

    let idx = root
        .configs
        .command_permissions
        .iter()
        .position(|r| r.config_id == id);

    let Some(idx) = idx else {
        return render_not_found(res, "command permission not found");
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

    {
        let record = &mut root.configs.command_permissions[idx];
        record.name = permission.command_name.clone();
        record.enabled = body.enabled;
        record.updated_at = now_rfc3339();
        record.spec = serde_json::to_value(&permission).unwrap_or_default();
    }

    match repo.save_root(&root) {
        Ok(()) => {
            let dto = CommandPermissionDto::from(&root.configs.command_permissions[idx]);
            // Also sync to the in-memory global registry
            if let Some(registry) = zihuan_service::command::global_command_registry() {
                registry.set_permissions(&permission.command_name, permission.rules);
            }
            res.render(Json(dto));
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn delete_command_permission(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();

    let repo = FsConfigRepository::default();
    let mut root = match repo.load_root() {
        Ok(root) => root,
        Err(err) => return render_internal_error(res, err),
    };

    let idx = root
        .configs
        .command_permissions
        .iter()
        .position(|r| r.config_id == id);

    match idx {
        Some(pos) => {
            root.configs.command_permissions.remove(pos);
            match repo.save_root(&root) {
                Ok(()) => res.render(Json(ok_response())),
                Err(err) => render_internal_error(res, err),
            }
        }
        None => render_not_found(res, "command permission not found"),
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
