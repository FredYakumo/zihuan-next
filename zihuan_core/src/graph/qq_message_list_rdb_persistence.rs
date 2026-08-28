use std::sync::Arc;

use chrono::Local;
use log::{debug, error, info, warn};
use tokio::task::block_in_place;

use crate::data_refs::MySqlConfig;
use crate::error::Result;
use crate::graph::message_rdb_chunking::{
    split_content_chunks, truncate_field_if_needed, truncate_optional_field_if_needed,
    AT_TARGET_LIST_MAX_CHARS, CONTENT_MAX_CHARS, GROUP_ID_MAX_CHARS, GROUP_NAME_MAX_CHARS,
    MEDIA_JSON_MAX_CHARS, MESSAGE_ID_MAX_CHARS, SENDER_ID_MAX_CHARS, SENDER_NAME_MAX_CHARS,
};
use crate::ims_bot_adapter::models::message::{collect_media_records, Message};

fn is_connection_error(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed | sqlx::Error::Io(_))
}

pub fn persist_qq_message_list(
    rdb_config: &Arc<MySqlConfig>,
    messages: &[Message],
    message_id: String,
    sender_id: String,
    sender_name: String,
    group_id: Option<String>,
    group_name: Option<String>,
) -> Result<bool> {
    let message_id = truncate_field_if_needed(
        "message_id",
        message_id.clone(),
        MESSAGE_ID_MAX_CHARS,
        &message_id,
    );
    let sender_id = truncate_field_if_needed("sender_id", sender_id, SENDER_ID_MAX_CHARS, &message_id);
    let sender_name = truncate_field_if_needed(
        "sender_name",
        sender_name,
        SENDER_NAME_MAX_CHARS,
        &message_id,
    );
    let group_id = truncate_optional_field_if_needed("group_id", group_id, GROUP_ID_MAX_CHARS, &message_id);
    let group_name =
        truncate_optional_field_if_needed("group_name", group_name, GROUP_NAME_MAX_CHARS, &message_id);
    let pool = match rdb_config.pool.clone() {
        Some(pool) => {
            let size = pool.size();
            let idle = pool.num_idle();
            let in_use = size.saturating_sub(idle as u32);
            debug!("[qq_message_list_rdb_persistence] pool size={size}, idle={idle}, in-use={in_use}");
            if idle == 0 {
                warn!("[qq_message_list_rdb_persistence] no idle connections; INSERT may stall");
            }
            pool
        }
        None => {
            error!("[qq_message_list_rdb_persistence] rdb pool has no active pool");
            return Ok(false);
        }
    };

    let content = messages.iter().map(ToString::to_string).collect::<Vec<_>>().join("");
    let content_chunks = split_content_chunks(&content, CONTENT_MAX_CHARS);
    let at_target_list = {
        let values = messages
            .iter()
            .filter_map(|message| match message {
                Message::At(at) => Some(at.target_id()),
                _ => None,
            })
            .collect::<Vec<_>>();
        (!values.is_empty()).then(|| values.join(","))
    };
    let at_target_list = truncate_optional_field_if_needed(
        "at_target_list",
        at_target_list,
        AT_TARGET_LIST_MAX_CHARS,
        &message_id,
    );
    let media_json = {
        let records = collect_media_records(messages);
        if records.is_empty() { None } else { Some(serde_json::to_string(&records)?) }
    };
    let media_json = truncate_optional_field_if_needed(
        "media_json",
        media_json,
        MEDIA_JSON_MAX_CHARS,
        &message_id,
    );
    let send_time = Local::now().naive_local();

    for attempt in 1..=2 {
        let run = async {
            for (chunk_index, content_chunk) in content_chunks.iter().enumerate() {
                sqlx::query(
                    r#"
                    INSERT INTO message_record
                    (message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(&message_id)
                .bind(&sender_id)
                .bind(&sender_name)
                .bind(send_time)
                .bind(&group_id)
                .bind(&group_name)
                .bind(content_chunk)
                .bind(if chunk_index == 0 { at_target_list.as_ref() } else { None })
                .bind(if chunk_index == 0 { media_json.as_ref() } else { None })
                .execute(&pool)
                .await?;
            }
            Ok::<(), sqlx::Error>(())
        };
        let result = if let Some(handle) = rdb_config.runtime_handle.clone() {
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
            Ok(()) => {
                info!("[qq_message_list_rdb_persistence] inserted message {message_id} (attempt {attempt})");
                return Ok(true);
            }
            Err(error) if attempt < 2 && is_connection_error(&error) => {
                warn!("[qq_message_list_rdb_persistence] retrying message {message_id}: {error}");
            }
            Err(error) => {
                error!("[qq_message_list_rdb_persistence] insert failed for message {message_id}: {error}");
                return Ok(false);
            }
        }
    }
    Ok(false)
}
