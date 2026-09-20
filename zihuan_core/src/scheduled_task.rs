use crate::data_refs::RelationalDbConnection;
use crate::error::{Error, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

/// Where a scheduled task stands between being written and being finished: still waiting,
/// being run, or already settled.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledTaskStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// One planned execution of a job at a fixed time. The record is written when the task is
/// scheduled, updated with the outcome once it runs, and kept afterwards as history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledTaskEntry {
    pub id: String,
    pub task_name: String,
    pub source_service: String,
    pub triggered_by: Option<String>,
    pub start_time: DateTime<Local>,
    pub end_time: Option<DateTime<Local>>,
    pub status: ScheduledTaskStatus,
    pub related_task_ids: Vec<String>,
    pub info_summary: Option<String>,
}

impl ScheduledTaskEntry {
    pub fn new(
        task_name: impl Into<String>,
        source_service: impl Into<String>,
        triggered_by: Option<String>,
        start_time: DateTime<Local>,
        info_summary: Option<&str>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            task_name: task_name.into(),
            source_service: source_service.into(),
            triggered_by,
            start_time,
            end_time: None,
            status: ScheduledTaskStatus::Pending,
            related_task_ids: Vec::new(),
            info_summary: info_summary.map(str::to_string),
        }
    }
}

fn pool_missing() -> Error {
    crate::string_error!("scheduled task database pool is unavailable")
}

pub async fn insert_task(
    connection: &RelationalDbConnection,
    entry: &ScheduledTaskEntry,
) -> Result<()> {
    let related = serde_json::to_string(&entry.related_task_ids)
        .map_err(|err| crate::string_error!("serialize related task ids: {err}"))?;
    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("INSERT INTO scheduled_task (id, task_name, source_service, triggered_by, start_time, end_time, status, related_task_ids, info_summary) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(&entry.id).bind(&entry.task_name).bind(&entry.source_service).bind(&entry.triggered_by)
                .bind(entry.start_time).bind(entry.end_time).bind(status_name(&entry.status)).bind(related).bind(&entry.info_summary)
                .execute(pool).await.map_err(Error::Database)?;
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("INSERT INTO scheduled_task (id, task_name, source_service, triggered_by, start_time, end_time, status, related_task_ids, info_summary) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
                .bind(&entry.id).bind(&entry.task_name).bind(&entry.source_service).bind(&entry.triggered_by)
                .bind(entry.start_time.to_rfc3339()).bind(entry.end_time.map(|value| value.to_rfc3339())).bind(status_name(&entry.status)).bind(related).bind(&entry.info_summary)
                .execute(pool).await.map_err(Error::Database)?;
        }
    }
    Ok(())
}

pub async fn cancel_pending_tasks(
    connection: &RelationalDbConnection,
    task_name: &str,
    source_service: &str,
    triggered_by: &str,
    summary: Option<&str>,
) -> Result<()> {
    let now = Local::now();
    let summary = summary.unwrap_or("被新任务替换");
    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("UPDATE scheduled_task SET status = 'cancelled', end_time = ?, info_summary = ? WHERE task_name = ? AND source_service = ? AND triggered_by = ? AND status = 'pending'")
                .bind(now).bind(summary).bind(task_name).bind(source_service).bind(triggered_by).execute(pool).await.map_err(Error::Database)?;
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("UPDATE scheduled_task SET status = 'cancelled', end_time = ?, info_summary = ? WHERE task_name = ? AND source_service = ? AND triggered_by = ? AND status = 'pending'")
                .bind(now.to_rfc3339()).bind(summary).bind(task_name).bind(source_service).bind(triggered_by).execute(pool).await.map_err(Error::Database)?;
        }
    }
    Ok(())
}

/// Atomically claims a pending task for execution: only one caller can flip a task from
/// `pending` to `running`, which doubles as the concurrency guard across ticks.
pub async fn claim_task(connection: &RelationalDbConnection, id: &str) -> Result<bool> {
    let affected = match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("UPDATE scheduled_task SET status = 'running', info_summary = '执行中' WHERE id = ? AND status = 'pending'")
                .bind(id)
                .execute(pool)
                .await
                .map_err(Error::Database)?
                .rows_affected()
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("UPDATE scheduled_task SET status = 'running', info_summary = '执行中' WHERE id = ? AND status = 'pending'")
                .bind(id)
                .execute(pool)
                .await
                .map_err(Error::Database)?
                .rows_affected()
        }
    };
    Ok(affected > 0)
}

/// Puts `running` tasks of one service back to `pending`. Called when a service registers
/// with the scheduler to recover tasks left running by a previous process crash.
pub async fn requeue_running_tasks(
    connection: &RelationalDbConnection,
    source_service: &str,
) -> Result<()> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("UPDATE scheduled_task SET status = 'pending', info_summary = '重启后重新排队' WHERE source_service = ? AND status = 'running'")
                .bind(source_service)
                .execute(pool)
                .await
                .map_err(Error::Database)?;
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("UPDATE scheduled_task SET status = 'pending', info_summary = '重启后重新排队' WHERE source_service = ? AND status = 'running'")
                .bind(source_service)
                .execute(pool)
                .await
                .map_err(Error::Database)?;
        }
    }
    Ok(())
}

pub async fn finish_task(
    connection: &RelationalDbConnection,
    id: &str,
    status: ScheduledTaskStatus,
    summary: Option<&str>,
) -> Result<()> {
    let now = Local::now();
    match connection {
        RelationalDbConnection::MySql(config) => {
            sqlx::query(
                "UPDATE scheduled_task SET status = ?, end_time = ?, info_summary = ? WHERE id = ?",
            )
            .bind(status_name(&status))
            .bind(now)
            .bind(summary)
            .bind(id)
            .execute(config.pool.as_ref().ok_or_else(pool_missing)?)
            .await
            .map_err(Error::Database)?;
        }
        RelationalDbConnection::Sqlite(config) => {
            sqlx::query(
                "UPDATE scheduled_task SET status = ?, end_time = ?, info_summary = ? WHERE id = ?",
            )
            .bind(status_name(&status))
            .bind(now.to_rfc3339())
            .bind(summary)
            .bind(id)
            .execute(config.pool.as_ref().ok_or_else(pool_missing)?)
            .await
            .map_err(Error::Database)?;
        }
    }
    Ok(())
}

pub async fn insert_dream_memory(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    chars: i64,
    content: &str,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    let now = Local::now();
    match connection {
        RelationalDbConnection::MySql(config) => {
            sqlx::query("INSERT INTO dream_memory (id, agent_id, sender_id, created_at, chat_text_char_count, memory_content) VALUES (?, ?, ?, ?, ?, ?)").bind(id).bind(agent_id).bind(sender_id).bind(now).bind(chars).bind(content).execute(config.pool.as_ref().ok_or_else(pool_missing)?).await.map_err(Error::Database)?;
        }
        RelationalDbConnection::Sqlite(config) => {
            sqlx::query("INSERT INTO dream_memory (id, agent_id, sender_id, created_at, chat_text_char_count, memory_content) VALUES (?, ?, ?, ?, ?, ?)").bind(id).bind(agent_id).bind(sender_id).bind(now.to_rfc3339()).bind(chars).bind(content).execute(config.pool.as_ref().ok_or_else(pool_missing)?).await.map_err(Error::Database)?;
        }
    }
    Ok(())
}

pub async fn latest_dream_memory(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
) -> Result<Option<String>> {
    match connection {
        RelationalDbConnection::MySql(config) => sqlx::query_scalar("SELECT memory_content FROM dream_memory WHERE agent_id = ? AND sender_id = ? ORDER BY created_at DESC LIMIT 1").bind(agent_id).bind(sender_id).fetch_optional(config.pool.as_ref().ok_or_else(pool_missing)?).await.map_err(Error::Database),
        RelationalDbConnection::Sqlite(config) => sqlx::query_scalar("SELECT memory_content FROM dream_memory WHERE agent_id = ? AND sender_id = ? ORDER BY created_at DESC LIMIT 1").bind(agent_id).bind(sender_id).fetch_optional(config.pool.as_ref().ok_or_else(pool_missing)?).await.map_err(Error::Database),
    }
}

pub async fn list_tasks(
    connection: &RelationalDbConnection,
    source_service: Option<&str>,
    status: Option<&str>,
) -> Result<Vec<ScheduledTaskEntry>> {
    let mut sql = "SELECT id, task_name, source_service, triggered_by, start_time, end_time, status, related_task_ids, info_summary FROM scheduled_task".to_string();
    if source_service.is_some() || status.is_some() {
        sql.push_str(" WHERE ");
    }
    if source_service.is_some() {
        sql.push_str("source_service = ?");
    }
    if source_service.is_some() && status.is_some() {
        sql.push_str(" AND ");
    }
    if status.is_some() {
        sql.push_str("status = ?");
    }
    sql.push_str(" ORDER BY start_time DESC");
    match connection {
        RelationalDbConnection::MySql(config) => {
            let mut query = sqlx::query(&sql);
            if let Some(value) = source_service {
                query = query.bind(value);
            }
            if let Some(value) = status {
                query = query.bind(value);
            }
            query
                .fetch_all(config.pool.as_ref().ok_or_else(pool_missing)?)
                .await
                .map_err(Error::Database)?
                .into_iter()
                .map(parse_mysql_row)
                .collect()
        }
        RelationalDbConnection::Sqlite(config) => {
            let mut query = sqlx::query(&sql);
            if let Some(value) = source_service {
                query = query.bind(value);
            }
            if let Some(value) = status {
                query = query.bind(value);
            }
            query
                .fetch_all(config.pool.as_ref().ok_or_else(pool_missing)?)
                .await
                .map_err(Error::Database)?
                .into_iter()
                .map(parse_sqlite_row)
                .collect()
        }
    }
}

/// Lists pending tasks whose scheduled start time has arrived; the scheduler kernel claims
/// each of them with [`claim_task`] before running its job.
pub async fn list_due_tasks(
    connection: &RelationalDbConnection,
    now: DateTime<Local>,
) -> Result<Vec<ScheduledTaskEntry>> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("SELECT id, task_name, source_service, triggered_by, start_time, end_time, status, related_task_ids, info_summary FROM scheduled_task WHERE status = 'pending' AND start_time <= ? ORDER BY start_time ASC")
                .bind(now)
                .fetch_all(pool)
                .await
                .map_err(Error::Database)?
                .into_iter()
                .map(parse_mysql_row)
                .collect()
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config.pool.as_ref().ok_or_else(pool_missing)?;
            sqlx::query("SELECT id, task_name, source_service, triggered_by, start_time, end_time, status, related_task_ids, info_summary FROM scheduled_task WHERE status = 'pending' AND start_time <= ? ORDER BY start_time ASC")
                .bind(now.to_rfc3339())
                .fetch_all(pool)
                .await
                .map_err(Error::Database)?
                .into_iter()
                .map(parse_sqlite_row)
                .collect()
        }
    }
}

fn status_name(status: &ScheduledTaskStatus) -> &'static str {
    match status {
        ScheduledTaskStatus::Pending => "pending",
        ScheduledTaskStatus::Running => "running",
        ScheduledTaskStatus::Succeeded => "succeeded",
        ScheduledTaskStatus::Failed => "failed",
        ScheduledTaskStatus::Cancelled => "cancelled",
    }
}
fn parse_status(value: String) -> Result<ScheduledTaskStatus> {
    match value.as_str() {
        "pending" => Ok(ScheduledTaskStatus::Pending),
        "running" => Ok(ScheduledTaskStatus::Running),
        "succeeded" => Ok(ScheduledTaskStatus::Succeeded),
        "failed" => Ok(ScheduledTaskStatus::Failed),
        "cancelled" => Ok(ScheduledTaskStatus::Cancelled),
        _ => Err(crate::string_error!("unknown scheduled task status '{value}'")),
    }
}
fn related(row: &sqlx::mysql::MySqlRow) -> Result<Vec<String>> {
    serde_json::from_str(
        &row.try_get::<Option<String>, _>("related_task_ids")
            .map_err(Error::Database)?
            .unwrap_or_else(|| "[]".to_string()),
    )
    .map_err(|err| crate::string_error!("parse related task ids: {err}"))
}
fn parse_mysql_row(row: sqlx::mysql::MySqlRow) -> Result<ScheduledTaskEntry> {
    Ok(ScheduledTaskEntry {
        id: row.try_get("id").map_err(Error::Database)?,
        task_name: row.try_get("task_name").map_err(Error::Database)?,
        source_service: row.try_get("source_service").map_err(Error::Database)?,
        triggered_by: row.try_get("triggered_by").map_err(Error::Database)?,
        start_time: row.try_get("start_time").map_err(Error::Database)?,
        end_time: row.try_get("end_time").map_err(Error::Database)?,
        status: parse_status(row.try_get("status").map_err(Error::Database)?)?,
        related_task_ids: related(&row)?,
        info_summary: row.try_get("info_summary").map_err(Error::Database)?,
    })
}
fn parse_sqlite_row(row: sqlx::sqlite::SqliteRow) -> Result<ScheduledTaskEntry> {
    let start: String = row.try_get("start_time").map_err(Error::Database)?;
    let end: Option<String> = row.try_get("end_time").map_err(Error::Database)?;
    Ok(ScheduledTaskEntry {
        id: row.try_get("id").map_err(Error::Database)?,
        task_name: row.try_get("task_name").map_err(Error::Database)?,
        source_service: row.try_get("source_service").map_err(Error::Database)?,
        triggered_by: row.try_get("triggered_by").map_err(Error::Database)?,
        start_time: DateTime::parse_from_rfc3339(&start)
            .map_err(|err| crate::string_error!("{}", err))?
            .with_timezone(&Local),
        end_time: end
            .map(|value| {
                DateTime::parse_from_rfc3339(&value)
                    .map(|date| date.with_timezone(&Local))
                    .map_err(|err| crate::string_error!("{}", err))
            })
            .transpose()?,
        status: parse_status(row.try_get("status").map_err(Error::Database)?)?,
        related_task_ids: serde_json::from_str(
            &row.try_get::<Option<String>, _>("related_task_ids")
                .map_err(Error::Database)?
                .unwrap_or_else(|| "[]".to_string()),
        )
        .map_err(|err| crate::string_error!("{}", err))?,
        info_summary: row.try_get("info_summary").map_err(Error::Database)?,
    })
}
