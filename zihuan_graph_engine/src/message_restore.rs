use crate::data_value::MySqlConfig;
use crate::message_mysql_history_common::run_mysql_query;
use log::{debug, warn};
use once_cell::sync::Lazy;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use zihuan_core::ims_bot_adapter::models::event_model::MessageEvent;
use zihuan_core::ims_bot_adapter::models::message::{ImageMessage, Message, MessageMediaRecord, PlainTextMessage};
use zihuan_core::error::Result;

static RUNTIME_MESSAGE_INDEX: Lazy<RwLock<HashMap<String, Vec<Message>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));
static LATEST_MYSQL_REF: Lazy<RwLock<Option<Arc<MySqlConfig>>>> = Lazy::new(|| RwLock::new(None));

const LOOKUP_SQL: &str = r#"
    SELECT content, media_json
    FROM message_record
    WHERE message_id = ?
    ORDER BY id ASC
    "#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRestoreSource {
    RuntimeCache,
    MySql,
}

impl MessageRestoreSource {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageRestoreSource::RuntimeCache => "cache",
            MessageRestoreSource::MySql => "mysql",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RestoredMessageSnapshot {
    pub messages: Vec<Message>,
    pub source: MessageRestoreSource,
}

pub fn cache_message_snapshot(event: &MessageEvent) {
    if let Ok(mut guard) = RUNTIME_MESSAGE_INDEX.write() {
        guard.insert(event.message_id.to_string(), event.message_list.clone());
    }
}

pub fn register_mysql_ref(config: Arc<MySqlConfig>) {
    if let Ok(mut guard) = LATEST_MYSQL_REF.write() {
        *guard = Some(config);
    }
}

pub fn restore_message_snapshot(message_id: i64) -> Result<Option<RestoredMessageSnapshot>> {
    let message_id_str = message_id.to_string();

    if let Ok(guard) = RUNTIME_MESSAGE_INDEX.read() {
        if let Some(messages) = guard.get(&message_id_str) {
            return Ok(Some(RestoredMessageSnapshot {
                messages: messages.clone(),
                source: MessageRestoreSource::RuntimeCache,
            }));
        }
    }

    let mysql_config = match LATEST_MYSQL_REF.read() {
        Ok(guard) => guard.clone(),
        Err(_) => None,
    };

    let Some(mysql_config) = mysql_config else {
        return Ok(None);
    };

    let lookup_id = message_id_str.clone();
    let rows = run_mysql_query(&mysql_config, move |pool| {
        Box::pin(async move {
            sqlx::query(LOOKUP_SQL)
                .bind(&lookup_id)
                .fetch_all(pool)
                .await
        })
    })?;

    if rows.is_empty() {
        return Ok(None);
    }

    let mut content = String::new();
    let mut media_json = None;
    for row in rows {
        let chunk_content: String = row.get("content");
        let chunk_media_json: Option<String> = row.get("media_json");
        content.push_str(&chunk_content);
        if media_json.is_none() {
            media_json = chunk_media_json;
        }
    }
    let messages = rebuild_message_list(&content, media_json.as_deref());

    if messages.is_empty() {
        debug!(
            "[message_restore] message {} found in mysql but rebuilt into an empty message list",
            message_id
        );
        return Ok(None);
    }

    if let Ok(mut guard) = RUNTIME_MESSAGE_INDEX.write() {
        guard.insert(message_id_str, messages.clone());
    }

    Ok(Some(RestoredMessageSnapshot {
        messages,
        source: MessageRestoreSource::MySql,
    }))
}

fn rebuild_message_list(content: &str, media_json: Option<&str>) -> Vec<Message> {
    let mut messages = Vec::new();
    let trimmed_content = content.trim();
    if !trimmed_content.is_empty() {
        messages.push(Message::PlainText(PlainTextMessage {
            text: trimmed_content.to_string(),
        }));
    }

    let Some(media_json) = media_json.filter(|value| !value.trim().is_empty()) else {
        return messages;
    };

    let records: Vec<MessageMediaRecord> = match serde_json::from_str(media_json) {
        Ok(records) => records,
        Err(error) => {
            warn!(
                "[message_restore] failed to parse media_json while rebuilding message: {}",
                error
            );
            return messages;
        }
    };

    for record in records {
        match record.r#type.as_str() {
            "image" => {
                let image = Message::Image(ImageMessage {
                    file: record.file.clone(),
                    path: record.path.clone(),
                    url: record.url.clone(),
                    name: record.name.clone(),
                    thumb: record.thumb.clone(),
                    summary: record.summary.clone(),
                    sub_type: record.sub_type,
                    object_key: record.object_key.clone(),
                    object_url: record.object_url.clone(),
                    local_path: record.path.clone(),
                    cache_status: record.cache_status.clone(),
                });
                let insert_at = record.segment_index.min(messages.len());
                messages.insert(insert_at, image);
            }
            other => {
                debug!(
                    "[message_restore] skipping unsupported media record type={} during rebuild",
                    other
                );
            }
        }
    }

    messages
}
