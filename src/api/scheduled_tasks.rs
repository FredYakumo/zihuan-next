use std::sync::Arc;

use salvo::prelude::*;
use salvo::writing::Json;

use crate::api::config::{render_bad_request, render_internal_error};
use crate::api::state::AppState;
use crate::system_config;

async fn connection_for_service(service_id: &str) -> Result<zihuan_core::data_refs::RelationalDbConnection, String> {
    let agents = system_config::load_agents().map_err(|err| err.to_string())?;
    let agent = agents.into_iter().find(|agent| agent.id == service_id).ok_or_else(|| "Service not found".to_string())?;
    let model_inference::system_config::AgentType::QqChat(config) = agent.agent_type else {
        return Err("计划任务目前仅支持 QQ Chat Service".to_string());
    };
    let rdb_id = config.resolved_rdb_id().ok_or_else(|| "该 Service 未配置关系数据库".to_string())?;
    let connections = system_config::load_connections().map_err(|err| err.to_string())?;
    storage_handler::build_relational_db_connection_for_connection(rdb_id, &connections).await.map_err(|err| err.to_string())
}

#[handler]
pub async fn list_scheduled_tasks(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let service_id = req.query::<String>("service_id");
    let Some(service_id) = service_id else { return render_bad_request(res, "service_id is required".to_string()); };
    let status = req.query::<String>("status");
    let result = async {
        let connection = connection_for_service(&service_id).await?;
        zihuan_service::scheduled_task::list_tasks(&connection, Some(&service_id), status.as_deref())
            .await
            .map_err(|err| err.to_string())
    }
    .await;
    match result {
        Ok(tasks) => res.render(Json(tasks)),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn cancel_scheduled_task(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let service_id = req.query::<String>("service_id");
    let Some(service_id) = service_id else { return render_bad_request(res, "service_id is required".to_string()); };
    let task_id = req.param::<String>("task_id").unwrap_or_default();
    let result = async {
        let connection = connection_for_service(&service_id).await?;
        let tasks = zihuan_service::scheduled_task::list_tasks(&connection, Some(&service_id), None).await.map_err(|err| err.to_string())?;
        let Some(task) = tasks.into_iter().find(|task| task.id == task_id) else { return Err("计划任务不存在".to_string()); };
        if task.status != zihuan_service::scheduled_task::ScheduledTaskStatus::Pending { return Err("只有等待中的计划任务可以取消".to_string()); }
        zihuan_service::scheduled_task::finish_task(&connection, &task_id, zihuan_service::scheduled_task::ScheduledTaskStatus::Cancelled, Some("已由管理员取消")).await.map_err(|err| err.to_string())
    }.await;
    match result { Ok(()) => res.render(Json(serde_json::json!({"ok": true}))), Err(err) => render_internal_error(res, err) }
}
