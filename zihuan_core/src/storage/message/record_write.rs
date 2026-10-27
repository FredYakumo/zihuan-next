use std::sync::Arc;

use log::{info, warn};
use redis::AsyncCommands;
use tokio::task::block_in_place;

use crate::data_refs::{MySqlConfig, RelationalDbConnection, SqliteConfig};
use crate::error::{Error, Result};
use crate::ims_bot_adapter::models::event_model::MessageEvent;
use crate::ims_bot_adapter::models::message::{collect_media_records, Message};
use crate::storage::message::chunking::{
    split_content_chunks, truncate_field_if_needed, truncate_optional_field_if_needed,
    AT_TARGET_LIST_MAX_CHARS, CONTENT_MAX_CHARS, GROUP_ID_MAX_CHARS, GROUP_NAME_MAX_CHARS,
    MEDIA_JSON_MAX_CHARS, MESSAGE_ID_MAX_CHARS, SENDER_ID_MAX_CHARS, SENDER_NAME_MAX_CHARS,
};
use crate::storage::message::record_restore::{
    cache_message_snapshot, register_rdb_pool, register_redis_ref, CachedMessageSnapshotPayload,
};
use crate::storage::RedisConfig;

const LOG_PREFIX: &str = "[message_record]";

/// Everything one `message_record` row grows out of, independent of where the caller got it.
///
/// The adapter path builds this from a live `MessageEvent`; the script path builds it from the
/// message list and identifiers a script already has. Both then go through the same write.
pub struct MessageRecordInput<'a> {
    pub message_id: &'a str,
    pub sender_id: &'a str,
    pub sender_name: &'a str,
    pub group_id: Option<&'a str>,
    pub group_name: Option<&'a str>,
    pub messages: &'a [Message],
    /// Serialized message list kept for lossless restore; `None` when the caller has no raw form.
    pub raw_message_json: Option<String>,
}

fn render_content(messages: &[Message]) -> String {
    crate::ims_bot_adapter::models::message::render_messages_readable(messages)
}

fn is_connection_error(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_))
}

/// Row fields shared by every content chunk; chunk-scoped columns are applied to the first only.
struct MessageRecordRow {
    message_id: String,
    sender_id: String,
    sender_name: String,
    send_time: String,
    group_id: Option<String>,
    group_name: Option<String>,
    at_target_list: Option<String>,
    media_json: Option<String>,
    raw_message_json: Option<String>,
    content_chunks: Vec<String>,
}

fn prepare_message_record(input: &MessageRecordInput<'_>) -> Result<MessageRecordRow> {
    let message_id = truncate_field_if_needed(
        "message_id",
        input.message_id.to_string(),
        MESSAGE_ID_MAX_CHARS,
        input.message_id,
    );
    let sender_id = truncate_field_if_needed(
        "sender_id",
        input.sender_id.to_string(),
        SENDER_ID_MAX_CHARS,
        &message_id,
    );
    let sender_name = truncate_field_if_needed(
        "sender_name",
        input.sender_name.to_string(),
        SENDER_NAME_MAX_CHARS,
        &message_id,
    );
    let group_id = truncate_optional_field_if_needed(
        "group_id",
        input.group_id.map(ToOwned::to_owned),
        GROUP_ID_MAX_CHARS,
        &message_id,
    );
    let group_name = truncate_optional_field_if_needed(
        "group_name",
        input.group_name.map(ToOwned::to_owned),
        GROUP_NAME_MAX_CHARS,
        &message_id,
    );

    let content = render_content(input.messages);
    let at_targets: Vec<String> = input
        .messages
        .iter()
        .filter_map(|message| match message {
            Message::At(at) => Some(at.target_id()),
            _ => None,
        })
        .collect();
    let at_target_list = truncate_optional_field_if_needed(
        "at_target_list",
        (!at_targets.is_empty()).then(|| at_targets.join(",")),
        AT_TARGET_LIST_MAX_CHARS,
        &message_id,
    );
    let media_json = {
        let records = collect_media_records(input.messages);
        if records.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&records)?)
        }
    };
    let media_json = truncate_optional_field_if_needed(
        "media_json",
        media_json,
        MEDIA_JSON_MAX_CHARS,
        &message_id,
    );

    Ok(MessageRecordRow {
        send_time: chrono::Local::now().naive_local().to_string(),
        content_chunks: split_content_chunks(&content, CONTENT_MAX_CHARS),
        message_id,
        sender_id,
        sender_name,
        group_id,
        group_name,
        at_target_list,
        media_json,
        raw_message_json: input.raw_message_json.clone(),
    })
}

const INSERT_MESSAGE_RECORD_SQL: &str = r#"
    INSERT INTO message_record
    (message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json, raw_message_json)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    "#;

async fn insert_message_record_mysql(
    pool: &sqlx::mysql::MySqlPool,
    row: &MessageRecordRow,
) -> std::result::Result<(), sqlx::Error> {
    for (chunk_index, content_chunk) in row.content_chunks.iter().enumerate() {
        let first = chunk_index == 0;
        sqlx::query(INSERT_MESSAGE_RECORD_SQL)
            .bind(&row.message_id)
            .bind(&row.sender_id)
            .bind(&row.sender_name)
            .bind(&row.send_time)
            .bind(&row.group_id)
            .bind(&row.group_name)
            .bind(content_chunk)
            .bind(first.then_some(row.at_target_list.as_ref()).flatten())
            .bind(first.then_some(row.media_json.as_ref()).flatten())
            .bind(first.then_some(row.raw_message_json.as_ref()).flatten())
            .execute(pool)
            .await?;
    }
    Ok(())
}

async fn insert_message_record_sqlite(
    pool: &sqlx::sqlite::SqlitePool,
    row: &MessageRecordRow,
) -> std::result::Result<(), sqlx::Error> {
    for (chunk_index, content_chunk) in row.content_chunks.iter().enumerate() {
        let first = chunk_index == 0;
        sqlx::query(INSERT_MESSAGE_RECORD_SQL)
            .bind(&row.message_id)
            .bind(&row.sender_id)
            .bind(&row.sender_name)
            .bind(&row.send_time)
            .bind(&row.group_id)
            .bind(&row.group_name)
            .bind(content_chunk)
            .bind(first.then_some(row.at_target_list.as_ref()).flatten())
            .bind(first.then_some(row.media_json.as_ref()).flatten())
            .bind(first.then_some(row.raw_message_json.as_ref()).flatten())
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// Write one message's chunks, retrying once on a lost MySQL connection.
///
/// Reports whether the row reached the database. Callers decide what a failure means: the
/// adapter path only logs it (archiving must never fail the turn), while the script path returns
/// the flag so the script sees `false`. SQLite has no reconnect semantics, so it is tried once.
fn write_message_record(
    row: &MessageRecordRow,
    connection: &RelationalDbConnection,
) -> Result<bool> {
    info!(
        "{LOG_PREFIX} writing message {} (sender={}, group={:?}, chunks={})",
        row.message_id,
        row.sender_id,
        row.group_id,
        row.content_chunks.len()
    );

    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = mysql_pool(config)?.clone();
            for attempt in 1..=2 {
                let run = insert_message_record_mysql(&pool, row);
                let result = if let Some(handle) = config.runtime_handle.clone() {
                    if tokio::runtime::Handle::try_current().is_ok() {
                        block_in_place(|| handle.block_on(run))
                    } else {
                        handle.block_on(run)
                    }
                } else if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    block_in_place(|| handle.block_on(run))
                } else {
                    tokio::runtime::Runtime::new()?.block_on(run)
                };

                match result {
                    Ok(()) => return Ok(true),
                    Err(error) if attempt < 2 && is_connection_error(&error) => {
                        warn!("{LOG_PREFIX} retrying message {}: {error}", row.message_id);
                    }
                    Err(error) => {
                        warn!(
                            "{LOG_PREFIX} relational DB write failed for message {}: {error}",
                            row.message_id
                        );
                        return Ok(false);
                    }
                }
            }
            Ok(false)
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = sqlite_pool(config)?.clone();
            let run = insert_message_record_sqlite(&pool, row);
            let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(run))
            } else {
                tokio::runtime::Runtime::new()?.block_on(run)
            };
            match result {
                Ok(()) => Ok(true),
                Err(error) => {
                    warn!(
                        "{LOG_PREFIX} relational DB write failed for message {}: {error}",
                        row.message_id
                    );
                    Ok(false)
                }
            }
        }
    }
}

fn write_media_records(messages: &[Message], connection: &RelationalDbConnection) -> Result<()> {
    let media_records = collect_media_records(messages);
    if media_records.is_empty() {
        return Ok(());
    }

    let persist_result = match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = mysql_pool(config)?.clone();
            let run = async move {
                for record in &media_records {
                    sqlx::query(
                        r#"
                        INSERT INTO media_record (media_id, source, original_source, rustfs_path, name, description, mime_type)
                        VALUES (?, ?, ?, ?, ?, ?, ?)
                        ON DUPLICATE KEY UPDATE
                            source = VALUES(source),
                            original_source = VALUES(original_source),
                            rustfs_path = VALUES(rustfs_path),
                            name = VALUES(name),
                            description = VALUES(description),
                            mime_type = VALUES(mime_type)
                        "#,
                    )
                    .bind(&record.media_id)
                    .bind(record.source.to_string())
                    .bind(&record.original_source)
                    .bind(&record.rustfs_path)
                    .bind(&record.name)
                    .bind(&record.description)
                    .bind(&record.mime_type)
                    .execute(&pool)
                    .await?;
                }
                Ok::<(), sqlx::Error>(())
            };
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(run))
            } else {
                tokio::runtime::Runtime::new()?.block_on(run)
            }
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = sqlite_pool(config)?.clone();
            let run = async move {
                for record in &media_records {
                    sqlx::query(
                        r#"
                        INSERT OR REPLACE INTO media_record (media_id, source, original_source, rustfs_path, name, description, mime_type)
                        VALUES (?, ?, ?, ?, ?, ?, ?)
                        "#,
                    )
                    .bind(&record.media_id)
                    .bind(record.source.to_string())
                    .bind(&record.original_source)
                    .bind(&record.rustfs_path)
                    .bind(&record.name)
                    .bind(&record.description)
                    .bind(&record.mime_type)
                    .execute(&pool)
                    .await?;
                }
                Ok::<(), sqlx::Error>(())
            };
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(run))
            } else {
                tokio::runtime::Runtime::new()?.block_on(run)
            }
        }
    };

    if let Err(error) = persist_result {
        warn!("{LOG_PREFIX} media_record write failed: {error}");
    }
    Ok(())
}

/// Persist one message to `message_record` plus its media rows.
///
/// Returns whether the `message_record` write succeeded; media rows are best-effort either way.
fn persist_message_record(
    input: &MessageRecordInput<'_>,
    connection: &RelationalDbConnection,
) -> Result<bool> {
    let row = prepare_message_record(input)?;
    let written = write_message_record(&row, connection)?;
    write_media_records(input.messages, connection)?;
    Ok(written)
}

fn persist_message_to_redis(
    message_id: &str,
    payload: &CachedMessageSnapshotPayload,
    redis_ref: &Arc<RedisConfig>,
) -> Result<()> {
    let Some(url) = redis_ref.url.clone() else {
        return Ok(());
    };

    let redis_ref = Arc::clone(redis_ref);
    let message_id = message_id.to_string();
    let payload = serde_json::to_string(payload)?;

    let run = async move {
        let mut cm_guard = redis_ref.redis_cm.lock().await;
        let mut url_guard = redis_ref.cached_redis_url.lock().await;

        if url_guard.as_deref() != Some(url.as_str()) {
            *cm_guard = None;
            *url_guard = Some(url.clone());
        }

        if cm_guard.is_none() {
            let client = redis::Client::open(url.as_str())?;
            *cm_guard = Some(client.get_tokio_connection().await?);
        }

        if let Some(cm) = cm_guard.as_mut() {
            let _: () = cm.set(&message_id, &payload).await?;
        }

        Ok::<(), crate::error::Error>(())
    };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}

/// Archive one inbound or outbound message event.
pub fn persist_message_event(
    event: &MessageEvent,
    rdb_pool: Option<&RelationalDbConnection>,
    redis_ref: Option<&Arc<RedisConfig>>,
) -> Result<()> {
    cache_message_snapshot(event);

    let message_id = event.message_id.to_string();
    let media_json = {
        let records = collect_media_records(&event.message_list);
        if records.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&records)?)
        }
    };
    let raw_message_json = Some(serde_json::to_string(&event.message_list)?);
    let redis_payload = CachedMessageSnapshotPayload {
        message_id: message_id.clone(),
        content: render_content(&event.message_list),
        media_json,
        raw_message_json: raw_message_json.clone(),
    };

    if let Some(redis_ref) = redis_ref.cloned() {
        register_redis_ref(redis_ref.clone());
        if let Err(error) = persist_message_to_redis(&message_id, &redis_payload, &redis_ref) {
            warn!("{LOG_PREFIX} Redis cache write failed for message {}: {}", message_id, error);
        }
    }

    let Some(rdb_pool) = rdb_pool.cloned() else {
        return Ok(());
    };
    register_rdb_pool(rdb_pool.clone());

    let sender_name = if event.sender.card.is_empty() {
        event.sender.nickname.clone()
    } else {
        event.sender.card.clone()
    };
    let sender_id = event.sender.user_id.to_string();
    let group_id = event.group_id.map(|id| id.to_string());
    let input = MessageRecordInput {
        message_id: &message_id,
        sender_id: &sender_id,
        sender_name: &sender_name,
        group_id: group_id.as_deref(),
        group_name: event.group_name.as_deref(),
        messages: &event.message_list,
        raw_message_json,
    };
    persist_message_record(&input, &rdb_pool).map(|_| ())
}

/// Archive a message list a script assembled, from identifiers the script supplies.
///
/// Returns whether the row reached the relational database; a missing pool or a failed insert
/// reports `false` instead of failing the script.
pub fn persist_qq_message_list(
    rdb_config: &Arc<MySqlConfig>,
    messages: &[Message],
    message_id: String,
    sender_id: String,
    sender_name: String,
    group_id: Option<String>,
    group_name: Option<String>,
) -> Result<bool> {
    if rdb_config.pool.is_none() {
        warn!("{LOG_PREFIX} rdb pool has no active pool");
        return Ok(false);
    }

    let raw_message_json = Some(serde_json::to_string(messages)?);
    let input = MessageRecordInput {
        message_id: &message_id,
        sender_id: &sender_id,
        sender_name: &sender_name,
        group_id: group_id.as_deref(),
        group_name: group_name.as_deref(),
        messages,
        raw_message_json,
    };
    let connection = RelationalDbConnection::MySql(Arc::clone(rdb_config));
    persist_message_record(&input, &connection)
}

fn mysql_pool(config: &Arc<MySqlConfig>) -> Result<&sqlx::mysql::MySqlPool> {
    config.pool.as_ref().ok_or_else(|| {
        Error::ValidationError("message persistence mysql pool is not initialized".to_string())
    })
}

fn sqlite_pool(config: &Arc<SqliteConfig>) -> Result<&sqlx::sqlite::SqlitePool> {
    config.pool.as_ref().ok_or_else(|| {
        Error::ValidationError("message persistence sqlite pool is not initialized".to_string())
    })
}
