//! Durable archive of QQ Chat conversation turns.
//!
//! The live context (memory/Redis) only holds the current window and disappears on restart.
//! Archiving each finished turn into the agent's relational database keeps the conversation
//! browsable across restarts.

use chrono::{DateTime, Local, NaiveDateTime};
use sqlx::Row;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::Result;
use zihuan_core::qq_context::{MediaRef, QqChatQuotedMessage, QqChatTurnEntry, QqChatTurnRecord};

const LOG_PREFIX: &str = "[qq_context_store]";

#[derive(Debug, Clone)]
pub struct ArchivedTurn {
    pub turn_id: String,
    pub created_at: String,
    pub user_text: String,
    pub quoted: Option<QqChatQuotedMessage>,
    pub media: Vec<MediaRef>,
    pub entries: Vec<QqChatTurnEntry>,
    pub assistant_text: Option<String>,
    pub reasoning_text: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub cached_prompt_tokens: Option<i64>,
    pub prompt_cache_miss_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub generation_ms: Option<i64>,
    pub output_tokens_per_second: Option<f64>,
    pub continues_previous: bool,
    pub finished: bool,
}

/// Upsert one finished turn. Failures are logged and swallowed: archiving must never break a
/// conversation turn.
pub fn persist_turn(connection: &RelationalDbConnection, record: &QqChatTurnRecord) {
    if let Err(error) = persist_turn_inner(connection, record) {
        log::warn!("{LOG_PREFIX} failed to archive turn {}: {}", record.turn_id, error);
    }
}

fn persist_turn_inner(
    connection: &RelationalDbConnection,
    record: &QqChatTurnRecord,
) -> Result<()> {
    let media_json = serde_json::to_string(&record.media).ok();
    let entries_json = serde_json::to_string(&record.entries).ok();
    let quoted_json = record.quoted.as_ref().and_then(|quoted| serde_json::to_string(quoted).ok());
    let created_at = DateTime::parse_from_rfc3339(&record.created_at)
        .map(|value| value.naive_local().to_string())
        .unwrap_or_else(|_| Local::now().naive_local().to_string());
    let usage = record.usage.as_ref();
    let prompt_tokens = usage.and_then(|usage| usage.prompt_tokens).map(|value| value as i64);
    let cached_prompt_tokens =
        usage.and_then(|usage| usage.cached_prompt_tokens).map(|value| value as i64);
    let prompt_cache_miss_tokens =
        usage.and_then(|usage| usage.prompt_cache_miss_tokens).map(|value| value as i64);
    let completion_tokens =
        usage.and_then(|usage| usage.completion_tokens).map(|value| value as i64);
    let total_tokens = usage.and_then(|usage| usage.total_tokens).map(|value| value as i64);
    let generation_ms = record.generation_ms.map(|value| value as i64);

    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config.pool.as_ref().ok_or_else(|| {
                zihuan_core::string_error!("context turn archive mysql pool is not initialized")
            })?;
            let query = sqlx::query(
                "INSERT INTO qq_chat_context_turn (turn_id, agent_id, sender_id, sender_name, group_name, is_group, created_at, user_text, quoted_json, media_json, entries_json, assistant_text, reasoning_text, prompt_tokens, cached_prompt_tokens, prompt_cache_miss_tokens, completion_tokens, total_tokens, generation_ms, output_tokens_per_second, continues_previous, finished) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
                 ON DUPLICATE KEY UPDATE user_text = VALUES(user_text), quoted_json = VALUES(quoted_json), media_json = VALUES(media_json), entries_json = VALUES(entries_json), assistant_text = VALUES(assistant_text), reasoning_text = VALUES(reasoning_text), prompt_tokens = VALUES(prompt_tokens), cached_prompt_tokens = VALUES(cached_prompt_tokens), prompt_cache_miss_tokens = VALUES(prompt_cache_miss_tokens), completion_tokens = VALUES(completion_tokens), total_tokens = VALUES(total_tokens), generation_ms = VALUES(generation_ms), output_tokens_per_second = VALUES(output_tokens_per_second), continues_previous = VALUES(continues_previous), finished = VALUES(finished)",
            )
            .bind(&record.turn_id)
            .bind(&record.agent_id)
            .bind(&record.sender_id)
            .bind(&record.sender_name)
            .bind(&record.group_name)
            .bind(record.is_group as i64)
            .bind(&created_at)
            .bind(&record.user_text)
            .bind(&quoted_json)
            .bind(&media_json)
            .bind(&entries_json)
            .bind(&record.assistant_text)
            .bind(&record.reasoning_text)
            .bind(prompt_tokens)
            .bind(cached_prompt_tokens)
            .bind(prompt_cache_miss_tokens)
            .bind(completion_tokens)
            .bind(total_tokens)
            .bind(generation_ms)
            .bind(record.output_tokens_per_second)
            .bind(record.continues_previous as i64)
            .bind(record.finished as i64);
            zihuan_core::runtime::block_async(async move { query.execute(pool).await })
                .map_err(zihuan_core::error::Error::Database)?;
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config.pool.as_ref().ok_or_else(|| {
                zihuan_core::string_error!("context turn archive sqlite pool is not initialized")
            })?;
            let query = sqlx::query(
                "INSERT OR REPLACE INTO qq_chat_context_turn (turn_id, agent_id, sender_id, sender_name, group_name, is_group, created_at, user_text, quoted_json, media_json, entries_json, assistant_text, reasoning_text, prompt_tokens, cached_prompt_tokens, prompt_cache_miss_tokens, completion_tokens, total_tokens, generation_ms, output_tokens_per_second, continues_previous, finished) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&record.turn_id)
            .bind(&record.agent_id)
            .bind(&record.sender_id)
            .bind(&record.sender_name)
            .bind(&record.group_name)
            .bind(record.is_group as i64)
            .bind(&created_at)
            .bind(&record.user_text)
            .bind(&quoted_json)
            .bind(&media_json)
            .bind(&entries_json)
            .bind(&record.assistant_text)
            .bind(&record.reasoning_text)
            .bind(prompt_tokens)
            .bind(cached_prompt_tokens)
            .bind(prompt_cache_miss_tokens)
            .bind(completion_tokens)
            .bind(total_tokens)
            .bind(generation_ms)
            .bind(record.output_tokens_per_second)
            .bind(record.continues_previous as i64)
            .bind(record.finished as i64);
            zihuan_core::runtime::block_async(async move { query.execute(pool).await })
                .map_err(zihuan_core::error::Error::Database)?;
        }
    }

    Ok(())
}

const SELECT_SQL: &str = "SELECT turn_id, created_at, user_text, quoted_json, media_json, entries_json, assistant_text, reasoning_text, prompt_tokens, cached_prompt_tokens, prompt_cache_miss_tokens, completion_tokens, total_tokens, generation_ms, output_tokens_per_second, continues_previous, finished FROM qq_chat_context_turn WHERE agent_id = ? AND sender_id = ? ORDER BY created_at ASC LIMIT ?";

/// Archived turns for one sender, oldest first.
pub fn load_turns(
    connection: &RelationalDbConnection,
    agent_id: &str,
    sender_id: &str,
    limit: i64,
) -> Result<Vec<ArchivedTurn>> {
    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config.pool.as_ref().ok_or_else(|| {
                zihuan_core::string_error!("context turn archive mysql pool is not initialized")
            })?;
            let rows = zihuan_core::runtime::block_async(async {
                sqlx::query(SELECT_SQL)
                    .bind(agent_id)
                    .bind(sender_id)
                    .bind(limit)
                    .fetch_all(pool)
                    .await
            })
            .map_err(zihuan_core::error::Error::Database)?;
            Ok(rows
                .into_iter()
                .map(|row| ArchivedTurn {
                    turn_id: row.try_get("turn_id").unwrap_or_default(),
                    created_at: row
                        .try_get::<NaiveDateTime, _>("created_at")
                        .map(|value| value.format("%Y-%m-%d %H:%M:%S").to_string())
                        .unwrap_or_default(),
                    user_text: row.try_get("user_text").unwrap_or_default(),
                    quoted: parse_quoted(row.try_get("quoted_json").unwrap_or(None)),
                    media: parse_media(row.try_get("media_json").unwrap_or(None)),
                    entries: parse_entries(row.try_get("entries_json").unwrap_or(None)),
                    assistant_text: row.try_get("assistant_text").unwrap_or(None),
                    reasoning_text: row.try_get("reasoning_text").unwrap_or(None),
                    prompt_tokens: row.try_get("prompt_tokens").unwrap_or(None),
                    cached_prompt_tokens: row.try_get("cached_prompt_tokens").unwrap_or(None),
                    prompt_cache_miss_tokens: row
                        .try_get("prompt_cache_miss_tokens")
                        .unwrap_or(None),
                    completion_tokens: row.try_get("completion_tokens").unwrap_or(None),
                    total_tokens: row.try_get("total_tokens").unwrap_or(None),
                    generation_ms: row.try_get("generation_ms").unwrap_or(None),
                    output_tokens_per_second: row
                        .try_get("output_tokens_per_second")
                        .unwrap_or(None),
                    continues_previous: row.try_get::<i64, _>("continues_previous").unwrap_or(0)
                        != 0,
                    finished: row.try_get::<i64, _>("finished").unwrap_or(0) != 0,
                })
                .collect())
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config.pool.as_ref().ok_or_else(|| {
                zihuan_core::string_error!("context turn archive sqlite pool is not initialized")
            })?;
            let rows = zihuan_core::runtime::block_async(async {
                sqlx::query(SELECT_SQL)
                    .bind(agent_id)
                    .bind(sender_id)
                    .bind(limit)
                    .fetch_all(pool)
                    .await
            })
            .map_err(zihuan_core::error::Error::Database)?;
            Ok(rows
                .into_iter()
                .map(|row| ArchivedTurn {
                    turn_id: row.try_get("turn_id").unwrap_or_default(),
                    created_at: row.try_get("created_at").unwrap_or_default(),
                    user_text: row.try_get("user_text").unwrap_or_default(),
                    quoted: parse_quoted(row.try_get("quoted_json").unwrap_or(None)),
                    media: parse_media(row.try_get("media_json").unwrap_or(None)),
                    entries: parse_entries(row.try_get("entries_json").unwrap_or(None)),
                    assistant_text: row.try_get("assistant_text").unwrap_or(None),
                    reasoning_text: row.try_get("reasoning_text").unwrap_or(None),
                    prompt_tokens: row.try_get("prompt_tokens").unwrap_or(None),
                    cached_prompt_tokens: row.try_get("cached_prompt_tokens").unwrap_or(None),
                    prompt_cache_miss_tokens: row
                        .try_get("prompt_cache_miss_tokens")
                        .unwrap_or(None),
                    completion_tokens: row.try_get("completion_tokens").unwrap_or(None),
                    total_tokens: row.try_get("total_tokens").unwrap_or(None),
                    generation_ms: row.try_get("generation_ms").unwrap_or(None),
                    output_tokens_per_second: row
                        .try_get("output_tokens_per_second")
                        .unwrap_or(None),
                    continues_previous: row.try_get::<i64, _>("continues_previous").unwrap_or(0)
                        != 0,
                    finished: row.try_get::<i64, _>("finished").unwrap_or(0) != 0,
                })
                .collect())
        }
    }
}

fn parse_quoted(raw: Option<String>) -> Option<QqChatQuotedMessage> {
    raw.and_then(|raw| serde_json::from_str(&raw).ok())
}

fn parse_entries(raw: Option<String>) -> Vec<QqChatTurnEntry> {
    raw.and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

fn parse_media(raw: Option<String>) -> Vec<MediaRef> {
    raw.and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}
