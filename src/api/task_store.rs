use std::sync::Arc;

use chrono::{DateTime, Local};
use sqlx::AnyPool;
use sqlx::Row;

use crate::api::state::{TaskEntry, TaskLogEntry, TaskStatus, TaskType};
use zihuan_core::error::{Error, Result};

pub async fn insert_task_entry(pool: &AnyPool, entry: &TaskEntry) -> Result<()> {
    let task_type = task_type_str(&entry.task_type);
    let start_time = entry.start_time.naive_utc().to_string();
    let status = task_status_str(&entry.status);
    let is_running = entry.is_running as i64;
    let is_workflow_set = entry.is_workflow_set as i64;
    let can_rerun = entry.can_rerun as i64;

    sqlx::query(
        "INSERT INTO task_entry \
         (id, task_type, graph_name, graph_session_id, file_path, is_workflow_set, \
          start_time, is_running, end_time, duration_ms, user_ip, owner_id, status, \
          error_message, result_summary, can_rerun) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry.id)
    .bind(task_type)
    .bind(&entry.graph_name)
    .bind(&entry.graph_session_id)
    .bind(&entry.file_path)
    .bind(is_workflow_set)
    .bind(&start_time)
    .bind(is_running)
    .bind::<Option<String>>(None)
    .bind::<Option<i64>>(None)
    .bind(&entry.user_ip)
    .bind(&entry.owner_id)
    .bind(status)
    .bind::<Option<String>>(None)
    .bind::<Option<String>>(None)
    .bind(can_rerun)
    .execute(pool)
    .await
    .map_err(Error::Database)?;

    Ok(())
}

pub async fn update_task_entry_finished(
    pool: &AnyPool,
    task_id: &str,
    status: &TaskStatus,
    error_message: Option<&str>,
    result_summary: Option<&str>,
    end_time: DateTime<Local>,
    duration_ms: i64,
) -> Result<()> {
    let status_str = task_status_str(status);
    let end_time_str = end_time.naive_utc().to_string();

    sqlx::query(
        "UPDATE task_entry SET is_running = 0, status = ?, error_message = ?, \
         result_summary = ?, end_time = ?, duration_ms = ? WHERE id = ?",
    )
    .bind(status_str)
    .bind(error_message)
    .bind(result_summary)
    .bind(&end_time_str)
    .bind(duration_ms)
    .bind(task_id)
    .execute(pool)
    .await
    .map_err(Error::Database)?;

    Ok(())
}

pub async fn update_task_entry_stopped(
    pool: &AnyPool,
    task_id: &str,
    end_time: DateTime<Local>,
    duration_ms: i64,
) -> Result<()> {
    let end_time_str = end_time.naive_utc().to_string();

    sqlx::query(
        "UPDATE task_entry SET is_running = 0, status = 'stopped', \
         end_time = ?, duration_ms = ? WHERE id = ?",
    )
    .bind(&end_time_str)
    .bind(duration_ms)
    .bind(task_id)
    .execute(pool)
    .await
    .map_err(Error::Database)?;

    Ok(())
}

pub async fn delete_task_entry(pool: &AnyPool, task_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM task_entry WHERE id = ?")
        .bind(task_id)
        .execute(pool)
        .await
        .map_err(Error::Database)?;

    Ok(())
}

pub async fn mark_orphan_running_stopped(pool: &AnyPool) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE task_entry SET is_running = 0, status = 'stopped' WHERE is_running = 1",
    )
    .execute(pool)
    .await
    .map_err(Error::Database)?;

    Ok(result.rows_affected())
}

pub async fn cleanup_expired_tasks(pool: &AnyPool, ttl_hours: u64) -> Result<u64> {
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(ttl_hours as i64);
    let cutoff_str = cutoff.naive_utc().to_string();

    let result = sqlx::query(
        "DELETE FROM task_entry WHERE is_running = 0 AND end_time IS NOT NULL AND end_time < ?",
    )
    .bind(&cutoff_str)
    .execute(pool)
    .await
    .map_err(Error::Database)?;

    Ok(result.rows_affected())
}

pub async fn append_task_log(pool: &AnyPool, task_id: &str, entry: &TaskLogEntry) -> Result<()> {
    sqlx::query(
        "INSERT INTO task_log (task_id, timestamp, level, message) VALUES (?, ?, ?, ?)",
    )
    .bind(task_id)
    .bind(&entry.timestamp)
    .bind(&entry.level)
    .bind(&entry.message)
    .execute(pool)
    .await
    .map_err(Error::Database)?;

    Ok(())
}

pub async fn read_task_logs(pool: &AnyPool, task_id: &str) -> Result<Vec<TaskLogEntry>> {
    let rows = sqlx::query(
        "SELECT timestamp, level, message FROM task_log WHERE task_id = ? ORDER BY id ASC",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)?;

    let entries = rows
        .into_iter()
        .map(|row| TaskLogEntry {
            timestamp: row.get::<String, _>(0),
            level: row.get::<String, _>(1),
            message: row.get::<String, _>(2),
        })
        .collect();

    Ok(entries)
}

pub async fn append_task_progress(pool: &AnyPool, task_id: &str, seq: i32, message: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO task_progress (task_id, seq, message) VALUES (?, ?, ?)",
    )
    .bind(task_id)
    .bind(seq)
    .bind(message)
    .execute(pool)
    .await
    .map_err(Error::Database)?;

    Ok(())
}

pub async fn read_task_progress(pool: &AnyPool, task_id: &str) -> Result<Vec<String>> {
    let rows = sqlx::query(
        "SELECT message FROM task_progress WHERE task_id = ? ORDER BY seq ASC",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await
    .map_err(Error::Database)?;

    Ok(rows.into_iter().map(|row| row.get::<String, _>(0)).collect())
}

fn task_type_str(task_type: &TaskType) -> &'static str {
    match task_type {
        TaskType::NodeGraph => "node_graph",
        TaskType::AgentService => "agent_service",
    }
}

fn task_status_str(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Running => "running",
        TaskStatus::Success => "success",
        TaskStatus::Failed => "failed",
        TaskStatus::Stopped => "stopped",
    }
}

/// Build an `AnyPool` from a SQLite pool by going through the connection URL.
/// The caller must ensure `sqlx::any::install_default_drivers()` was called at startup.
pub async fn sqlite_any_pool(path: &str) -> Result<Arc<AnyPool>> {
    let db_url = format!("sqlite://{}?mode=rwc", path);
    let pool = sqlx::any::AnyPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await
        .map_err(Error::Database)?;
    Ok(Arc::new(pool))
}
