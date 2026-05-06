pub mod agents;
pub mod connections;
pub mod llm_refs;

use chrono::Utc;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct OkResponse {
    ok: bool,
}

pub fn ok_response() -> OkResponse {
    OkResponse { ok: true }
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

pub fn render_bad_request(res: &mut Response, message: String) {
    res.status_code(StatusCode::BAD_REQUEST);
    res.render(Json(serde_json::json!({ "error": message })));
}

pub fn render_not_found(res: &mut Response, message: &str) {
    res.status_code(StatusCode::NOT_FOUND);
    res.render(Json(serde_json::json!({ "error": message })));
}

pub fn render_unprocessable_entity(res: &mut Response, message: String) {
    res.status_code(StatusCode::UNPROCESSABLE_ENTITY);
    res.render(Json(serde_json::json!({ "error": message })));
}

pub fn render_internal_error(res: &mut Response, err: impl ToString) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(Json(serde_json::json!({ "error": err.to_string() })));
}
