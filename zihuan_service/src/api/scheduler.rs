//! Scheduler diagnostics and job-script management.
//!
//! Job scripts live in `scheduled_jobs/` and declare their own manifest. The read/write
//! handlers here are what let the admin page edit them; every write re-registers the jobs on
//! success, so an edit takes effect without restarting the service.

use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::config::{render_bad_request, render_internal_error};

/// Read-only diagnostics: the scheduler's registered jobs and the services whose job
/// resources are currently online (`GET /api/scheduler/jobs`).
#[handler]
pub async fn list_scheduler_jobs(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    res.render(Json(zihuan_core::scheduler::status()));
}

#[derive(Debug, Deserialize)]
pub struct SaveJobScriptRequest {
    script: String,
    content: String,
}

/// Returns one job script's source for editing (`GET /api/scheduler/jobs/script?script=`).
#[handler]
pub async fn get_scheduler_job_script(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let Some(script) = req.query::<String>("script") else {
        return render_bad_request(res, "script is required".to_string());
    };
    match zihuan_core::scheduler::read_job_script(&script) {
        Ok(content) => res.render(Json(json!({ "script": script, "content": content }))),
        Err(error) => render_bad_request(res, error.to_string()),
    }
}

/// Writes a job script and re-registers the jobs (`POST /api/scheduler/jobs/script`).
///
/// Content that yields no usable manifest is rejected and the previous file restored, so a
/// failed save can never drop a job out of the catalog.
#[handler]
pub async fn save_scheduler_job_script(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let body: SaveJobScriptRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => {
            return render_bad_request(res, format!("请求体解析失败: {error}"));
        }
    };
    match zihuan_core::scheduler::save_job_script(&body.script, &body.content) {
        Ok(saved) => res.render(Json(json!({
            "ok": true,
            "script": saved.script,
            "task_name": saved.task_name,
        }))),
        Err(error) => render_bad_request(res, error.to_string()),
    }
}

/// Deletes an operator-provided job script (`DELETE /api/scheduler/jobs/script?script=`).
#[handler]
pub async fn delete_scheduler_job_script(
    req: &mut Request,
    res: &mut Response,
    _depot: &mut Depot,
) {
    let Some(script) = req.query::<String>("script") else {
        return render_bad_request(res, "script is required".to_string());
    };
    match zihuan_core::scheduler::delete_job_script(&script) {
        Ok(()) => res.render(Json(json!({ "ok": true }))),
        Err(error) => render_bad_request(res, error.to_string()),
    }
}

/// Re-reads `scheduled_jobs/` and rebuilds the job registry (`POST /api/scheduler/jobs/reload`),
/// for changes made outside the editor.
#[handler]
pub async fn reload_scheduler_jobs(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    match zihuan_core::scheduler::reload_script_jobs() {
        Ok(count) => res.render(Json(json!({ "ok": true, "count": count }))),
        Err(error) => render_internal_error(res, error),
    }
}
