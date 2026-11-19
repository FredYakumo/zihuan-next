use super::{render_bad_request, render_internal_error, render_not_found};
use salvo::prelude::*;
use salvo::writing::Json;
use zihuan_core::config::planers::{self, PlanerConfig};

#[handler]
pub async fn list(res: &mut Response) {
    match planers::load_planers() {
        Ok(items) => {
            let runtime = crate::api::planer_runtime::shared().ok();
            let values: Vec<_> = items
                .into_iter()
                .map(|p| {
                    let running = runtime.as_ref().is_some_and(|r| r.is_running(&p.config_id));
                    let mut value = serde_json::to_value(p).unwrap();
                    value["runtime"] =
                        serde_json::json!({"status":if running {"running"} else {"stopped"}});
                    value
                })
                .collect();
            res.render(Json(values));
        }
        Err(e) => render_internal_error(res, e),
    }
}
#[handler]
pub async fn get(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    match planers::load_planers() {
        Ok(items) => match items.into_iter().find(|b| b.config_id == id) {
            Some(item) => res.render(Json(item)),
            None => render_not_found(res, "Planer not found"),
        },
        Err(e) => render_internal_error(res, e),
    }
}
async fn save(req: &mut Request, res: &mut Response, creating: bool) {
    let mut item = match req.parse_json::<PlanerConfig>().await {
        Ok(item) => item,
        Err(e) => return render_bad_request(res, e.to_string()),
    };
    if creating {
        item.config_id = uuid::Uuid::new_v4().to_string();
    } else {
        item.config_id = req.param::<String>("id").unwrap_or_default();
        match planers::load_planers() {
            Ok(items) if !items.iter().any(|b| b.config_id == item.config_id) => {
                return render_not_found(res, "Planer not found")
            }
            Err(e) => return render_internal_error(res, e),
            _ => {}
        }
    }
    item.updated_at = chrono::Utc::now().to_rfc3339();
    match planers::save_planer(item.clone()) {
        Ok(()) => res.render(Json(item)),
        Err(e) => render_bad_request(res, e.to_string()),
    }
}
#[handler]
pub async fn create(req: &mut Request, res: &mut Response) {
    save(req, res, true).await;
}
#[handler]
pub async fn update(req: &mut Request, res: &mut Response) {
    save(req, res, false).await;
}
#[handler]
pub async fn delete(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    if crate::api::planer_runtime::shared().is_ok_and(|r| r.is_running(&id)) {
        return render_bad_request(res, "请先停止 Planer".into());
    }
    match planers::delete_planer(&id) {
        Ok(true) => res.render(Json(serde_json::json!({"ok":true}))),
        Ok(false) => render_not_found(res, "Planer not found"),
        Err(e) => render_internal_error(res, e),
    }
}

#[handler]
pub async fn start(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    match crate::api::planer_runtime::shared().and_then(|r| r.start(&id)) {
        Ok(()) => res.render(Json(serde_json::json!({"ok":true}))),
        Err(e) => render_bad_request(res, e.to_string()),
    }
}
#[handler]
pub async fn stop(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    match crate::api::planer_runtime::shared().and_then(|r| r.stop(&id)) {
        Ok(()) => res.render(Json(serde_json::json!({"ok":true}))),
        Err(e) => render_bad_request(res, e.to_string()),
    }
}
