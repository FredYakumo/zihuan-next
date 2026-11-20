use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use zihuan_core::error::Error;
use zihuan_core::permission::{CreateEntryResult, CreateGroupResult, EntityType, PermissionEntry};

use super::{
    ok_response, render_bad_request, render_internal_error, render_not_found,
    render_unprocessable_entity,
};

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    name: String,
}

#[derive(Deserialize)]
pub struct EntryRequest {
    entity_id: String,
    entity_type: String,
    category: String,
    value: i32,
}

#[derive(Deserialize)]
pub struct DeleteEntryQuery {
    entity_id: String,
    entity_type: String,
    category: String,
}

fn render_error(res: &mut Response, error: Error) {
    match error {
        Error::ValidationError(message) => render_unprocessable_entity(res, message),
        other => render_internal_error(res, other),
    }
}

fn parse_entity_type(value: &str, res: &mut Response) -> Option<EntityType> {
    match EntityType::parse(value) {
        Ok(entity_type) => Some(entity_type),
        Err(error) => {
            render_error(res, error);
            None
        }
    }
}

fn finish_entry(res: &mut Response, result: Result<PermissionEntry, Error>, status: StatusCode) {
    match result {
        Ok(entry) => {
            res.status_code(status);
            res.render(Json(entry));
        }
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub async fn list_permission_groups(req: &mut Request, res: &mut Response) {
    let search = req.query::<String>("search");
    match zihuan_core::permission::list_groups(search.as_deref()) {
        Ok(groups) => res.render(Json(groups)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub async fn create_permission_group(req: &mut Request, res: &mut Response) {
    let body: CreateGroupRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_bad_request(res, error.to_string()),
    };
    match zihuan_core::permission::create_group(&body.name) {
        Ok(CreateGroupResult::Created(group)) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(group));
        }
        Ok(CreateGroupResult::Existing(group)) => {
            res.status_code(StatusCode::CONFLICT);
            res.render(Json(serde_json::json!({
                "error": "权限组已存在",
                "group": group,
            })));
        }
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub async fn list_permission_entries(req: &mut Request, res: &mut Response) {
    let group_name = req.param::<String>("name").unwrap_or_default();
    match zihuan_core::permission::group_exists(&group_name) {
        Ok(true) => {}
        Ok(false) => return render_not_found(res, "权限组不存在"),
        Err(error) => return render_error(res, error),
    }
    match zihuan_core::permission::list_entries(&group_name) {
        Ok(entries) => res.render(Json(entries)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub async fn create_permission_entry(req: &mut Request, res: &mut Response) {
    let group_name = req.param::<String>("name").unwrap_or_default();
    match zihuan_core::permission::group_exists(&group_name) {
        Ok(true) => {}
        Ok(false) => return render_not_found(res, "权限组不存在"),
        Err(error) => return render_error(res, error),
    }
    let body: EntryRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_bad_request(res, error.to_string()),
    };
    let Some(entity_type) = parse_entity_type(&body.entity_type, res) else {
        return;
    };
    match zihuan_core::permission::create_entry(
        &group_name,
        &body.entity_id,
        entity_type,
        &body.category,
        body.value,
    ) {
        Ok(CreateEntryResult::Created(entry)) => {
            res.status_code(StatusCode::CREATED);
            res.render(Json(entry));
        }
        Ok(CreateEntryResult::Conflict(entry)) => {
            res.status_code(StatusCode::CONFLICT);
            res.render(Json(serde_json::json!({
                "error": "条目已存在",
                "entry": entry,
            })));
        }
        Err(error) => render_error(res, error),
    }
}

#[handler]
pub async fn update_permission_entry(req: &mut Request, res: &mut Response) {
    let group_name = req.param::<String>("name").unwrap_or_default();
    let body: EntryRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_bad_request(res, error.to_string()),
    };
    let Some(entity_type) = parse_entity_type(&body.entity_type, res) else {
        return;
    };
    let result = zihuan_core::permission::update_entry(
        &group_name,
        &body.entity_id,
        entity_type,
        &body.category,
        body.value,
    )
    .and_then(|entry| entry.ok_or(Error::NotFound));
    match result {
        Err(Error::NotFound) => render_not_found(res, "条目不存在"),
        result => finish_entry(res, result, StatusCode::OK),
    }
}

#[handler]
pub async fn delete_permission_entry(req: &mut Request, res: &mut Response) {
    let group_name = req.param::<String>("name").unwrap_or_default();
    let query: DeleteEntryQuery = match req.parse_queries() {
        Ok(query) => query,
        Err(error) => return render_bad_request(res, error.to_string()),
    };
    let Some(entity_type) = parse_entity_type(&query.entity_type, res) else {
        return;
    };
    match zihuan_core::permission::delete_entry(
        &group_name,
        &query.entity_id,
        entity_type,
        &query.category,
    ) {
        Ok(true) => res.render(Json(ok_response())),
        Ok(false) => render_not_found(res, "条目不存在"),
        Err(error) => render_error(res, error),
    }
}
