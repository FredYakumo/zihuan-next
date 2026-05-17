use crate::data_value::RedisConfig;
use crate::message_mysql_history_common::run_mysql_query;
use log::{debug, warn};
use once_cell::sync::Lazy;
use redis::AsyncCommands;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::task::block_in_place;
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::Result;
use zihuan_core::ims_bot_adapter::models::event_model::MessageEvent;
use zihuan_core::ims_bot_adapter::models::message::{
    ImageMessage, Message, MessageMediaRecord, PersistedMedia, PlainTextMessage,
};

static RUNTIME_MESSAGE_INDEX: Lazy<RwLock<HashMap<String, Vec<Message>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));
static LATEST_MYSQL_REF: Lazy<RwLock<Option<Arc<MySqlConfig>>>> = Lazy::new(|| RwLock::new(None));
static LATEST_REDIS_REF: Lazy<RwLock<Option<Arc<RedisConfig>>>> = Lazy::new(|| RwLock::new(None));

const LOOKUP_SQL: &str = r#"
    SELECT content, media_json, raw_message_json
    FROM message_record
    WHERE message_id = ?
    ORDER BY id ASC
    "#;

const MEDIA_LOOKUP_SQL: &str = r#"
    SELECT media_json, raw_message_json
    FROM message_record
    WHERE media_json LIKE ? OR raw_message_json LIKE ?
    ORDER BY id DESC
    LIMIT 50
    "#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRestoreSource {
    RuntimeCache,
    Redis,
    MySql,
}

impl MessageRestoreSource {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageRestoreSource::RuntimeCache => "cache",
            MessageRestoreSource::Redis => "redis",
            MessageRestoreSource::MySql => "mysql",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RestoredMessageSnapshot {
    pub messages: Vec<Message>,
    pub source: MessageRestoreSource,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedMessageSnapshotPayload {
    pub message_id: String,
    pub content: String,
    pub media_json: Option<String>,
    pub raw_message_json: Option<String>,
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

pub fn register_redis_ref(config: Arc<RedisConfig>) {
    if let Ok(mut guard) = LATEST_REDIS_REF.write() {
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
    let redis_config = match LATEST_REDIS_REF.read() {
        Ok(guard) => guard.clone(),
        Err(_) => None,
    };

    if let Some(redis_config) = redis_config {
        if let Some(snapshot) = restore_message_snapshot_from_redis(&redis_config, &message_id_str)?
        {
            if let Ok(mut guard) = RUNTIME_MESSAGE_INDEX.write() {
                guard.insert(message_id_str.clone(), snapshot.messages.clone());
            }
            return Ok(Some(snapshot));
        }
    }

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
    let mut raw_message_json = None;
    for row in rows {
        let chunk_content: String = row.get("content");
        let chunk_media_json: Option<String> = row.get("media_json");
        let chunk_raw_message_json: Option<String> = row.get("raw_message_json");
        content.push_str(&chunk_content);
        if media_json.is_none() {
            media_json = chunk_media_json;
        }
        if raw_message_json.is_none() {
            raw_message_json = chunk_raw_message_json;
        }
    }
    let messages = raw_message_json
        .as_deref()
        .and_then(rebuild_message_list_from_raw_json)
        .unwrap_or_else(|| rebuild_message_list(&content, media_json.as_deref()));

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

pub fn restore_media_by_id(media_id: &str) -> Result<Option<PersistedMedia>> {
    let media_id = media_id.trim();
    if media_id.is_empty() {
        return Ok(None);
    }

    if let Ok(guard) = RUNTIME_MESSAGE_INDEX.read() {
        for messages in guard.values() {
            if let Some(media) = find_media_in_messages(messages, media_id) {
                return Ok(Some(media));
            }
        }
    }

    let mysql_config = match LATEST_MYSQL_REF.read() {
        Ok(guard) => guard.clone(),
        Err(_) => None,
    };
    let Some(mysql_config) = mysql_config else {
        return Ok(None);
    };

    let like_pattern = format!("%{media_id}%");
    let rows = run_mysql_query(&mysql_config, move |pool| {
        let like_pattern_media = like_pattern.clone();
        let like_pattern_raw = like_pattern.clone();
        Box::pin(async move {
            sqlx::query(MEDIA_LOOKUP_SQL)
                .bind(like_pattern_media)
                .bind(like_pattern_raw)
                .fetch_all(pool)
                .await
        })
    })?;

    for row in rows {
        let raw_message_json: Option<String> = row.get("raw_message_json");
        if let Some(messages) = raw_message_json
            .as_deref()
            .and_then(rebuild_message_list_from_raw_json)
        {
            if let Some(media) = find_media_in_messages(&messages, media_id) {
                return Ok(Some(media));
            }
        }

        let media_json: Option<String> = row.get("media_json");
        if let Some(media) = media_json
            .as_deref()
            .and_then(|value| find_media_in_media_json(value, media_id))
        {
            return Ok(Some(media));
        }
    }

    Ok(None)
}

fn restore_message_snapshot_from_redis(
    redis_ref: &Arc<RedisConfig>,
    message_id: &str,
) -> Result<Option<RestoredMessageSnapshot>> {
    let Some(url) = redis_ref.url.clone() else {
        return Ok(None);
    };

    let redis_ref = Arc::clone(redis_ref);
    let message_id = message_id.to_string();
    let message_id_for_get = message_id.clone();
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

        let Some(cm) = cm_guard.as_mut() else {
            return Ok::<Option<String>, zihuan_core::error::Error>(None);
        };

        let payload: Option<String> = cm.get(&message_id_for_get).await?;
        Ok::<Option<String>, zihuan_core::error::Error>(payload)
    };

    let payload = if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }?;

    let Some(payload) = payload else {
        return Ok(None);
    };

    let snapshot: CachedMessageSnapshotPayload = match serde_json::from_str(&payload) {
        Ok(value) => value,
        Err(error) => {
            warn!(
                "[message_restore] failed to parse Redis cached snapshot for message {}: {}",
                message_id, error
            );
            return Ok(None);
        }
    };

    let messages = snapshot
        .raw_message_json
        .as_deref()
        .and_then(rebuild_message_list_from_raw_json)
        .unwrap_or_else(|| rebuild_message_list(&snapshot.content, snapshot.media_json.as_deref()));

    if messages.is_empty() {
        return Ok(None);
    }

    Ok(Some(RestoredMessageSnapshot {
        messages,
        source: MessageRestoreSource::Redis,
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
                let image = Message::Image(ImageMessage::new(PersistedMedia {
                    media_id: record.media_id.clone(),
                    source: record.source.clone(),
                    original_source: record.original_source.clone(),
                    rustfs_path: record.rustfs_path.clone(),
                    name: record.name.clone(),
                    description: record.description.clone(),
                    mime_type: record.mime_type.clone(),
                }));
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

fn rebuild_message_list_from_raw_json(raw_message_json: &str) -> Option<Vec<Message>> {
    if raw_message_json.trim().is_empty() {
        return None;
    }

    match serde_json::from_str::<Vec<Message>>(raw_message_json) {
        Ok(messages) if messages.is_empty() => None,
        Ok(messages) => Some(messages),
        Err(error) => {
            warn!(
                "[message_restore] failed to parse raw_message_json while rebuilding message: {}",
                error
            );
            None
        }
    }
}

fn find_media_in_messages(messages: &[Message], media_id: &str) -> Option<PersistedMedia> {
    for message in messages {
        match message {
            Message::Image(image) if image.media.media_id == media_id => {
                return Some(image.media.clone());
            }
            Message::Forward(forward) => {
                for node in &forward.content {
                    if let Some(media) = find_media_in_messages(&node.content, media_id) {
                        return Some(media);
                    }
                }
            }
            _ => {}
        }
    }

    None
}

fn find_media_in_media_json(media_json: &str, media_id: &str) -> Option<PersistedMedia> {
    let records: Vec<MessageMediaRecord> = serde_json::from_str(media_json).ok()?;
    records
        .into_iter()
        .find(|record| record.r#type == "image" && record.media_id == media_id)
        .map(|record| PersistedMedia {
            media_id: record.media_id,
            source: record.source,
            original_source: record.original_source,
            rustfs_path: record.rustfs_path,
            name: record.name,
            description: record.description,
            mime_type: record.mime_type,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zihuan_core::ims_bot_adapter::models::message::collect_media_records;
    use zihuan_core::ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};

    #[test]
    fn rebuild_message_list_from_media_json_restores_persisted_media_image() {
        let media_json = serde_json::to_string(&vec![MessageMediaRecord {
            segment_index: 0,
            r#type: "image".to_string(),
            media_id: "media-1".to_string(),
            source: PersistedMediaSource::QqChat,
            original_source: "https://multimedia.nt.qq.com.cn/download?fileid=1".to_string(),
            rustfs_path: "qq-images/2026/05/16/1.jpg".to_string(),
            name: Some("download".to_string()),
            description: Some("图片描述".to_string()),
            mime_type: Some("image/jpeg".to_string()),
        }])
        .expect("serialize media json");

        let messages = rebuild_message_list("", Some(&media_json));
        match &messages[0] {
            Message::Image(image) => {
                assert_eq!(image.media.media_id, "media-1");
                assert_eq!(image.media.rustfs_path, "qq-images/2026/05/16/1.jpg");
                assert_eq!(image.media.mime_type.as_deref(), Some("image/jpeg"));
            }
            other => panic!("expected image message, got {other:?}"),
        }
    }

    #[test]
    fn rebuild_message_list_from_raw_json_restores_nested_media() {
        let messages = vec![Message::Image(ImageMessage::new(PersistedMedia::new(
            PersistedMediaSource::Upload,
            "upload://manual/demo",
            "uploads/demo.png",
            Some("demo.png".to_string()),
            None,
            Some("image/png".to_string()),
        )))];
        let raw_json = serde_json::to_string(&messages).expect("serialize messages");
        let restored = rebuild_message_list_from_raw_json(&raw_json).expect("restore raw json");
        assert_eq!(restored.len(), 1);
        match &restored[0] {
            Message::Image(image) => {
                assert_eq!(image.media.rustfs_path, "uploads/demo.png");
            }
            other => panic!("expected image message, got {other:?}"),
        }
    }

    #[test]
    fn redis_snapshot_payload_roundtrip_restores_media_ids() {
        let messages = vec![Message::Image(ImageMessage::new(PersistedMedia::new(
            PersistedMediaSource::QqChat,
            "https://multimedia.nt.qq.com.cn/download?fileid=1",
            "qq-images/2026/05/16/1.jpg",
            Some("download".to_string()),
            Some("图片描述".to_string()),
            Some("image/jpeg".to_string()),
        )))];
        let payload = CachedMessageSnapshotPayload {
            message_id: "1".to_string(),
            content: String::new(),
            media_json: Some(
                serde_json::to_string(&collect_media_records(&messages)).expect("serialize media"),
            ),
            raw_message_json: Some(
                serde_json::to_string(&messages).expect("serialize raw message json"),
            ),
        };

        let restored = payload
            .raw_message_json
            .as_deref()
            .and_then(rebuild_message_list_from_raw_json)
            .expect("restore raw json");
        match &restored[0] {
            Message::Image(image) => {
                assert!(image.media.media_id.starts_with("media-"));
                assert_eq!(image.media.rustfs_path, "qq-images/2026/05/16/1.jpg");
            }
            other => panic!("expected image message, got {other:?}"),
        }
    }

    #[test]
    fn find_media_in_media_json_matches_media_id() {
        let media_json = serde_json::to_string(&vec![MessageMediaRecord {
            segment_index: 0,
            r#type: "image".to_string(),
            media_id: "media-lookup".to_string(),
            source: PersistedMediaSource::WebSearch,
            original_source: "https://example.com/demo.jpg".to_string(),
            rustfs_path: "tavily/demo.jpg".to_string(),
            name: None,
            description: Some("demo".to_string()),
            mime_type: Some("image/jpeg".to_string()),
        }])
        .expect("serialize media json");

        let media =
            find_media_in_media_json(&media_json, "media-lookup").expect("find media by id");
        assert_eq!(media.media_id, "media-lookup");
        assert_eq!(media.rustfs_path, "tavily/demo.jpg");
        assert_eq!(media.mime_type.as_deref(), Some("image/jpeg"));
    }
}
