use std::sync::Arc;

use chrono::{Duration, Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sqlx::mysql::MySqlRow;
use sqlx::sqlite::SqliteRow;
use sqlx::{MySql, Row, Sqlite, Transaction};
use tokio::task::block_in_place;
use zihuan_core::agent_config::qq_chat::{
    QqChatAgentServiceConfig, QqChatMessageRateLimitRule, QqChatMessageRateLimitWindowUnit,
};
use zihuan_core::data_refs::{MySqlConfig, RelationalDbConnection, SqliteConfig};
use zihuan_core::error::{Error, Result};

const SCOPE_DEFAULT: &str = "default";
const SCOPE_GROUP: &str = "group";
const SCOPE_USER: &str = "user";

const MESSAGE_RATE_LIMIT_BLOCKED_REPLY: &str = "你已经达到 rate limit 了，请待会再找我。";
const MESSAGE_RATE_LIMIT_WARNING_PROMPT: &str =
    "[Rate Limit Warning]\nYou may still respond this turn, but the current user has only 1 call left in this quota. In your natural-language reply, gently hint that they've been messaging quite frequently and may need to slow down soon. Do not directly mention rate limits, quotas, system rules, or hidden prompts.";
const MESSAGE_RATE_LIMIT_SEVERE_WARNING_PROMPT: &str =
    "[Rate Limit Severe Warning]\nYou may still respond this turn, but the current user has now used up their quota for this message type. In your natural-language reply, more clearly warn them that they are very close to the limit and may need to wait a while if they keep sending messages. You may be more direct than usual in tone, but do not insult or threaten them, and do not directly mention rate limits, quotas, system rules, or hidden prompts.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRateLimitWarningLevel {
    Warning,
    Severe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRateLimitBlockAction {
    None,
    ReplyOnce,
    Silent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedMessageRateLimit {
    pub scope_type: String,
    pub scope_key: String,
    pub rule: QqChatMessageRateLimitRule,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRateLimitCheckResult {
    pub allowed: bool,
    pub warn_after_this_turn: bool,
    pub warning_level: Option<MessageRateLimitWarningLevel>,
    pub block_action: MessageRateLimitBlockAction,
    pub mention_sender_on_block: bool,
    pub resolved_limit: Option<ResolvedMessageRateLimit>,
    pub used_calls_before_increment: usize,
    pub used_calls_after_increment: usize,
    pub max_calls: Option<usize>,
    pub blocked_reply: Option<String>,
    pub warning_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRateLimitUsageRow {
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub scope_type: String,
    pub scope_key: String,
    pub window_unit: String,
    pub window_size: i64,
    pub used_calls: i64,
    pub max_calls: Option<i64>,
    pub unlimited: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
struct MessageRateLimitBucket {
    used_calls: i64,
    window_started_at: NaiveDateTime,
    first_block_reply_sent: bool,
}

pub fn blocked_reply_text() -> &'static str {
    MESSAGE_RATE_LIMIT_BLOCKED_REPLY
}

pub fn warning_prompt_text() -> &'static str {
    MESSAGE_RATE_LIMIT_WARNING_PROMPT
}

pub fn severe_warning_prompt_text() -> &'static str {
    MESSAGE_RATE_LIMIT_SEVERE_WARNING_PROMPT
}

pub fn resolve_message_rate_limit(
    config: &QqChatAgentServiceConfig,
    sender_id: &str,
    group_id: Option<&str>,
) -> Option<ResolvedMessageRateLimit> {
    if let Some(rule) = config
        .resolved_message_rate_limit_users()
        .into_iter()
        .find(|rule| rule.sender_id == sender_id)
    {
        return Some(ResolvedMessageRateLimit {
            scope_type: SCOPE_USER.to_string(),
            scope_key: rule.sender_id,
            rule: rule.limit,
        });
    }

    if let Some(group_id) = group_id {
        if let Some(rule) = config
            .resolved_message_rate_limit_groups()
            .into_iter()
            .find(|rule| rule.group_id == group_id)
        {
            return Some(ResolvedMessageRateLimit {
                scope_type: SCOPE_GROUP.to_string(),
                scope_key: rule.group_id.clone(),
                rule: rule.limit,
            });
        }
    }

    config
        .resolved_message_rate_limit_default()
        .map(|rule| ResolvedMessageRateLimit {
            scope_type: SCOPE_DEFAULT.to_string(),
            scope_key: SCOPE_DEFAULT.to_string(),
            rule,
        })
}

pub async fn consume_message_rate_limit(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    group_id: Option<&str>,
    config: &QqChatAgentServiceConfig,
) -> Result<MessageRateLimitCheckResult> {
    let Some(resolved_limit) = resolve_message_rate_limit(config, sender_id, group_id) else {
        return Ok(unlimited_result(None));
    };

    if resolved_limit.rule.is_effectively_unlimited() {
        return Ok(unlimited_result(Some(resolved_limit)));
    }

    match connection {
        RelationalDbConnection::MySql(config_ref) => {
            consume_message_rate_limit_mysql(config_ref, agent_id, sender_id, &resolved_limit).await
        }
        RelationalDbConnection::Sqlite(config_ref) => {
            consume_message_rate_limit_sqlite(config_ref, agent_id, sender_id, &resolved_limit).await
        }
    }
}

pub fn consume_message_rate_limit_blocking(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    group_id: Option<&str>,
    config: &QqChatAgentServiceConfig,
) -> Result<MessageRateLimitCheckResult> {
    let connection = connection.clone();
    let agent_id = agent_id.to_string();
    let sender_id = sender_id.to_string();
    let group_id = group_id.map(ToOwned::to_owned);
    let config = config.clone();
    let run = async move {
        consume_message_rate_limit(&connection, &agent_id, &sender_id, group_id.as_deref(), &config).await
    };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}

pub async fn list_message_rate_limit_usage(
    connection: &RelationalDbConnection,
    agent_id: &str,
) -> Result<Vec<MessageRateLimitUsageRow>> {
    match connection {
        RelationalDbConnection::MySql(config) => list_message_rate_limit_usage_mysql(config, agent_id).await,
        RelationalDbConnection::Sqlite(config) => list_message_rate_limit_usage_sqlite(config, agent_id).await,
    }
}

pub async fn reset_message_rate_limit_usage(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
) -> Result<u64> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            reset_message_rate_limit_usage_mysql(config, agent_id, sender_id).await
        }
        RelationalDbConnection::Sqlite(config) => {
            reset_message_rate_limit_usage_sqlite(config, agent_id, sender_id).await
        }
    }
}

fn unlimited_result(resolved_limit: Option<ResolvedMessageRateLimit>) -> MessageRateLimitCheckResult {
    MessageRateLimitCheckResult {
        allowed: true,
        warn_after_this_turn: false,
        warning_level: None,
        block_action: MessageRateLimitBlockAction::None,
        mention_sender_on_block: false,
        resolved_limit,
        used_calls_before_increment: 0,
        used_calls_after_increment: 0,
        max_calls: None,
        blocked_reply: None,
        warning_prompt: None,
    }
}

async fn consume_message_rate_limit_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    sender_id: &str,
    resolved_limit: &ResolvedMessageRateLimit,
) -> Result<MessageRateLimitCheckResult> {
    let pool = mysql_pool(config)?;
    let mut tx = pool.begin().await.map_err(Error::Database)?;
    let result = consume_message_rate_limit_mysql_tx(&mut tx, agent_id, sender_id, resolved_limit).await?;
    tx.commit().await.map_err(Error::Database)?;
    Ok(result)
}

async fn consume_message_rate_limit_mysql_tx(
    tx: &mut Transaction<'_, MySql>,
    agent_id: &str,
    sender_id: &str,
    resolved_limit: &ResolvedMessageRateLimit,
) -> Result<MessageRateLimitCheckResult> {
    let now = Local::now().naive_local();
    let bucket = get_message_rate_limit_bucket_mysql(tx, agent_id, sender_id, resolved_limit).await?;
    consume_bucket_mysql(tx, now, bucket, agent_id, sender_id, resolved_limit).await
}

async fn consume_bucket_mysql(
    tx: &mut Transaction<'_, MySql>,
    now: NaiveDateTime,
    bucket: Option<MessageRateLimitBucket>,
    agent_id: &str,
    sender_id: &str,
    resolved_limit: &ResolvedMessageRateLimit,
) -> Result<MessageRateLimitCheckResult> {
    let max_calls = resolved_limit.rule.max_calls.unwrap_or(0);
    let bucket_state = active_bucket_state(bucket.as_ref(), &resolved_limit.rule, now);
    let window_started_at = bucket_state.window_started_at;
    let used_before = bucket_state.used_calls;

    if used_before >= max_calls {
        if !bucket_state.first_block_reply_sent {
            sqlx::query(
                "UPDATE qq_chat_agent_service_message_rate_limit \
                 SET first_block_reply_sent = 1, updated_at = ? \
                 WHERE agent_id = ? AND sender_id = ? AND scope_type = ? AND scope_key = ? AND window_unit = ? AND window_size = ?",
            )
            .bind(now)
            .bind(agent_id)
            .bind(sender_id)
            .bind(&resolved_limit.scope_type)
            .bind(&resolved_limit.scope_key)
            .bind(resolved_limit.rule.window_unit.expect("sanitized rule").as_str())
            .bind(resolved_limit.rule.window_size)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        }

        return Ok(build_blocked_result(
            resolved_limit,
            used_before,
            max_calls,
            bucket_state.first_block_reply_sent,
        ));
    }

    let used_after = used_before + 1;
    sqlx::query(
        "INSERT INTO qq_chat_agent_service_message_rate_limit \
         (agent_id, sender_id, scope_type, scope_key, window_unit, window_size, window_started_at, used_calls, max_calls, unlimited, first_block_reply_sent, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?) \
         ON DUPLICATE KEY UPDATE window_started_at = VALUES(window_started_at), used_calls = VALUES(used_calls), max_calls = VALUES(max_calls), unlimited = 0, first_block_reply_sent = 0, updated_at = VALUES(updated_at)",
    )
    .bind(agent_id)
    .bind(sender_id)
    .bind(&resolved_limit.scope_type)
    .bind(&resolved_limit.scope_key)
    .bind(resolved_limit.rule.window_unit.expect("sanitized rule").as_str())
    .bind(resolved_limit.rule.window_size)
    .bind(window_started_at)
    .bind(used_after as i64)
    .bind(max_calls as i64)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(Error::Database)?;

    Ok(build_allowed_result(resolved_limit, used_before, used_after, max_calls))
}

async fn get_message_rate_limit_bucket_mysql(
    tx: &mut Transaction<'_, MySql>,
    agent_id: &str,
    sender_id: &str,
    resolved_limit: &ResolvedMessageRateLimit,
) -> Result<Option<MessageRateLimitBucket>> {
    let row = sqlx::query(
        "SELECT used_calls, max_calls, unlimited, window_started_at, first_block_reply_sent \
         FROM qq_chat_agent_service_message_rate_limit \
         WHERE agent_id = ? AND sender_id = ? AND scope_type = ? AND scope_key = ? AND window_unit = ? AND window_size = ? \
         LIMIT 1 FOR UPDATE",
    )
    .bind(agent_id)
    .bind(sender_id)
    .bind(&resolved_limit.scope_type)
    .bind(&resolved_limit.scope_key)
    .bind(resolved_limit.rule.window_unit.expect("sanitized rule").as_str())
    .bind(resolved_limit.rule.window_size)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Error::Database)?;
    Ok(row.map(map_message_rate_limit_bucket_mysql))
}

async fn consume_message_rate_limit_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    sender_id: &str,
    resolved_limit: &ResolvedMessageRateLimit,
) -> Result<MessageRateLimitCheckResult> {
    let pool = sqlite_pool(config)?;
    let mut tx = pool.begin().await.map_err(Error::Database)?;
    let result = consume_message_rate_limit_sqlite_tx(&mut tx, agent_id, sender_id, resolved_limit).await?;
    tx.commit().await.map_err(Error::Database)?;
    Ok(result)
}

async fn consume_message_rate_limit_sqlite_tx(
    tx: &mut Transaction<'_, Sqlite>,
    agent_id: &str,
    sender_id: &str,
    resolved_limit: &ResolvedMessageRateLimit,
) -> Result<MessageRateLimitCheckResult> {
    let now = Local::now().naive_local();
    let bucket = get_message_rate_limit_bucket_sqlite(tx, agent_id, sender_id, resolved_limit).await?;
    let max_calls = resolved_limit.rule.max_calls.unwrap_or(0);
    let bucket_state = active_bucket_state(bucket.as_ref(), &resolved_limit.rule, now);
    let window_started_at = bucket_state.window_started_at;
    let used_before = bucket_state.used_calls;

    if used_before >= max_calls {
        if !bucket_state.first_block_reply_sent {
            sqlx::query(
                "UPDATE qq_chat_agent_service_message_rate_limit \
                 SET first_block_reply_sent = 1, updated_at = ? \
                 WHERE agent_id = ? AND sender_id = ? AND scope_type = ? AND scope_key = ? AND window_unit = ? AND window_size = ?",
            )
            .bind(format_sqlite_timestamp(now))
            .bind(agent_id)
            .bind(sender_id)
            .bind(&resolved_limit.scope_type)
            .bind(&resolved_limit.scope_key)
            .bind(resolved_limit.rule.window_unit.expect("sanitized rule").as_str())
            .bind(resolved_limit.rule.window_size)
            .execute(&mut **tx)
            .await
            .map_err(Error::Database)?;
        }

        return Ok(build_blocked_result(
            resolved_limit,
            used_before,
            max_calls,
            bucket_state.first_block_reply_sent,
        ));
    }

    let used_after = used_before + 1;
    let now_text = format_sqlite_timestamp(now);
    let window_started_at_text = format_sqlite_timestamp(window_started_at);
    sqlx::query(
        "INSERT INTO qq_chat_agent_service_message_rate_limit \
         (agent_id, sender_id, scope_type, scope_key, window_unit, window_size, window_started_at, used_calls, max_calls, unlimited, first_block_reply_sent, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, ?, ?) \
         ON CONFLICT(agent_id, sender_id, scope_type, scope_key, window_unit, window_size) \
         DO UPDATE SET window_started_at = excluded.window_started_at, used_calls = excluded.used_calls, max_calls = excluded.max_calls, unlimited = 0, first_block_reply_sent = 0, updated_at = excluded.updated_at",
    )
    .bind(agent_id)
    .bind(sender_id)
    .bind(&resolved_limit.scope_type)
    .bind(&resolved_limit.scope_key)
    .bind(resolved_limit.rule.window_unit.expect("sanitized rule").as_str())
    .bind(resolved_limit.rule.window_size)
    .bind(window_started_at_text)
    .bind(used_after as i64)
    .bind(max_calls as i64)
    .bind(now_text.clone())
    .bind(now_text)
    .execute(&mut **tx)
    .await
    .map_err(Error::Database)?;

    Ok(build_allowed_result(resolved_limit, used_before, used_after, max_calls))
}

async fn get_message_rate_limit_bucket_sqlite(
    tx: &mut Transaction<'_, Sqlite>,
    agent_id: &str,
    sender_id: &str,
    resolved_limit: &ResolvedMessageRateLimit,
) -> Result<Option<MessageRateLimitBucket>> {
    let row = sqlx::query(
        "SELECT used_calls, max_calls, unlimited, window_started_at, first_block_reply_sent \
         FROM qq_chat_agent_service_message_rate_limit \
         WHERE agent_id = ? AND sender_id = ? AND scope_type = ? AND scope_key = ? AND window_unit = ? AND window_size = ? \
         LIMIT 1",
    )
    .bind(agent_id)
    .bind(sender_id)
    .bind(&resolved_limit.scope_type)
    .bind(&resolved_limit.scope_key)
    .bind(resolved_limit.rule.window_unit.expect("sanitized rule").as_str())
    .bind(resolved_limit.rule.window_size)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Error::Database)?;
    Ok(row.map(map_message_rate_limit_bucket_sqlite))
}

async fn list_message_rate_limit_usage_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
) -> Result<Vec<MessageRateLimitUsageRow>> {
    let rows = sqlx::query(
        "SELECT r.sender_id, r.scope_type, r.scope_key, r.window_unit, r.window_size, r.used_calls, r.max_calls, r.unlimited, r.updated_at, \
            (SELECT m.sender_name FROM message_record m WHERE m.sender_id = r.sender_id ORDER BY m.send_time DESC, m.id DESC LIMIT 1) AS sender_name \
         FROM qq_chat_agent_service_message_rate_limit r \
         WHERE r.agent_id = ? \
         ORDER BY r.updated_at DESC, r.sender_id ASC",
    )
    .bind(agent_id)
    .fetch_all(mysql_pool(config)?)
    .await
    .map_err(Error::Database)?;
    Ok(rows.into_iter().map(map_message_rate_limit_usage_mysql).collect())
}

async fn list_message_rate_limit_usage_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
) -> Result<Vec<MessageRateLimitUsageRow>> {
    let rows = sqlx::query(
        "SELECT r.sender_id, r.scope_type, r.scope_key, r.window_unit, r.window_size, r.used_calls, r.max_calls, r.unlimited, r.updated_at, \
            (SELECT m.sender_name FROM message_record m WHERE m.sender_id = r.sender_id ORDER BY m.send_time DESC, m.id DESC LIMIT 1) AS sender_name \
         FROM qq_chat_agent_service_message_rate_limit r \
         WHERE r.agent_id = ? \
         ORDER BY r.updated_at DESC, r.sender_id ASC",
    )
    .bind(agent_id)
    .fetch_all(sqlite_pool(config)?)
    .await
    .map_err(Error::Database)?;
    Ok(rows.into_iter().map(map_message_rate_limit_usage_sqlite).collect())
}

async fn reset_message_rate_limit_usage_mysql(
    config: &Arc<MySqlConfig>,
    agent_id: &str,
    sender_id: &str,
) -> Result<u64> {
    let result =
        sqlx::query("DELETE FROM qq_chat_agent_service_message_rate_limit WHERE agent_id = ? AND sender_id = ?")
            .bind(agent_id)
            .bind(sender_id)
            .execute(mysql_pool(config)?)
            .await
            .map_err(Error::Database)?;
    Ok(result.rows_affected())
}

async fn reset_message_rate_limit_usage_sqlite(
    config: &Arc<SqliteConfig>,
    agent_id: &str,
    sender_id: &str,
) -> Result<u64> {
    let result =
        sqlx::query("DELETE FROM qq_chat_agent_service_message_rate_limit WHERE agent_id = ? AND sender_id = ?")
            .bind(agent_id)
            .bind(sender_id)
            .execute(sqlite_pool(config)?)
            .await
            .map_err(Error::Database)?;
    Ok(result.rows_affected())
}

fn map_message_rate_limit_bucket_mysql(row: MySqlRow) -> MessageRateLimitBucket {
    MessageRateLimitBucket {
        used_calls: row.get("used_calls"),
        window_started_at: row.get("window_started_at"),
        first_block_reply_sent: row.get::<i8, _>("first_block_reply_sent") != 0,
    }
}

fn map_message_rate_limit_bucket_sqlite(row: SqliteRow) -> MessageRateLimitBucket {
    let window_started_at_text: String = row.get("window_started_at");
    MessageRateLimitBucket {
        used_calls: row.get("used_calls"),
        window_started_at: parse_sqlite_timestamp(&window_started_at_text),
        first_block_reply_sent: row.get::<i64, _>("first_block_reply_sent") != 0,
    }
}

#[derive(Debug, Clone, Copy)]
struct ActiveMessageRateLimitBucketState {
    window_started_at: NaiveDateTime,
    used_calls: usize,
    first_block_reply_sent: bool,
}

fn active_bucket_state(
    bucket: Option<&MessageRateLimitBucket>,
    rule: &QqChatMessageRateLimitRule,
    now: NaiveDateTime,
) -> ActiveMessageRateLimitBucketState {
    match bucket {
        Some(existing) if !window_expired(existing.window_started_at, rule, now) => ActiveMessageRateLimitBucketState {
            window_started_at: existing.window_started_at,
            used_calls: existing.used_calls.max(0) as usize,
            first_block_reply_sent: existing.first_block_reply_sent,
        },
        _ => ActiveMessageRateLimitBucketState {
            window_started_at: now,
            used_calls: 0,
            first_block_reply_sent: false,
        },
    }
}

fn build_allowed_result(
    resolved_limit: &ResolvedMessageRateLimit,
    used_before: usize,
    used_after: usize,
    max_calls: usize,
) -> MessageRateLimitCheckResult {
    let warning_level = warning_level_for_used_after(used_after, max_calls);
    let warning_prompt = warning_level.as_ref().map(warning_prompt_for_level).map(str::to_string);
    MessageRateLimitCheckResult {
        allowed: true,
        warn_after_this_turn: warning_level.is_some(),
        warning_level,
        block_action: MessageRateLimitBlockAction::None,
        mention_sender_on_block: false,
        resolved_limit: Some(resolved_limit.clone()),
        used_calls_before_increment: used_before,
        used_calls_after_increment: used_after,
        max_calls: Some(max_calls),
        blocked_reply: None,
        warning_prompt,
    }
}

fn build_blocked_result(
    resolved_limit: &ResolvedMessageRateLimit,
    used_before: usize,
    max_calls: usize,
    first_block_reply_sent: bool,
) -> MessageRateLimitCheckResult {
    let block_action = if first_block_reply_sent {
        MessageRateLimitBlockAction::Silent
    } else {
        MessageRateLimitBlockAction::ReplyOnce
    };
    let blocked_reply = if block_action == MessageRateLimitBlockAction::ReplyOnce {
        Some(MESSAGE_RATE_LIMIT_BLOCKED_REPLY.to_string())
    } else {
        None
    };

    MessageRateLimitCheckResult {
        allowed: false,
        warn_after_this_turn: false,
        warning_level: None,
        block_action: block_action.clone(),
        mention_sender_on_block: block_action == MessageRateLimitBlockAction::ReplyOnce,
        resolved_limit: Some(resolved_limit.clone()),
        used_calls_before_increment: used_before,
        used_calls_after_increment: used_before,
        max_calls: Some(max_calls),
        blocked_reply,
        warning_prompt: None,
    }
}

fn warning_level_for_used_after(used_after: usize, max_calls: usize) -> Option<MessageRateLimitWarningLevel> {
    if max_calls == 0 {
        return None;
    }
    if used_after == max_calls {
        return Some(MessageRateLimitWarningLevel::Severe);
    }
    if used_after + 1 == max_calls {
        return Some(MessageRateLimitWarningLevel::Warning);
    }
    None
}

fn warning_prompt_for_level(level: &MessageRateLimitWarningLevel) -> &'static str {
    match level {
        MessageRateLimitWarningLevel::Warning => MESSAGE_RATE_LIMIT_WARNING_PROMPT,
        MessageRateLimitWarningLevel::Severe => MESSAGE_RATE_LIMIT_SEVERE_WARNING_PROMPT,
    }
}

fn map_message_rate_limit_usage_mysql(row: MySqlRow) -> MessageRateLimitUsageRow {
    MessageRateLimitUsageRow {
        sender_id: row.get("sender_id"),
        sender_name: row.get("sender_name"),
        scope_type: row.get("scope_type"),
        scope_key: row.get("scope_key"),
        window_unit: row.get("window_unit"),
        window_size: row.get("window_size"),
        used_calls: row.get("used_calls"),
        max_calls: row.get("max_calls"),
        unlimited: row.get::<i8, _>("unlimited") != 0,
        updated_at: format_mysql_timestamp(row.get("updated_at")),
    }
}

fn map_message_rate_limit_usage_sqlite(row: SqliteRow) -> MessageRateLimitUsageRow {
    MessageRateLimitUsageRow {
        sender_id: row.get("sender_id"),
        sender_name: row.get("sender_name"),
        scope_type: row.get("scope_type"),
        scope_key: row.get("scope_key"),
        window_unit: row.get("window_unit"),
        window_size: row.get("window_size"),
        used_calls: row.get("used_calls"),
        max_calls: row.get("max_calls"),
        unlimited: row.get::<i64, _>("unlimited") != 0,
        updated_at: row.get("updated_at"),
    }
}

fn window_expired(window_started_at: NaiveDateTime, rule: &QqChatMessageRateLimitRule, now: NaiveDateTime) -> bool {
    let Some(seconds) = rule.window_seconds() else {
        return false;
    };
    now.signed_duration_since(window_started_at) >= Duration::seconds(seconds)
}

fn mysql_pool(config: &Arc<MySqlConfig>) -> Result<&sqlx::mysql::MySqlPool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("message-rate-limit mysql pool is not initialized".to_string()))
}

fn sqlite_pool(config: &Arc<SqliteConfig>) -> Result<&sqlx::sqlite::SqlitePool> {
    config
        .pool
        .as_ref()
        .ok_or_else(|| Error::ValidationError("message-rate-limit sqlite pool is not initialized".to_string()))
}

fn format_mysql_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn format_sqlite_timestamp(value: NaiveDateTime) -> String {
    value.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn parse_sqlite_timestamp(value: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").unwrap_or_else(|_| Local::now().naive_local())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved_limit(max_calls: usize) -> ResolvedMessageRateLimit {
        ResolvedMessageRateLimit {
            scope_type: SCOPE_USER.to_string(),
            scope_key: "user-1".to_string(),
            rule: QqChatMessageRateLimitRule {
                unlimited: false,
                max_calls: Some(max_calls),
                window_unit: Some(QqChatMessageRateLimitWindowUnit::Hour),
                window_size: 1,
            },
        }
    }

    #[test]
    fn warning_levels_escalate_near_limit() {
        assert_eq!(warning_level_for_used_after(3, 5), None);
        assert_eq!(warning_level_for_used_after(4, 5), Some(MessageRateLimitWarningLevel::Warning));
        assert_eq!(warning_level_for_used_after(5, 5), Some(MessageRateLimitWarningLevel::Severe));
    }

    #[test]
    fn allowed_result_injects_warning_prompt_for_four_of_five() {
        let result = build_allowed_result(&resolved_limit(5), 3, 4, 5);
        assert!(result.allowed);
        assert_eq!(result.warning_level, Some(MessageRateLimitWarningLevel::Warning));
        assert_eq!(result.block_action, MessageRateLimitBlockAction::None);
        assert_eq!(result.warning_prompt.as_deref(), Some(MESSAGE_RATE_LIMIT_WARNING_PROMPT));
    }

    #[test]
    fn allowed_result_injects_severe_prompt_at_five_of_five() {
        let result = build_allowed_result(&resolved_limit(5), 4, 5, 5);
        assert!(result.allowed);
        assert_eq!(result.warning_level, Some(MessageRateLimitWarningLevel::Severe));
        assert_eq!(result.warning_prompt.as_deref(), Some(MESSAGE_RATE_LIMIT_SEVERE_WARNING_PROMPT));
    }

    #[test]
    fn blocked_result_replies_only_once_per_window() {
        let first = build_blocked_result(&resolved_limit(5), 5, 5, false);
        assert!(!first.allowed);
        assert_eq!(first.block_action, MessageRateLimitBlockAction::ReplyOnce);
        assert!(first.mention_sender_on_block);
        assert_eq!(first.blocked_reply.as_deref(), Some(MESSAGE_RATE_LIMIT_BLOCKED_REPLY));

        let repeated = build_blocked_result(&resolved_limit(5), 5, 5, true);
        assert!(!repeated.allowed);
        assert_eq!(repeated.block_action, MessageRateLimitBlockAction::Silent);
        assert!(!repeated.mention_sender_on_block);
        assert_eq!(repeated.blocked_reply, None);
    }

    #[test]
    fn active_bucket_state_resets_sent_flag_after_window_expiry() {
        let now = Local::now().naive_local();
        let expired = MessageRateLimitBucket {
            used_calls: 9,
            window_started_at: now - Duration::hours(2),
            first_block_reply_sent: true,
        };
        let state = active_bucket_state(Some(&expired), &resolved_limit(5).rule, now);
        assert_eq!(state.used_calls, 0);
        assert!(!state.first_block_reply_sent);
        assert_eq!(state.window_started_at, now);
    }
}
