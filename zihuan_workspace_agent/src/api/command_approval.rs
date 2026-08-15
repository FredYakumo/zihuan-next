use salvo::prelude::*;
use serde::Deserialize;
use serde_json::json;

use crate::tools::workspace_tools::{approve_command, reject_command};

#[derive(Deserialize)]
struct CommandApprovalRequest {
    command: String,
    decision: String,
}

#[handler]
pub async fn approve_command_execution(req: &mut Request, res: &mut Response) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    let body: CommandApprovalRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(json!({ "error": format!("invalid command approval: {error}") })));
            return;
        }
    };
    if session_id.trim().is_empty() || body.command.trim().is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(json!({ "error": "session_id and command must not be empty" })));
        return;
    }
    match body.decision.as_str() {
        "once" => approve_command(&session_id, &body.command, false),
        "session" => approve_command(&session_id, &body.command, true),
        "reject" => reject_command(&session_id, &body.command),
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(json!({ "error": "decision must be once, session, or reject" })));
            return;
        }
    }
    res.render(Json(json!({ "ok": true })));
}
