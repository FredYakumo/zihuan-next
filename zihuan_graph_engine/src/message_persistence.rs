use crate::data_value::RedisConfig;
use crate::message_mysql_chunking::{
    split_content_chunks, truncate_field_if_needed, truncate_optional_field_if_needed,
    AT_TARGET_LIST_MAX_CHARS, CONTENT_MAX_CHARS, GROUP_ID_MAX_CHARS, GROUP_NAME_MAX_CHARS,
    MEDIA_JSON_MAX_CHARS, MESSAGE_ID_MAX_CHARS, SENDER_ID_MAX_CHARS, SENDER_NAME_MAX_CHARS,
};
use crate::message_restore::{cache_message_snapshot, register_redis_ref, CachedMessageSnapshotPayload};
use log::{info, warn};
use once_cell::sync::Lazy;
use redis::AsyncCommands;
use std::sync::{Arc, RwLock};
use tokio::task::block_in_place;
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::Result;
use zihuan_core::ims_bot_adapter::models::event_model::MessageEvent;
use zihuan_core::ims_bot_adapter::models::message::{collect_media_records, Message};

static LATEST_MYSQL_REF: Lazy<RwLock<Option<Arc<MySqlConfig>>>> = Lazy::new(|| RwLock::new(None));
static LATEST_REDIS_REF: Lazy<RwLock<Option<Arc<RedisConfig>>>> = Lazy::new(|| RwLock::new(None));

pub fn register_mysql_persistence_ref(config: Arc<MySqlConfig>) {
    if let Ok(mut guard) = LATEST_MYSQL_REF.write() {
        *guard = Some(config);
    }
}

pub fn register_redis_persistence_ref(config: Arc<RedisConfig>) {
    if let Ok(mut guard) = LATEST_REDIS_REF.write() {
        *guard = Some(config.clone());
    }
    register_redis_ref(config);
}

fn latest_mysql_ref() -> Option<Arc<MySqlConfig>> {
    LATEST_MYSQL_REF.read().ok().and_then(|guard| guard.clone())
}

fn latest_redis_ref() -> Option<Arc<RedisConfig>> {
    LATEST_REDIS_REF.read().ok().and_then(|guard| guard.clone())
}

fn is_connection_error(e: &sqlx::Error) -> bool {
    matches!(
        e,
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_)
    )
}

fn render_content(messages: &[Message]) -> String {
    zihuan_core::ims_bot_adapter::models::message::render_messages_readable(messages)
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

        Ok::<(), zihuan_core::error::Error>(())
    };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}

fn persist_message_to_mysql(event: &MessageEvent, mysql_ref: &Arc<MySqlConfig>) -> Result<()> {
    let pool = match mysql_ref.pool.clone() {
        Some(pool) => pool,
        None => {
            warn!("[message_persistence] mysql_ref has no active pool, skipping MySQL persistence");
            return Ok(());
        }
    };

    let raw_message_id = event.message_id.to_string();
    let message_id = truncate_field_if_needed(
        "message_id",
        raw_message_id.clone(),
        MESSAGE_ID_MAX_CHARS,
        &raw_message_id,
    );
    let sender_id = truncate_field_if_needed(
        "sender_id",
        event.sender.user_id.to_string(),
        SENDER_ID_MAX_CHARS,
        &message_id,
    );
    let sender_name = if event.sender.card.is_empty() {
        event.sender.nickname.clone()
    } else {
        event.sender.card.clone()
    };
    let sender_name = truncate_field_if_needed(
        "sender_name",
        sender_name,
        SENDER_NAME_MAX_CHARS,
        &message_id,
    );
    let send_time = chrono::Local::now().naive_local();
    let group_id = truncate_optional_field_if_needed(
        "group_id",
        event.group_id.map(|id| id.to_string()),
        GROUP_ID_MAX_CHARS,
        &message_id,
    );
    let group_name = truncate_optional_field_if_needed(
        "group_name",
        event.group_name.clone(),
        GROUP_NAME_MAX_CHARS,
        &message_id,
    );
    let content = render_content(&event.message_list);
    let at_targets: Vec<String> = event
        .message_list
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
        let records = collect_media_records(&event.message_list);
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
    let raw_message_json = Some(serde_json::to_string(&event.message_list)?);
    let content_chunks = split_content_chunks(&content, CONTENT_MAX_CHARS);

    info!(
        "[message_persistence] Persisting message {} (sender={}, group={:?}, chunks={}) to MySQL",
        message_id,
        sender_id,
        group_id,
        content_chunks.len()
    );

    for attempt in 1u32..=2 {
        let message_id_for_bind = message_id.clone();
        let sender_id_for_bind = sender_id.clone();
        let sender_name_for_bind = sender_name.clone();
        let group_id_for_bind = group_id.clone();
        let group_name_for_bind = group_name.clone();
        let at_target_list_for_bind = at_target_list.clone();
        let media_json_for_bind = media_json.clone();
        let raw_message_json_for_bind = raw_message_json.clone();
        let content_chunks_for_bind = content_chunks.clone();

        let run = async {
            for (chunk_index, content_chunk) in content_chunks_for_bind.iter().enumerate() {
                let chunk_at_target_list = if chunk_index == 0 {
                    at_target_list_for_bind.as_ref()
                } else {
                    None
                };
                let chunk_media_json = if chunk_index == 0 {
                    media_json_for_bind.as_ref()
                } else {
                    None
                };
                let chunk_raw_message_json = if chunk_index == 0 {
                    raw_message_json_for_bind.as_ref()
                } else {
                    None
                };

                sqlx::query(
                    r#"
                    INSERT INTO message_record
                    (message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json, raw_message_json)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&message_id_for_bind)
                .bind(&sender_id_for_bind)
                .bind(&sender_name_for_bind)
                .bind(send_time)
                .bind(&group_id_for_bind)
                .bind(&group_name_for_bind)
                .bind(content_chunk)
                .bind(chunk_at_target_list)
                .bind(chunk_media_json)
                .bind(chunk_raw_message_json)
                .execute(&pool)
                .await?;
            }

            Ok::<(), sqlx::Error>(())
        };

        let result = if let Some(handle) = mysql_ref.runtime_handle.clone() {
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
            Ok(()) => return Ok(()),
            Err(ref error) if attempt < 2 && is_connection_error(error) => {
                warn!(
                    "[message_persistence] MySQL persist attempt {} failed with connection error ({}), retrying",
                    attempt, error
                );
            }
            Err(error) => {
                warn!(
                    "[message_persistence] MySQL persist failed for message {} (attempt {}): {}",
                    message_id, attempt, error
                );
                return Ok(());
            }
        }
    }

    Ok(())
}

pub fn persist_message_event(
    event: &MessageEvent,
    mysql_ref: Option<&Arc<MySqlConfig>>,
    redis_ref: Option<&Arc<RedisConfig>>,
) -> Result<()> {
    cache_message_snapshot(event);

    let message_id = event.message_id.to_string();
    let content = render_content(&event.message_list);
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
        content: content.clone(),
        media_json,
        raw_message_json,
    };

    if let Some(redis_ref) = redis_ref.cloned().or_else(latest_redis_ref) {
        register_redis_ref(redis_ref.clone());
        if let Err(error) = persist_message_to_redis(&message_id, &redis_payload, &redis_ref) {
            warn!(
                "[message_persistence] Redis cache write failed for message {}: {}",
                message_id, error
            );
        }
    }

    if let Some(mysql_ref) = mysql_ref.cloned().or_else(latest_mysql_ref) {
        persist_message_to_mysql(event, &mysql_ref)?;
    }

    Ok(())
}
