use std::sync::Arc;

use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sqlx::mysql::MySqlRow;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use tokio::task::block_in_place;
use zihuan_core::data_refs::{MySqlConfig, RelationalDbConnection, SqliteConfig};
use zihuan_core::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatAgentServiceToolQuota {
    pub id: i64,
    pub agent_id: String,
    pub sender_id: String,
    pub tool_name: String,
    pub remaining_calls: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get_tool_quota(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    tool_name: &str,
) -> Result<Option<QqChatAgentServiceToolQuota>> {
    match connection {
        RelationalDbConnection::MySql(config) => get_tool_quota_mysql(config, agent_id, sender_id, tool_name).await,
        RelationalDbConnection::Sqlite(config) => get_tool_quota_sqlite(config, agent_id, sender_id, tool_name).await,
    }
}

pub async fn decrement_tool_quota_if_needed(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    tool_name: &str,
) -> Result<bool> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            decrement_tool_quota_if_needed_mysql(config, agent_id, sender_id, tool_name).await
        }
        RelationalDbConnection::Sqlite(config) => {
            decrement_tool_quota_if_needed_sqlite(config, agent_id, sender_id, tool_name).await
        }
    }
}

pub fn decrement_tool_quota_if_needed_blocking(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    tool_name: &str,
) -> Result<bool> {
    let connection = connection.clone();
    let agent_id = agent_id.to_string();
    let sender_id = sender_id.to_string();
    let tool_name = tool_name.to_string();
    let run =
        async move { decrement_tool_quota_if_needed(&connection, &agent_id, &sender_id, &tool_name).await };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}

async fn get_tool_quota_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    sender_id: &str,
    tool_name: &str,
) -> Result<Option<QqChatAgentServiceToolQuota>> {
    let row = sqlx::query(
        "SELECT id, agent_id, sender_id, tool_name, remaining_calls, created_at, updated_at \
         FROM qq_chat_agent_service_tool_quota WHERE agent_id = ? AND sender_id = ? AND tool_name = ? LIMIT 1",
    )
    .bind(agent_id)
    .bind(sender_id)
    .bind(tool_name)
    .fetch_optional(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(row.map(map_tool_quota_mysql_row))
}

async fn get_tool_quota_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    sender_id: &str,
    tool_name: &str,
) -> Result<Option<QqChatAgentServiceToolQuota>> {
    let row = sqlx::query(
        "SELECT id, agent_id, sender_id, tool_name, remaining_calls, created_at, updated_at \
         FROM qq_chat_agent_service_tool_quota WHERE agent_id = ? AND sender_id = ? AND tool_name = ? LIMIT 1",
    )
    .bind(agent_id)
    .bind(sender_id)
    .bind(tool_name)
    .fetch_optional(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(row.map(map_tool_quota_sqlite_row))
}

async fn decrement_tool_quota_if_needed_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    sender_id: &str,
    tool_name: &str,
) -> Result<bool> {
    let existing = get_tool_quota_mysql(config, agent_id, sender_id, tool_name).await?;
    let Some(existing) = existing else {
        return Ok(true);
    };
    let Some(remaining_calls) = existing.remaining_calls else {
        return Ok(true);
    };
    if remaining_calls <= 0 {
        return Ok(false);
    }

    let now = Local::now().naive_local();
    let result = sqlx::query(
        "UPDATE qq_chat_agent_service_tool_quota \
         SET remaining_calls = remaining_calls - 1, updated_at = ? \
         WHERE agent_id = ? AND sender_id = ? AND tool_name = ? AND remaining_calls > 0",
    )
    .bind(now)
    .bind(agent_id)
    .bind(sender_id)
    .bind(tool_name)
    .execute(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(result.rows_affected() > 0)
}

async fn decrement_tool_quota_if_needed_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    sender_id: &str,
    tool_name: &str,
) -> Result<bool> {
    let existing = get_tool_quota_sqlite(config, agent_id, sender_id, tool_name).await?;
    let Some(existing) = existing else {
        return Ok(true);
    };
    let Some(remaining_calls) = existing.remaining_calls else {
        return Ok(true);
    };
    if remaining_calls <= 0 {
        return Ok(false);
    }

    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let result = sqlx::query(
        "UPDATE qq_chat_agent_service_tool_quota \
         SET remaining_calls = remaining_calls - 1, updated_at = ? \
         WHERE agent_id = ? AND sender_id = ? AND tool_name = ? AND remaining_calls > 0",
    )
    .bind(now)
    .bind(agent_id)
    .bind(sender_id)
    .bind(tool_name)
    .execute(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(result.rows_affected() > 0)
}

fn map_tool_quota_mysql_row(row: MySqlRow) -> QqChatAgentServiceToolQuota {
    QqChatAgentServiceToolQuota {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        sender_id: row.get("sender_id"),
        tool_name: row.get("tool_name"),
        remaining_calls: row.get("remaining_calls"),
        created_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("created_at")),
        updated_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("updated_at")),
    }
}

fn map_tool_quota_sqlite_row(row: SqliteRow) -> QqChatAgentServiceToolQuota {
    QqChatAgentServiceToolQuota {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        sender_id: row.get("sender_id"),
        tool_name: row.get("tool_name"),
        remaining_calls: row.get("remaining_calls"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn mysql_pool(config: &Arc<MySqlConfig>) -> Result<&sqlx::mysql::MySqlPool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("tool-quota mysql pool is not initialized".to_string()))
}

fn sqlite_pool(config: &Arc<SqliteConfig>) -> Result<&sqlx::sqlite::SqlitePool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("tool-quota sqlite pool is not initialized".to_string()))
}

fn format_mysql_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}
