use salvo::prelude::*;
use salvo::writing::Json;

/// Read-only diagnostics: the scheduler's registered jobs and the services whose job
/// resources are currently online (`GET /api/scheduler/jobs`).
#[handler]
pub async fn list_scheduler_jobs(_req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    res.render(Json(zihuan_core::scheduler::status()));
}
