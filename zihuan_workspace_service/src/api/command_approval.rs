use salvo::prelude::*;
use serde::Deserialize;
use serde_json::json;

use crate::tools::workspace_tools::{
    approve_command, pending_command_approval, reject_command, revoke_session_command_approval,
    session_command_approvals,
};

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
    let accepted = match body.decision.as_str() {
        "once" => approve_command(&session_id, &body.command, false),
        "session" => approve_command(&session_id, &body.command, true),
        "reject" => reject_command(&session_id, &body.command),
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(json!({ "error": "decision must be once, session, or reject" })));
            return;
        }
    };
    if !accepted {
        res.status_code(StatusCode::CONFLICT);
        res.render(Json(json!({ "error": "no matching pending command approval" })));
        return;
    }
    res.render(Json(json!({ "ok": true })));
}

#[handler]
pub async fn get_pending_command_approval(req: &mut Request, res: &mut Response) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    if session_id.trim().is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(json!({ "error": "session_id must not be empty" })));
        return;
    }
    res.render(Json(json!({ "pending": pending_command_approval(&session_id) })));
}

#[handler]
pub async fn get_session_command_approvals(req: &mut Request, res: &mut Response) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    res.render(Json(json!({ "commands": session_command_approvals(&session_id) })));
}

#[handler]
pub async fn revoke_session_command(req: &mut Request, res: &mut Response) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    let family = req.param::<String>("family").unwrap_or_default();
    if revoke_session_command_approval(&session_id, &family) {
        res.render(Json(json!({ "ok": true })));
    } else {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(json!({ "error": "command approval not found" })));
    }
}
