use chrono::Local;
use log::info;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use uuid::Uuid;

use zihuan_core::error::{Error, Result};
use zihuan_core::ims_bot_adapter::models::event_model::MessageEvent;
use zihuan_core::ims_bot_adapter::models::message::{
    collect_media_records, Message, PersistedMedia,
};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::weaviate::WeaviateRef;

pub fn upsert_message_event(
    weaviate_ref: &WeaviateRef,
    event: &MessageEvent,
    embedding_model: &dyn EmbeddingBase,
) -> Result<Value> {
    let sender_name = if event.sender.card.trim().is_empty() {
        event.sender.nickname.as_str()
    } else {
        event.sender.card.as_str()
    };
    let group_id = event.group_id.map(|value| value.to_string());
    upsert_qq_message_list(
        weaviate_ref,
        &event.message_list,
        &event.message_id.to_string(),
        &event.sender.user_id.to_string(),
        sender_name,
        group_id.as_deref(),
        event.group_name.as_deref(),
        embedding_model,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn upsert_qq_message_list(
    weaviate_ref: &WeaviateRef,
    messages: &[Message],
    message_id: &str,
    sender_id: &str,
    sender_name: &str,
    group_id: Option<&str>,
    group_name: Option<&str>,
    embedding_model: &dyn EmbeddingBase,
) -> Result<Value> {
    let message_id = required_non_empty_string(message_id, "message_id")?;
    let sender_id = required_non_empty_string(sender_id, "sender_id")?;
    let sender_name = required_non_empty_string(sender_name, "sender_name")?;
    let group_id = normalize_optional_string(group_id);
    let group_name = normalize_optional_string(group_name);
    let content = messages
        .iter()
        .map(|message| message.to_string())
        .collect::<Vec<_>>()
        .join("");
    if content.trim().is_empty() {
        return Err(Error::ValidationError(
            "qq_message_list content must not be empty".to_string(),
        ));
    }

    let at_targets: Vec<String> = messages
        .iter()
        .filter_map(|message| match message {
            Message::At(at) => Some(at.target_id()),
            _ => None,
        })
        .collect();
    let at_target_list = (!at_targets.is_empty()).then(|| at_targets.join(","));
    let media_json = {
        let records = collect_media_records(messages);
        (!records.is_empty())
            .then(|| serde_json::to_string(&records))
            .transpose()?
    };
    let properties = json!({
        "message_id": message_id,
        "sender_id": sender_id,
        "sender_name": sender_name,
        "send_time": Local::now().to_rfc3339(),
        "group_id": group_id,
        "group_name": group_name,
        "content": content,
        "at_target_list": at_target_list,
        "media_json": media_json,
    });
    let vector = embedding_model.inference(
        properties
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    )?;
    let object_id = deterministic_message_object_id(&weaviate_ref.class_name, &message_id);

    info!(
        "[WeaviateRef] Upserting message {} into class {}",
        message_id, weaviate_ref.class_name
    );

    weaviate_ref.upsert_object(
        &weaviate_ref.class_name,
        properties,
        Some(vector),
        Some(&object_id),
    )
}

pub fn upsert_image_record(
    weaviate_ref: &WeaviateRef,
    media: &PersistedMedia,
    description_vector: &[f32],
    name_vector: Option<&[f32]>,
) -> Result<Value> {
    if description_vector.is_empty() {
        return Err(Error::ValidationError(
            "description_vector must not be empty".to_string(),
        ));
    }
    let properties = build_image_record_properties(media)?;
    let object_id = deterministic_media_object_id(&weaviate_ref.class_name, &media.media_id);

    let mut vectors = HashMap::new();
    vectors.insert(
        "description_vector".to_string(),
        description_vector.to_vec(),
    );
    if let Some(name_vector) = name_vector {
        if !name_vector.is_empty() {
            vectors.insert("name_vector".to_string(), name_vector.to_vec());
        }
    }

    weaviate_ref.upsert_object_with_vectors(
        &weaviate_ref.class_name,
        properties,
        vectors,
        Some(&object_id),
    )
}

pub fn deterministic_media_object_id(class_name: &str, media_id: &str) -> String {
    let seed = format!("{class_name}:{media_id}");
    let mut first = DefaultHasher::new();
    seed.hash(&mut first);

    let mut second = DefaultHasher::new();
    format!("media:{seed}").hash(&mut second);

    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&first.finish().to_be_bytes());
    bytes[8..].copy_from_slice(&second.finish().to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    Uuid::from_bytes(bytes).to_string()
}

pub fn deterministic_message_object_id(_class_name: &str, _message_id: &str) -> String {
    Uuid::new_v4().to_string()
}

pub fn build_image_record_properties(media: &PersistedMedia) -> Result<Value> {
    let media_id = required_non_empty_string(&media.media_id, "media_id")?;
    let original_source = required_non_empty_string(&media.original_source, "original_source")?;
    let rustfs_path = required_non_empty_string(&media.rustfs_path, "rustfs_path")?;

    Ok(json!({
        "media_id": media_id,
        "original_source": original_source,
        "rustfs_path": rustfs_path,
        "name": normalize_optional_string(media.name.as_deref()),
        "description": normalize_optional_string(media.description.as_deref()),
        "mime_type": normalize_optional_string(media.mime_type.as_deref()),
        "source": media.source.to_string(),
    }))
}

fn required_non_empty_string(value: &str, field_name: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(format!(
            "{field_name} must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zihuan_core::ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};

    #[test]
    fn image_record_properties_map_from_persisted_media() {
        let media = PersistedMedia::new(
            PersistedMediaSource::QqChat,
            "https://multimedia.nt.qq.com.cn/download?fileid=1",
            "qq-images/2026/05/16/1.jpg",
            Some("download".to_string()),
            Some("Image description".to_string()),
            Some("image/jpeg".to_string()),
        );
        let properties = build_image_record_properties(&media).expect("build image properties");
        assert_eq!(properties["media_id"], media.media_id);
        assert_eq!(properties["original_source"], media.original_source);
        assert_eq!(properties["rustfs_path"], media.rustfs_path);
        assert_eq!(properties["description"], "Image description");
        assert_eq!(properties["mime_type"], "image/jpeg");
        assert_eq!(properties["source"], "qq_chat");
    }

    #[test]
    fn deterministic_media_object_id_is_stable_uuid() {
        let id1 = deterministic_media_object_id("ImageSemantic", "media-123");
        let id2 = deterministic_media_object_id("ImageSemantic", "media-123");

        assert_eq!(id1, id2);
        assert!(Uuid::parse_str(&id1).is_ok());
    }
}
