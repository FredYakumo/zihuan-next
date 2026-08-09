use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct LogRequest {
    pub level: String,
    pub message: String,
}

/// Accept a log message from the frontend and emit it via the Rust `log` crate
/// so it flows through LogUtil (console + file output).
#[handler]
pub async fn frontend_log(req: &mut Request, res: &mut Response) {
    let body: LogRequest = match req.parse_json().await {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": e.to_string()})));
            return;
        }
    };

    match body.level.to_lowercase().as_str() {
        "error" => log::error!("[UI] {}", body.message),
        "warn" => log::warn!("[UI] {}", body.message),
        "debug" => log::debug!("[UI] {}", body.message),
        "trace" => log::trace!("[UI] {}", body.message),
        _ => log::info!("[UI] {}", body.message),
    }

    res.render(Json(serde_json::json!({"ok": true})));
}
