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
pub struct QqChatAgentServiceIgnoreRule {
    pub id: i64,
    pub agent_id: String,
    pub sender_id: Option<String>,
    pub group_id: Option<String>,
    pub match_key: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatAgentServiceIgnoreRuleUpsert {
    pub sender_id: Option<String>,
    pub group_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NormalizedIgnoreRuleInput {
    pub sender_id: Option<String>,
    pub group_id: Option<String>,
    pub match_key: String,
}

pub fn normalize_ignore_rule_input(
    sender_id: Option<&str>,
    group_id: Option<&str>,
) -> Result<NormalizedIgnoreRuleInput> {
    let sender_id = sender_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let group_id = group_id.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned);

    let match_key = match (&sender_id, &group_id) {
        (Some(sender_id), Some(group_id)) => {
            format!("group:{group_id}|sender:{sender_id}")
        }
        (Some(sender_id), None) => format!("sender:{sender_id}"),
        (None, Some(group_id)) => format!("group:{group_id}"),
        (None, None) => {
            return Err(Error::ValidationError(
                "ignore rule requires at least one of sender_id or group_id".to_string(),
            ))
        }
    };

    Ok(NormalizedIgnoreRuleInput { sender_id, group_id, match_key })
}

pub async fn list_ignore_rules(
    connection: &RelationalDbConnection,
    agent_id: &str,
) -> Result<Vec<QqChatAgentServiceIgnoreRule>> {
    match connection {
        RelationalDbConnection::MySql(config) => list_ignore_rules_mysql(config, agent_id).await,
        RelationalDbConnection::Sqlite(config) => list_ignore_rules_sqlite(config, agent_id).await,
    }
}

pub async fn create_ignore_rule(
    connection: &RelationalDbConnection,
    agent_id: &str,
    input: &QqChatAgentServiceIgnoreRuleUpsert,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let normalized = normalize_ignore_rule_input(input.sender_id.as_deref(), input.group_id.as_deref())?;
    match connection {
        RelationalDbConnection::MySql(config) => create_ignore_rule_mysql(config, agent_id, &normalized).await,
        RelationalDbConnection::Sqlite(config) => create_ignore_rule_sqlite(config, agent_id, &normalized).await,
    }
}

pub async fn update_ignore_rule(
    connection: &RelationalDbConnection,
    agent_id: &str,
    rule_id: i64,
    input: &QqChatAgentServiceIgnoreRuleUpsert,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let normalized = normalize_ignore_rule_input(input.sender_id.as_deref(), input.group_id.as_deref())?;
    match connection {
        RelationalDbConnection::MySql(config) => update_ignore_rule_mysql(config, agent_id, rule_id, &normalized).await,
        RelationalDbConnection::Sqlite(config) => {
            update_ignore_rule_sqlite(config, agent_id, rule_id, &normalized).await
        }
    }
}

pub async fn delete_ignore_rule(connection: &RelationalDbConnection, agent_id: &str, rule_id: i64) -> Result<()> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            sqlx::query("DELETE FROM qq_chat_agent_service_ignore_rule WHERE id = ? AND agent_id = ?")
                .bind(rule_id)
                .bind(agent_id)
                .execute(mysql_pool(config)?)
                .await
                .map_err(Error::Database)?;
        }
        RelationalDbConnection::Sqlite(config) => {
            sqlx::query("DELETE FROM qq_chat_agent_service_ignore_rule WHERE id = ? AND agent_id = ?")
                .bind(rule_id)
                .bind(agent_id)
                .execute(sqlite_pool(config)?)
                .await
                .map_err(Error::Database)?;
        }
    }

    Ok(())
}

pub async fn should_ignore_message(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    group_id: Option<&str>,
) -> Result<bool> {
    let sender_match_key = format!("sender:{}", sender_id.trim());
    let group_match_key = group_id.map(|value| format!("group:{}", value.trim()));
    let group_sender_match_key = group_id.map(|value| format!("group:{}|sender:{}", value.trim(), sender_id.trim()));

    match connection {
        RelationalDbConnection::MySql(config) => {
            let rows = sqlx::query(
                "SELECT 1 FROM qq_chat_agent_service_ignore_rule \
                 WHERE agent_id = ? AND (match_key = ? OR match_key = ? OR match_key = ?) LIMIT 1",
            )
            .bind(agent_id)
            .bind(&sender_match_key)
            .bind(group_match_key.as_deref())
            .bind(group_sender_match_key.as_deref())
            .fetch_all(mysql_pool(config)?)
            .await
            .map_err(Error::Database)?;

            Ok(!rows.is_empty())
        }
        RelationalDbConnection::Sqlite(config) => {
            let rows = sqlx::query(
                "SELECT 1 FROM qq_chat_agent_service_ignore_rule \
                 WHERE agent_id = ? AND (match_key = ? OR match_key = ? OR match_key = ?) LIMIT 1",
            )
            .bind(agent_id)
            .bind(&sender_match_key)
            .bind(group_match_key.as_deref())
            .bind(group_sender_match_key.as_deref())
            .fetch_all(sqlite_pool(config)?)
            .await
            .map_err(Error::Database)?;

            Ok(!rows.is_empty())
        }
    }
}

pub fn should_ignore_message_blocking(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    group_id: Option<&str>,
) -> Result<bool> {
    let connection = connection.clone();
    let agent_id = agent_id.to_string();
    let sender_id = sender_id.to_string();
    let group_id = group_id.map(ToOwned::to_owned);
    let run = async move { should_ignore_message(&connection, &agent_id, &sender_id, group_id.as_deref()).await };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}

async fn list_ignore_rules_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
) -> Result<Vec<QqChatAgentServiceIgnoreRule>> {
    let rows = sqlx::query(
        "SELECT id, agent_id, sender_id, group_id, match_key, created_at, updated_at \
         FROM qq_chat_agent_service_ignore_rule WHERE agent_id = ? ORDER BY id ASC",
    )
    .bind(agent_id)
    .fetch_all(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(rows.into_iter().map(map_ignore_rule_mysql_row).collect())
}

async fn list_ignore_rules_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
) -> Result<Vec<QqChatAgentServiceIgnoreRule>> {
    let rows = sqlx::query(
        "SELECT id, agent_id, sender_id, group_id, match_key, created_at, updated_at \
         FROM qq_chat_agent_service_ignore_rule WHERE agent_id = ? ORDER BY id ASC",
    )
    .bind(agent_id)
    .fetch_all(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(rows.into_iter().map(map_ignore_rule_sqlite_row).collect())
}

async fn create_ignore_rule_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    normalized: &NormalizedIgnoreRuleInput,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let now = Local::now().naive_local();

    sqlx::query(
        "INSERT INTO qq_chat_agent_service_ignore_rule \
         (agent_id, sender_id, group_id, match_key, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(agent_id)
    .bind(&normalized.sender_id)
    .bind(&normalized.group_id)
    .bind(&normalized.match_key)
    .bind(now)
    .bind(now)
    .execute(mysql_pool(config)?)
    .await
    .map_err(map_conflict_error)?;

    fetch_ignore_rule_by_match_key_mysql(config, agent_id, &normalized.match_key).await
}

async fn create_ignore_rule_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    normalized: &NormalizedIgnoreRuleInput,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        "INSERT INTO qq_chat_agent_service_ignore_rule \
         (agent_id, sender_id, group_id, match_key, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(agent_id)
    .bind(&normalized.sender_id)
    .bind(&normalized.group_id)
    .bind(&normalized.match_key)
    .bind(&now)
    .bind(&now)
    .execute(sqlite_pool(config)?)
    .await
    .map_err(map_conflict_error)?;

    fetch_ignore_rule_by_match_key_sqlite(config, agent_id, &normalized.match_key).await
}

async fn update_ignore_rule_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    rule_id: i64,
    normalized: &NormalizedIgnoreRuleInput,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let now = Local::now().naive_local();

    let result = sqlx::query(
        "UPDATE qq_chat_agent_service_ignore_rule \
         SET sender_id = ?, group_id = ?, match_key = ?, updated_at = ? \
         WHERE id = ? AND agent_id = ?",
    )
    .bind(&normalized.sender_id)
    .bind(&normalized.group_id)
    .bind(&normalized.match_key)
    .bind(now)
    .bind(rule_id)
    .bind(agent_id)
    .execute(mysql_pool(config)?)
    .await
    .map_err(map_conflict_error)?;

    if result.rows_affected() == 0 {
        return Err(Error::ValidationError(format!(
            "ignore rule '{}' not found for agent '{}'",
            rule_id, agent_id
        )));
    }

    fetch_ignore_rule_by_id_mysql(config, agent_id, rule_id).await
}

async fn update_ignore_rule_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    rule_id: i64,
    normalized: &NormalizedIgnoreRuleInput,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let result = sqlx::query(
        "UPDATE qq_chat_agent_service_ignore_rule \
         SET sender_id = ?, group_id = ?, match_key = ?, updated_at = ? \
         WHERE id = ? AND agent_id = ?",
    )
    .bind(&normalized.sender_id)
    .bind(&normalized.group_id)
    .bind(&normalized.match_key)
    .bind(&now)
    .bind(rule_id)
    .bind(agent_id)
    .execute(sqlite_pool(config)?)
    .await
    .map_err(map_conflict_error)?;

    if result.rows_affected() == 0 {
        return Err(Error::ValidationError(format!(
            "ignore rule '{}' not found for agent '{}'",
            rule_id, agent_id
        )));
    }

    fetch_ignore_rule_by_id_sqlite(config, agent_id, rule_id).await
}

async fn fetch_ignore_rule_by_id_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    rule_id: i64,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let row = sqlx::query(
        "SELECT id, agent_id, sender_id, group_id, match_key, created_at, updated_at \
         FROM qq_chat_agent_service_ignore_rule WHERE id = ? AND agent_id = ? LIMIT 1",
    )
    .bind(rule_id)
    .bind(agent_id)
    .fetch_one(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(map_ignore_rule_mysql_row(row))
}

async fn fetch_ignore_rule_by_id_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    rule_id: i64,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let row = sqlx::query(
        "SELECT id, agent_id, sender_id, group_id, match_key, created_at, updated_at \
         FROM qq_chat_agent_service_ignore_rule WHERE id = ? AND agent_id = ? LIMIT 1",
    )
    .bind(rule_id)
    .bind(agent_id)
    .fetch_one(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(map_ignore_rule_sqlite_row(row))
}

async fn fetch_ignore_rule_by_match_key_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    match_key: &str,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let row = sqlx::query(
        "SELECT id, agent_id, sender_id, group_id, match_key, created_at, updated_at \
         FROM qq_chat_agent_service_ignore_rule WHERE agent_id = ? AND match_key = ? LIMIT 1",
    )
    .bind(agent_id)
    .bind(match_key)
    .fetch_one(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(map_ignore_rule_mysql_row(row))
}

async fn fetch_ignore_rule_by_match_key_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    match_key: &str,
) -> Result<QqChatAgentServiceIgnoreRule> {
    let row = sqlx::query(
        "SELECT id, agent_id, sender_id, group_id, match_key, created_at, updated_at \
         FROM qq_chat_agent_service_ignore_rule WHERE agent_id = ? AND match_key = ? LIMIT 1",
    )
    .bind(agent_id)
    .bind(match_key)
    .fetch_one(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;

    Ok(map_ignore_rule_sqlite_row(row))
}

fn map_ignore_rule_mysql_row(row: MySqlRow) -> QqChatAgentServiceIgnoreRule {
    QqChatAgentServiceIgnoreRule {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        sender_id: row.get("sender_id"),
        group_id: row.get("group_id"),
        match_key: row.get("match_key"),
        created_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("created_at")),
        updated_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("updated_at")),
    }
}

fn map_ignore_rule_sqlite_row(row: SqliteRow) -> QqChatAgentServiceIgnoreRule {
    QqChatAgentServiceIgnoreRule {
        id: row.get("id"),
        agent_id: row.get("agent_id"),
        sender_id: row.get("sender_id"),
        group_id: row.get("group_id"),
        match_key: row.get("match_key"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn mysql_pool(config: &Arc<MySqlConfig>) -> Result<&sqlx::mysql::MySqlPool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("ignore-rule mysql pool is not initialized".to_string()))
}

fn sqlite_pool(config: &Arc<SqliteConfig>) -> Result<&sqlx::sqlite::SqlitePool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("ignore-rule sqlite pool is not initialized".to_string()))
}

fn format_mysql_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn map_conflict_error(error: sqlx::Error) -> Error {
    let message = error.to_string().to_lowercase();
    if message.contains("unique") || message.contains("duplicate") || message.contains("constraint failed") {
        return Error::ValidationError("ignore rule already exists".to_string());
    }
    Error::Database(error)
}
