//! Task-manager integration for resumable data migrations (schema readiness stays synchronous).
use crate::task_context::{AgentTaskRequest, AgentTaskResult, AgentTaskRuntime, AgentTaskStatus};
use std::sync::Arc;

pub fn progress(message: impl Into<String>) {
    let message = message.into();
    log::info!("[data_migration] {message}");
    crate::task_context::append_current_task_progress(message);
}

fn report(
    runtime: Option<&Arc<dyn AgentTaskRuntime>>,
    handle: Option<&Arc<crate::task_context::AgentTaskHandle>>,
    message: String,
) {
    if let (Some(runtime), Some(handle)) = (runtime, handle) {
        crate::task_context::scope_task_id(handle.task_id.clone(), || {
            log::info!("[data_migration] {message}")
        });
        runtime.append_task_progress(&handle.task_id, message);
    } else {
        log::info!("[data_migration] {message}");
    }
}

/// Schema-coupled transfers must finish before the connection is used, but are still visible
/// as tasks. Server startup opens role connections in its background startup worker.
pub async fn tracked<T>(
    name: &str,
    operation: impl std::future::Future<Output = crate::error::Result<T>>,
) -> crate::error::Result<T> {
    let runtime = crate::command::global_task_runtime();
    let handle = runtime.as_ref().map(|runtime| {
        runtime.start_task(AgentTaskRequest {
            task_name: name.to_string(),
            agent_id: "database-migration".into(),
            agent_name: "数据库迁移".into(),
            user_ip: None,
            owner_id: None,
            task_db_connection_id: None,
        })
    });
    report(runtime.as_ref(), handle.as_ref(), format!("{name}：开始"));
    tokio::pin!(operation);
    let started = std::time::Instant::now();
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(5));
    heartbeat.tick().await;
    let result = loop {
        tokio::select! {
            result = &mut operation => break result,
            _ = heartbeat.tick() => {
                let message = format!("{name}：正在执行数据库操作，已耗时 {:.1}s", started.elapsed().as_secs_f64());
                report(runtime.as_ref(), handle.as_ref(), message);
            }
        }
    };
    let error = result.as_ref().err().map(ToString::to_string);
    let summary = format!("{name}：{}", if error.is_some() { "失败" } else { "完成" });
    report(runtime.as_ref(), handle.as_ref(), summary.clone());
    if let Some(handle) = handle {
        handle.finish(AgentTaskResult {
            status: Some(if error.is_some() {
                AgentTaskStatus::Failed
            } else {
                AgentTaskStatus::Success
            }),
            result_summary: Some(summary),
            error_message: error,
        });
    }
    result
}

/// The blocking worker owns thread-local task context across async database operations.
pub fn spawn(
    runtime: Option<Arc<dyn AgentTaskRuntime>>,
    agent_id: String,
    agent_name: String,
    name: String,
    run: impl FnOnce() -> crate::error::Result<()> + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let handle = runtime.as_ref().map(|runtime| {
            runtime.start_task(AgentTaskRequest {
                task_name: name.clone(),
                agent_id,
                agent_name,
                user_ip: None,
                owner_id: None,
                task_db_connection_id: None,
            })
        });
        let task = handle.clone();
        let label = name.clone();
        let worker_runtime = runtime.clone();
        let mut worker = tokio::task::spawn_blocking(move || {
            let execute = || {
                progress(format!("{label}：开始"));
                run()
            };
            match (worker_runtime, task) {
                (Some(runtime), Some(task)) => {
                    crate::task_context::scope_task_runtime(runtime, || {
                        crate::task_context::scope_task_id(task.task_id.clone(), execute)
                    })
                }
                _ => execute(),
            }
        });
        let started = std::time::Instant::now();
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(5));
        heartbeat.tick().await;
        let result = loop {
            tokio::select! {
                result = &mut worker => break result,
                _ = heartbeat.tick() => report(runtime.as_ref(), handle.as_ref(), format!("{name}：后台迁移仍在运行，已耗时 {:.1}s", started.elapsed().as_secs_f64())),
            }
        };
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error.to_string()),
            Err(error) => Some(error.to_string()),
        };
        let summary = match &error {
            Some(error) => format!("{name}：失败，剩余数据保留供重试：{error}"),
            None => format!("{name}：完成"),
        };
        report(runtime.as_ref(), handle.as_ref(), summary.clone());
        if let Some(handle) = handle {
            handle.finish(AgentTaskResult {
                status: Some(if error.is_some() {
                    AgentTaskStatus::Failed
                } else {
                    AgentTaskStatus::Success
                }),
                result_summary: Some(summary),
                error_message: error,
            });
        }
    })
}
