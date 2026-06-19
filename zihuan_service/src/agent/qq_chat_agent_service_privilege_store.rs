use std::sync::Arc;

use chrono::{Duration, Local, NaiveDateTime};
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use sqlx::mysql::MySqlRow;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use tokio::task::block_in_place;
use zihuan_core::data_refs::{MySqlConfig, RelationalDbConnection, SqliteConfig};
use zihuan_core::error::{Error, Result};

const AUTH_TTL_MINUTES: i64 = 5;
const MAX_FAILED_ATTEMPTS: i32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatAgentServicePrivilegeAuthRecord {
    pub id: i64,
    pub agent_id: String,
    pub sender_id: String,
    pub auth_key: String,
    pub purpose: String,
    pub pending_task_id: Option<String>,
    pub pending_target_id: Option<String>,
    pub pending_group_id: Option<i64>,
    pub pending_is_group: bool,
    pub failed_attempts: i32,
    pub expires_at: String,
    pub elevated_until: Option<String>,
    pub consumed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub id: i64,
    pub agent_id: String,
    pub sender_id: String,
    pub purpose: String,
    pub auth_key: String,
    pub failed_attempts: i32,
    pub expires_at: String,
    pub elevated_until: Option<String>,
    pub consumed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub enum PrivilegeAuthStatus {
    Elevated {
        until: String,
        record: QqChatAgentServicePrivilegeAuthRecord,
    },
    Pending(QqChatAgentServicePrivilegeAuthRecord),
    NotFound,
    Failed(String),
}

pub async fn create_privilege_auth(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    purpose: &str,
    pending_task_id: Option<&str>,
    pending_target_id: Option<&str>,
    pending_group_id: Option<i64>,
    pending_is_group: bool,
) -> Result<QqChatAgentServicePrivilegeAuthRecord> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            create_privilege_auth_mysql(
                config,
                agent_id,
                sender_id,
                purpose,
                pending_task_id,
                pending_target_id,
                pending_group_id,
                pending_is_group,
            )
            .await
        }
        RelationalDbConnection::Sqlite(config) => {
            create_privilege_auth_sqlite(
                config,
                agent_id,
                sender_id,
                purpose,
                pending_task_id,
                pending_target_id,
                pending_group_id,
                pending_is_group,
            )
            .await
        }
    }
}

pub async fn verify_privilege_auth(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    auth_key: &str,
) -> Result<PrivilegeAuthStatus> {
    match connection {
        RelationalDbConnection::MySql(config) => verify_privilege_auth_mysql(config, agent_id, sender_id, auth_key).await,
        RelationalDbConnection::Sqlite(config) => {
            verify_privilege_auth_sqlite(config, agent_id, sender_id, auth_key).await
        }
    }
}

pub async fn has_active_privilege(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
) -> Result<bool> {
    match connection {
        RelationalDbConnection::MySql(config) => has_active_privilege_mysql(config, agent_id, sender_id).await,
        RelationalDbConnection::Sqlite(config) => has_active_privilege_sqlite(config, agent_id, sender_id).await,
    }
}

pub async fn list_recent_notifications(
    connection: &RelationalDbConnection,
    limit: i64,
) -> Result<Vec<NotificationRecord>> {
    match connection {
        RelationalDbConnection::MySql(config) => list_recent_notifications_mysql(config, limit).await,
        RelationalDbConnection::Sqlite(config) => list_recent_notifications_sqlite(config, limit).await,
    }
}

pub async fn delete_all_notifications(
    connection: &RelationalDbConnection,
) -> Result<u64> {
    match connection {
        RelationalDbConnection::MySql(config) => delete_all_notifications_mysql(config).await,
        RelationalDbConnection::Sqlite(config) => delete_all_notifications_sqlite(config).await,
    }
}

pub fn has_active_privilege_blocking(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
) -> Result<bool> {
    let connection = connection.clone();
    let agent_id = agent_id.to_string();
    let sender_id = sender_id.to_string();
    let run = async move { has_active_privilege(&connection, &agent_id, &sender_id).await };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}

fn generate_auth_key() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect::<String>()
        .to_uppercase()
}

async fn create_privilege_auth_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    sender_id: &str,
    purpose: &str,
    pending_task_id: Option<&str>,
    pending_target_id: Option<&str>,
    pending_group_id: Option<i64>,
    pending_is_group: bool,
) -> Result<QqChatAgentServicePrivilegeAuthRecord> {
    let now = Local::now().naive_local();
    let expires_at = now + Duration::minutes(AUTH_TTL_MINUTES);
    let auth_key = generate_auth_key();

    sqlx::query(
        "INSERT INTO qq_chat_agent_service_privilege_auth \
         (agent_id, sender_id, auth_key, purpose, pending_task_id, pending_target_id, pending_group_id, pending_is_group, failed_attempts, expires_at, elevated_until, consumed, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, NULL, 0, ?, ?)",
    )
    .bind(agent_id)
    .bind(sender_id)
    .bind(&auth_key)
    .bind(purpose)
    .bind(pending_task_id)
    .bind(pending_target_id)
    .bind(pending_group_id)
    .bind(pending_is_group)
    .bind(expires_at)
    .bind(now)
    .bind(now)
    .execute(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;

    latest_auth_mysql(config, agent_id, sender_id).await
}

async fn create_privilege_auth_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    sender_id: &str,
    purpose: &str,
    pending_task_id: Option<&str>,
    pending_target_id: Option<&str>,
    pending_group_id: Option<i64>,
    pending_is_group: bool,
) -> Result<QqChatAgentServicePrivilegeAuthRecord> {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let expires_at = (Local::now() + Duration::minutes(AUTH_TTL_MINUTES))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let auth_key = generate_auth_key();

    sqlx::query(
        "INSERT INTO qq_chat_agent_service_privilege_auth \
         (agent_id, sender_id, auth_key, purpose, pending_task_id, pending_target_id, pending_group_id, pending_is_group, failed_attempts, expires_at, elevated_until, consumed, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, NULL, 0, ?, ?)",
    )
    .bind(agent_id)
    .bind(sender_id)
    .bind(&auth_key)
    .bind(purpose)
    .bind(pending_task_id)
    .bind(pending_target_id)
    .bind(pending_group_id)
    .bind(pending_is_group)
    .bind(&expires_at)
    .bind(&now)
    .bind(&now)
    .execute(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;

    latest_auth_sqlite(config, agent_id, sender_id).await
}

async fn verify_privilege_auth_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    sender_id: &str,
    auth_key: &str,
) -> Result<PrivilegeAuthStatus> {
    let Some(record) = latest_auth_optional_mysql(config, agent_id, sender_id).await? else {
        return Ok(PrivilegeAuthStatus::NotFound);
    };
    let now = Local::now().naive_local();
    let expires_at = parse_mysql_timestamp(&record.expires_at)?;

    if record.consumed || now > expires_at {
        return Ok(PrivilegeAuthStatus::Failed("密钥已过期，请重新触发命令生成新的密钥。".to_string()));
    }

    if record.auth_key != auth_key.trim() {
        let next_attempts = record.failed_attempts + 1;
        let consumed = next_attempts >= MAX_FAILED_ATTEMPTS;
        sqlx::query(
            "UPDATE qq_chat_agent_service_privilege_auth \
             SET failed_attempts = ?, consumed = ?, updated_at = ? WHERE id = ?",
        )
        .bind(next_attempts)
        .bind(consumed)
        .bind(now)
        .bind(record.id)
        .execute(mysql_pool(config)?)
        .await
        .map_err(Error::Database)?;

        return if consumed {
            Ok(PrivilegeAuthStatus::Failed("密钥连续输错 2 次，已作废，请重新触发命令。".to_string()))
        } else {
            Ok(PrivilegeAuthStatus::Failed("密钥错误，请重新输入。".to_string()))
        };
    }

    let elevated_until = now + Duration::minutes(AUTH_TTL_MINUTES);
    sqlx::query(
        "UPDATE qq_chat_agent_service_privilege_auth \
         SET consumed = 1, elevated_until = ?, updated_at = ? WHERE id = ?",
    )
    .bind(elevated_until)
    .bind(now)
    .bind(record.id)
    .execute(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(PrivilegeAuthStatus::Elevated {
        until: format_mysql_timestamp(elevated_until),
        record,
    })
}

async fn verify_privilege_auth_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    sender_id: &str,
    auth_key: &str,
) -> Result<PrivilegeAuthStatus> {
    let Some(record) = latest_auth_optional_sqlite(config, agent_id, sender_id).await? else {
        return Ok(PrivilegeAuthStatus::NotFound);
    };
    let now = Local::now();
    let expires_at = parse_sqlite_timestamp(&record.expires_at)?;

    if record.consumed || now > expires_at {
        return Ok(PrivilegeAuthStatus::Failed("密钥已过期，请重新触发命令生成新的密钥。".to_string()));
    }

    if record.auth_key != auth_key.trim() {
        let next_attempts = record.failed_attempts + 1;
        let consumed = next_attempts >= MAX_FAILED_ATTEMPTS;
        sqlx::query(
            "UPDATE qq_chat_agent_service_privilege_auth \
             SET failed_attempts = ?, consumed = ?, updated_at = ? WHERE id = ?",
        )
        .bind(next_attempts)
        .bind(consumed)
        .bind(now.format("%Y-%m-%d %H:%M:%S").to_string())
        .bind(record.id)
        .execute(sqlite_pool(config)?)
        .await
        .map_err(Error::Database)?;

        return if consumed {
            Ok(PrivilegeAuthStatus::Failed("密钥连续输错 2 次，已作废，请重新触发命令。".to_string()))
        } else {
            Ok(PrivilegeAuthStatus::Failed("密钥错误，请重新输入。".to_string()))
        };
    }

    let elevated_until = (now + Duration::minutes(AUTH_TTL_MINUTES))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    sqlx::query(
        "UPDATE qq_chat_agent_service_privilege_auth \
         SET consumed = 1, elevated_until = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&elevated_until)
    .bind(now.format("%Y-%m-%d %H:%M:%S").to_string())
    .bind(record.id)
    .execute(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(PrivilegeAuthStatus::Elevated {
        until: elevated_until,
        record,
    })
}

async fn has_active_privilege_mysql(config: &Arc<MySqlConfig>, agent_id: &str, sender_id: &str) -> Result<bool> {
    let row = sqlx::query(
        "SELECT elevated_until FROM qq_chat_agent_service_privilege_auth \
         WHERE agent_id = ? AND sender_id = ? AND elevated_until IS NOT NULL \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(agent_id)
    .bind(sender_id)
    .fetch_optional(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;
    let Some(row) = row else {
        return Ok(false);
    };
    let elevated_until = row.get::<NaiveDateTime, _>("elevated_until");
    Ok(Local::now().naive_local() <= elevated_until)
}

async fn has_active_privilege_sqlite(config: &Arc<SqliteConfig>, agent_id: &str, sender_id: &str) -> Result<bool> {
    let row = sqlx::query(
        "SELECT elevated_until FROM qq_chat_agent_service_privilege_auth \
         WHERE agent_id = ? AND sender_id = ? AND elevated_until IS NOT NULL \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(agent_id)
    .bind(sender_id)
    .fetch_optional(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;
    let Some(row) = row else {
        return Ok(false);
    };
    let elevated_until: String = row.get("elevated_until");
    Ok(Local::now() <= parse_sqlite_timestamp(&elevated_until)?)
}

async fn list_recent_notifications_mysql(
    config: &Arc<MySqlConfig>,
    limit: i64,
) -> Result<Vec<NotificationRecord>> {
    let rows = sqlx::query(
        "SELECT id, agent_id, sender_id, purpose, auth_key, failed_attempts, expires_at, elevated_until, consumed, created_at, updated_at \
         FROM qq_chat_agent_service_privilege_auth ORDER BY created_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;
    Ok(rows.into_iter().map(map_notification_card_mysql_row).collect())
}

async fn list_recent_notifications_sqlite(
    config: &Arc<SqliteConfig>,
    limit: i64,
) -> Result<Vec<NotificationRecord>> {
    let rows = sqlx::query(
        "SELECT id, agent_id, sender_id, purpose, auth_key, failed_attempts, expires_at, elevated_until, consumed, created_at, updated_at \
         FROM qq_chat_agent_service_privilege_auth ORDER BY created_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;
    Ok(rows.into_iter().map(map_notification_card_sqlite_row).collect())
}

async fn latest_auth_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    sender_id: &str,
) -> Result<QqChatAgentServicePrivilegeAuthRecord> {
    latest_auth_optional_mysql(config, agent_id, sender_id)
        .await?
        .ok_or_else(|| Error::ValidationError("privilege auth record missing after insert".to_string()))
}

async fn latest_auth_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    sender_id: &str,
) -> Result<QqChatAgentServicePrivilegeAuthRecord> {
    latest_auth_optional_sqlite(config, agent_id, sender_id)
        .await?
        .ok_or_else(|| Error::ValidationError("privilege auth record missing after insert".to_string()))
}

async fn latest_auth_optional_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    sender_id: &str,
) -> Result<Option<QqChatAgentServicePrivilegeAuthRecord>> {
    let row = sqlx::query(
        "SELECT id, agent_id, sender_id, auth_key, purpose, pending_task_id, pending_target_id, pending_group_id, pending_is_group, failed_attempts, expires_at, elevated_until, consumed, created_at, updated_at \
         FROM qq_chat_agent_service_privilege_auth WHERE agent_id = ? AND sender_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(agent_id)
    .bind(sender_id)
    .fetch_optional(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;
    Ok(row.map(map_privilege_auth_mysql_row))
}

async fn latest_auth_optional_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    sender_id: &str,
) -> Result<Option<QqChatAgentServicePrivilegeAuthRecord>> {
    let row = sqlx::query(
        "SELECT id, agent_id, sender_id, auth_key, purpose, pending_task_id, pending_target_id, pending_group_id, pending_is_group, failed_attempts, expires_at, elevated_until, consumed, created_at, updated_at \
         FROM qq_chat_agent_service_privilege_auth WHERE agent_id = ? AND sender_id = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(agent_id)
    .bind(sender_id)
    .fetch_optional(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;
    Ok(row.map(map_privilege_auth_sqlite_row))
}

fn map_privilege_auth_mysql_row(row: MySqlRow) -> QqChatAgentServicePrivilegeAuthRecord {
    QqChatAgentServicePrivilegeAuthRecord {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        sender_id: row.get("sender_id"),
        auth_key: row.get("auth_key"),
        purpose: row.get("purpose"),
        pending_task_id: row.get("pending_task_id"),
        pending_target_id: row.get("pending_target_id"),
        pending_group_id: row.get("pending_group_id"),
        pending_is_group: row.get::<i8, _>("pending_is_group") != 0,
        failed_attempts: row.get("failed_attempts"),
        expires_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("expires_at")),
        elevated_until: row
            .get::<Option<NaiveDateTime>, _>("elevated_until")
            .map(format_mysql_timestamp),
        consumed: row.get::<i8, _>("consumed") != 0,
        created_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("created_at")),
        updated_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("updated_at")),
    }
}

fn map_privilege_auth_sqlite_row(row: SqliteRow) -> QqChatAgentServicePrivilegeAuthRecord {
    QqChatAgentServicePrivilegeAuthRecord {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        sender_id: row.get("sender_id"),
        auth_key: row.get("auth_key"),
        purpose: row.get("purpose"),
        pending_task_id: row.get("pending_task_id"),
        pending_target_id: row.get("pending_target_id"),
        pending_group_id: row.get("pending_group_id"),
        pending_is_group: row.get::<i64, _>("pending_is_group") != 0,
        failed_attempts: row.get("failed_attempts"),
        expires_at: row.get("expires_at"),
        elevated_until: row.get("elevated_until"),
        consumed: row.get::<i64, _>("consumed") != 0,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn map_notification_card_mysql_row(row: MySqlRow) -> NotificationRecord {
    NotificationRecord {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        sender_id: row.get("sender_id"),
        purpose: row.get("purpose"),
        auth_key: row.get("auth_key"),
        failed_attempts: row.get("failed_attempts"),
        expires_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("expires_at")),
        elevated_until: row
            .get::<Option<NaiveDateTime>, _>("elevated_until")
            .map(format_mysql_timestamp),
        consumed: row.get::<i8, _>("consumed") != 0,
        created_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("created_at")),
        updated_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("updated_at")),
    }
}

fn map_notification_card_sqlite_row(row: SqliteRow) -> NotificationRecord {
    NotificationRecord {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        sender_id: row.get("sender_id"),
        purpose: row.get("purpose"),
        auth_key: row.get("auth_key"),
        failed_attempts: row.get("failed_attempts"),
        expires_at: row.get("expires_at"),
        elevated_until: row.get("elevated_until"),
        consumed: row.get::<i64, _>("consumed") != 0,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn mysql_pool(config: &Arc<MySqlConfig>) -> Result<&sqlx::mysql::MySqlPool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("privilege-auth mysql pool is not initialized".to_string()))
}

fn sqlite_pool(config: &Arc<SqliteConfig>) -> Result<&sqlx::sqlite::SqlitePool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("privilege-auth sqlite pool is not initialized".to_string()))
}

async fn delete_all_notifications_mysql(config: &Arc<MySqlConfig>) -> Result<u64> {
    let result = sqlx::query("DELETE FROM qq_chat_agent_service_privilege_auth")
        .execute(mysql_pool(config)?)
        .await
        .map_err(Error::Database)?;
    Ok(result.rows_affected())
}

async fn delete_all_notifications_sqlite(config: &Arc<SqliteConfig>) -> Result<u64> {
    let result = sqlx::query("DELETE FROM qq_chat_agent_service_privilege_auth")
        .execute(sqlite_pool(config)?)
        .await
        .map_err(Error::Database)?;
    Ok(result.rows_affected())
}

fn format_mysql_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn parse_mysql_timestamp(value: &str) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map_err(|err| Error::ValidationError(format!("invalid mysql timestamp '{value}': {err}")))
}

fn parse_sqlite_timestamp(value: &str) -> Result<chrono::DateTime<Local>> {
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
        .map_err(|err| Error::ValidationError(format!("invalid sqlite timestamp '{value}': {err}")))?;
    Ok(chrono::TimeZone::from_local_datetime(&Local, &naive)
        .single()
        .ok_or_else(|| Error::ValidationError(format!("ambiguous sqlite timestamp '{value}'")))?)
}
