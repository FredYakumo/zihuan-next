use std::sync::Arc;

use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sqlx::mysql::MySqlRow;
use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use tokio::task::block_in_place;
use zihuan_core::data_refs::{MySqlConfig, RelationalDbConnection, SqliteConfig};
use zihuan_core::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LanguageStyleScope {
    Global,
    Group { group_id: String },
}

impl LanguageStyleScope {
    fn scope_type(&self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Group { .. } => "group",
        }
    }

    fn group_id(&self) -> Option<&str> {
        match self {
            Self::Global => None,
            Self::Group { group_id } => Some(group_id.as_str()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatAgentServiceLanguageStyle {
    pub id: i64,
    pub scope_type: String,
    pub group_id: Option<String>,
    pub style_prompt: String,
    pub sample_count: i32,
    pub learned_by_sender_id: String,
    pub learned_at: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn upsert_language_style(
    connection: &RelationalDbConnection,
    scope: &LanguageStyleScope,
    style_prompt: &str,
    sample_count: i32,
    learned_by_sender_id: &str,
) -> Result<QqChatAgentServiceLanguageStyle> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            upsert_language_style_mysql(config, scope, style_prompt, sample_count, learned_by_sender_id).await
        }
        RelationalDbConnection::Sqlite(config) => {
            upsert_language_style_sqlite(config, scope, style_prompt, sample_count, learned_by_sender_id).await
        }
    }
}

pub async fn get_applicable_language_style(
    connection: &RelationalDbConnection,
    group_id: Option<&str>,
) -> Result<Option<QqChatAgentServiceLanguageStyle>> {
    match connection {
        RelationalDbConnection::MySql(config) => get_applicable_language_style_mysql(config, group_id).await,
        RelationalDbConnection::Sqlite(config) => get_applicable_language_style_sqlite(config, group_id).await,
    }
}

pub fn get_applicable_language_style_blocking(
    connection: &RelationalDbConnection,
    group_id: Option<&str>,
) -> Result<Option<QqChatAgentServiceLanguageStyle>> {
    let connection = connection.clone();
    let group_id = group_id.map(ToOwned::to_owned);
    let run = async move { get_applicable_language_style(&connection, group_id.as_deref()).await };
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}

async fn upsert_language_style_mysql(
    config: &Arc<MySqlConfig>,
    scope: &LanguageStyleScope,
    style_prompt: &str,
    sample_count: i32,
    learned_by_sender_id: &str,
) -> Result<QqChatAgentServiceLanguageStyle> {
    let now = Local::now().naive_local();
    sqlx::query(
        "INSERT INTO qq_chat_agent_service_language_style \
         (scope_type, group_id, style_prompt, sample_count, learned_by_sender_id, learned_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE \
         style_prompt = VALUES(style_prompt), sample_count = VALUES(sample_count), learned_by_sender_id = VALUES(learned_by_sender_id), learned_at = VALUES(learned_at), updated_at = VALUES(updated_at)",
    )
    .bind(scope.scope_type())
    .bind(scope.group_id())
    .bind(style_prompt)
    .bind(sample_count)
    .bind(learned_by_sender_id)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;
    fetch_scope_mysql(config, scope).await
}

async fn upsert_language_style_sqlite(
    config: &Arc<SqliteConfig>,
    scope: &LanguageStyleScope,
    style_prompt: &str,
    sample_count: i32,
    learned_by_sender_id: &str,
) -> Result<QqChatAgentServiceLanguageStyle> {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query(
        "INSERT INTO qq_chat_agent_service_language_style \
         (scope_type, group_id, style_prompt, sample_count, learned_by_sender_id, learned_at, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(scope_type, group_id) DO UPDATE SET \
         style_prompt = excluded.style_prompt, sample_count = excluded.sample_count, learned_by_sender_id = excluded.learned_by_sender_id, learned_at = excluded.learned_at, updated_at = excluded.updated_at",
    )
    .bind(scope.scope_type())
    .bind(scope.group_id())
    .bind(style_prompt)
    .bind(sample_count)
    .bind(learned_by_sender_id)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;
    fetch_scope_sqlite(config, scope).await
}

async fn get_applicable_language_style_mysql(
    config: &Arc<MySqlConfig>,
    group_id: Option<&str>,
) -> Result<Option<QqChatAgentServiceLanguageStyle>> {
    if let Some(group_id) = group_id {
        if let Some(record) =
            fetch_scope_optional_mysql(config, &LanguageStyleScope::Group { group_id: group_id.to_string() }).await?
        {
            return Ok(Some(record));
        }
    }
    fetch_scope_optional_mysql(config, &LanguageStyleScope::Global).await
}

async fn get_applicable_language_style_sqlite(
    config: &Arc<SqliteConfig>,
    group_id: Option<&str>,
) -> Result<Option<QqChatAgentServiceLanguageStyle>> {
    if let Some(group_id) = group_id {
        if let Some(record) =
            fetch_scope_optional_sqlite(config, &LanguageStyleScope::Group { group_id: group_id.to_string() }).await?
        {
            return Ok(Some(record));
        }
    }
    fetch_scope_optional_sqlite(config, &LanguageStyleScope::Global).await
}

async fn fetch_scope_mysql(
    config: &Arc<MySqlConfig>,
    scope: &LanguageStyleScope,
) -> Result<QqChatAgentServiceLanguageStyle> {
    fetch_scope_optional_mysql(config, scope)
        .await?
        .ok_or_else(|| Error::ValidationError("language-style record missing after upsert".to_string()))
}

async fn fetch_scope_sqlite(
    config: &Arc<SqliteConfig>,
    scope: &LanguageStyleScope,
) -> Result<QqChatAgentServiceLanguageStyle> {
    fetch_scope_optional_sqlite(config, scope)
        .await?
        .ok_or_else(|| Error::ValidationError("language-style record missing after upsert".to_string()))
}

async fn fetch_scope_optional_mysql(
    config: &Arc<MySqlConfig>,
    scope: &LanguageStyleScope,
) -> Result<Option<QqChatAgentServiceLanguageStyle>> {
    let row = sqlx::query(
        "SELECT id, scope_type, group_id, style_prompt, sample_count, learned_by_sender_id, learned_at, created_at, updated_at \
         FROM qq_chat_agent_service_language_style WHERE scope_type = ? AND ((group_id IS NULL AND ? IS NULL) OR group_id = ?) LIMIT 1",
    )
    .bind(scope.scope_type())
    .bind(scope.group_id())
    .bind(scope.group_id())
    .fetch_optional(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;
    Ok(row.map(map_language_style_mysql_row))
}

async fn fetch_scope_optional_sqlite(
    config: &Arc<SqliteConfig>,
    scope: &LanguageStyleScope,
) -> Result<Option<QqChatAgentServiceLanguageStyle>> {
    let row = sqlx::query(
        "SELECT id, scope_type, group_id, style_prompt, sample_count, learned_by_sender_id, learned_at, created_at, updated_at \
         FROM qq_chat_agent_service_language_style WHERE scope_type = ? AND ((group_id IS NULL AND ? IS NULL) OR group_id = ?) LIMIT 1",
    )
    .bind(scope.scope_type())
    .bind(scope.group_id())
    .bind(scope.group_id())
    .fetch_optional(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;
    Ok(row.map(map_language_style_sqlite_row))
}

fn map_language_style_mysql_row(row: MySqlRow) -> QqChatAgentServiceLanguageStyle {
    QqChatAgentServiceLanguageStyle {
        id: row.get("id"),
        scope_type: row.get("scope_type"),
        group_id: row.get("group_id"),
        style_prompt: row.get("style_prompt"),
        sample_count: row.get("sample_count"),
        learned_by_sender_id: row.get("learned_by_sender_id"),
        learned_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("learned_at")),
        created_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("created_at")),
        updated_at: format_mysql_timestamp(row.get::<NaiveDateTime, _>("updated_at")),
    }
}

fn map_language_style_sqlite_row(row: SqliteRow) -> QqChatAgentServiceLanguageStyle {
    QqChatAgentServiceLanguageStyle {
        id: row.get("id"),
        scope_type: row.get("scope_type"),
        group_id: row.get("group_id"),
        style_prompt: row.get("style_prompt"),
        sample_count: row.get("sample_count"),
        learned_by_sender_id: row.get("learned_by_sender_id"),
        learned_at: row.get("learned_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn mysql_pool(config: &Arc<MySqlConfig>) -> Result<&sqlx::mysql::MySqlPool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("language-style mysql pool is not initialized".to_string()))
}

fn sqlite_pool(config: &Arc<SqliteConfig>) -> Result<&sqlx::sqlite::SqlitePool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("language-style sqlite pool is not initialized".to_string()))
}

fn format_mysql_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}
