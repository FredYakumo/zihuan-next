use chrono::Local;
use log::info;
use reqwest::{Client, RequestBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::time::Duration;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::ims_bot_adapter::models::event_model::MessageEvent;
use crate::ims_bot_adapter::models::message::{collect_media_records, Message, PersistedMedia};
use crate::llm::embedding_base::EmbeddingBase;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaviateCollectionSchema {
    MessageRecordSemantic,
    ImageSemantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaviateEnsureCollectionResult {
    Existing,
    Created,
}

#[derive(Clone)]
pub struct WeaviateRef {
    pub base_url: String,
    pub class_name: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub api_key: Option<String>,
    pub timeout: Duration,
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviatePropertyConfig {
    pub name: String,
    #[serde(rename = "dataType")]
    pub data_type: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviateCollectionConfig {
    #[serde(rename = "class")]
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub properties: Vec<WeaviatePropertyConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vectorizer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviateObjectInput {
    #[serde(rename = "class")]
    pub class_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub properties: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector: Option<Vec<f32>>,
}

impl WeaviateRef {
    pub fn new(
        base_url: impl Into<String>,
        class_name: impl Into<String>,
        username: Option<String>,
        password: Option<String>,
        api_key: Option<String>,
        timeout: Duration,
    ) -> Result<Self> {
        let base_url = normalize_base_url(base_url.into())?;
        let class_name = normalize_class_name(class_name.into())?;
        let client = Client::builder().timeout(timeout).build()?;

        Ok(Self {
            base_url,
            class_name,
            username: normalize_owned_optional_string(username),
            password: normalize_owned_optional_string(password),
            api_key: api_key.filter(|value| !value.trim().is_empty()),
            timeout,
            client,
        })
    }

    pub fn ready(&self) -> Result<bool> {
        crate::runtime::block_async(self.ready_async())
    }

    pub fn meta(&self) -> Result<Value> {
        self.get_json("/v1/meta")
    }

    pub fn schema(&self) -> Result<Value> {
        self.get_json("/v1/schema")
    }

    pub fn execute_graphql_query(&self, query: &str) -> Result<Value> {
        self.post_json("/v1/graphql", json!({ "query": query }))
    }

    pub fn list_collections(&self) -> Result<Vec<String>> {
        let schema = self.schema()?;
        let classes = schema
            .get("classes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Ok(classes
            .into_iter()
            .filter_map(|class| {
                class
                    .get("class")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect())
    }

    pub fn collection_exists(&self, class_name: &str) -> Result<bool> {
        Ok(self
            .list_collections()?
            .iter()
            .any(|existing| existing == class_name))
    }

    pub fn create_collection(&self, collection: &WeaviateCollectionConfig) -> Result<Value> {
        self.post_json("/v1/schema", serde_json::to_value(collection)?)
    }

    pub fn ensure_collection(&self, collection: &WeaviateCollectionConfig) -> Result<()> {
        if self.collection_exists(&collection.class_name)? {
            return Ok(());
        }

        self.create_collection(collection)?;
        Ok(())
    }

    pub fn ensure_collection_schema(
        &self,
        schema: WeaviateCollectionSchema,
        create_missing: bool,
    ) -> Result<WeaviateEnsureCollectionResult> {
        let collection = collection_config_for_schema(schema, self.class_name.clone());
        match self.find_collection_schema(&collection.class_name)? {
            Some(existing) => {
                validate_collection_schema(&existing, &collection)?;
                Ok(WeaviateEnsureCollectionResult::Existing)
            }
            None if create_missing => {
                self.create_collection(&collection)?;
                Ok(WeaviateEnsureCollectionResult::Created)
            }
            None => Err(Error::ValidationError(format!(
                "Weaviate collection '{}' does not exist",
                collection.class_name
            ))),
        }
    }

    pub fn find_collection_schema(&self, class_name: &str) -> Result<Option<Value>> {
        let schema = self.schema()?;
        let classes = schema
            .get("classes")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Ok(classes.into_iter().find(|class| {
            class
                .get("class")
                .and_then(Value::as_str)
                .map(|name| name == class_name)
                .unwrap_or(false)
        }))
    }

    pub fn delete_collection(&self, class_name: &str) -> Result<()> {
        self.delete_empty(&format!("/v1/schema/{class_name}"))
    }

    pub fn upsert_object(
        &self,
        class_name: &str,
        properties: Value,
        vector: Option<Vec<f32>>,
        id: Option<&str>,
    ) -> Result<Value> {
        let mut payload = json!({
            "class": class_name,
            "properties": properties,
        });
        if let Some(id) = id.filter(|value| !value.trim().is_empty()) {
            payload["id"] = Value::String(id.to_string());
        }
        if let Some(vector) = vector {
            payload["vector"] = serde_json::to_value(vector)?;
        }
        self.post_json("/v1/objects", payload)
    }

    pub fn upsert_message_event(
        &self,
        event: &MessageEvent,
        embedding_model: &dyn EmbeddingBase,
    ) -> Result<Value> {
        let sender_name = if event.sender.card.trim().is_empty() {
            event.sender.nickname.as_str()
        } else {
            event.sender.card.as_str()
        };
        let group_id = event.group_id.map(|value| value.to_string());
        self.upsert_qq_message_list(
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
        &self,
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
        let object_id = deterministic_message_object_id(&self.class_name, &message_id);

        info!(
            "[WeaviateRef] Upserting message {} into class {}",
            message_id, self.class_name
        );

        self.upsert_object(&self.class_name, properties, Some(vector), Some(&object_id))
    }

    pub fn upsert_image_record(
        &self,
        media: &PersistedMedia,
        vector: &[f32],
    ) -> Result<Value> {
        if vector.is_empty() {
            return Err(Error::ValidationError(
                "vector must not be empty".to_string(),
            ));
        }
        let properties = build_image_record_properties(media)?;
        let object_id = deterministic_media_object_id(&self.class_name, &media.media_id);
        self.upsert_object(
            &self.class_name,
            properties,
            Some(vector.to_vec()),
            Some(&object_id),
        )
    }

    pub fn batch_upsert_objects(&self, objects: &[WeaviateObjectInput]) -> Result<Value> {
        self.post_json("/v1/batch/objects", json!({ "objects": objects }))
    }

    pub fn get_object(&self, class_name: &str, id: &str) -> Result<Value> {
        self.get_json(&format!("/v1/objects/{class_name}/{id}"))
    }

    pub fn delete_object(&self, class_name: &str, id: &str) -> Result<()> {
        self.delete_empty(&format!("/v1/objects/{class_name}/{id}"))
    }

    pub fn query_near_vector(
        &self,
        class_name: &str,
        vector: &[f32],
        limit: usize,
        property_names: &[String],
        include_distance: bool,
        include_vector: bool,
    ) -> Result<Value> {
        let mut requested_fields = property_names
            .iter()
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>();
        let mut additional_fields = vec!["id".to_string()];
        if include_distance {
            additional_fields.push("distance".to_string());
        }
        if include_vector {
            additional_fields.push("vector".to_string());
        }
        requested_fields.push(format!("_additional {{ {} }}", additional_fields.join(" ")));
        let vector_body = vector
            .iter()
            .map(|value| {
                let mut rendered = value.to_string();
                if !rendered.contains('.') && !rendered.contains('e') && !rendered.contains('E') {
                    rendered.push_str(".0");
                }
                rendered
            })
            .collect::<Vec<_>>()
            .join(", ");
        let fields = requested_fields.join(" ");
        let graphql = format!(
            "{{ Get {{ {class_name}(nearVector: {{ vector: [{vector_body}] }}, limit: {limit}) {{ {fields} }} }} }}"
        );
        self.execute_graphql_query(&graphql)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn authorized(&self, builder: RequestBuilder) -> RequestBuilder {
        if let Some(api_key) = &self.api_key {
            builder.bearer_auth(api_key)
        } else if self.username.is_some() || self.password.is_some() {
            builder.basic_auth(
                self.username.clone().unwrap_or_default(),
                self.password.clone(),
            )
        } else {
            builder
        }
    }

    fn get_json(&self, path: &str) -> Result<Value> {
        crate::runtime::block_async(self.get_json_async(path))
    }

    fn post_json(&self, path: &str, body: Value) -> Result<Value> {
        crate::runtime::block_async(self.post_json_async(path, body))
    }

    fn delete_empty(&self, path: &str) -> Result<()> {
        crate::runtime::block_async(self.delete_empty_async(path))
    }

    async fn ready_async(&self) -> Result<bool> {
        let response = self
            .authorized(self.client.get(self.url("/v1/.well-known/ready")))
            .send()
            .await?;
        if response.status().is_success() {
            return Ok(true);
        }
        if response.status().as_u16() == 503 {
            return Ok(false);
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(Error::StringError(format!(
            "Weaviate readiness probe failed with status {}: {}",
            status, body
        )))
    }

    async fn get_json_async(&self, path: &str) -> Result<Value> {
        Self::send_json_async(self.authorized(self.client.get(self.url(path)))).await
    }

    async fn post_json_async(&self, path: &str, body: Value) -> Result<Value> {
        Self::send_json_async(
            self.authorized(self.client.post(self.url(path)))
                .json(&body),
        )
        .await
    }

    async fn delete_empty_async(&self, path: &str) -> Result<()> {
        let response = self
            .authorized(self.client.delete(self.url(path)))
            .send()
            .await?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(Error::StringError(format!(
            "Weaviate request failed with status {}: {}",
            status, body
        )))
    }

    async fn send_json_async(builder: RequestBuilder) -> Result<Value> {
        let response = builder.send().await?;
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(Error::StringError(format!(
                "Weaviate request failed with status {}: {}",
                status, body
            )));
        }
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|err| {
            Error::StringError(format!(
                "Failed to parse Weaviate response as JSON: {err}; body={body}"
            ))
        })
    }
}

impl fmt::Debug for WeaviateRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WeaviateRef")
            .field("base_url", &self.base_url)
            .field("class_name", &self.class_name)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("timeout", &self.timeout)
            .finish()
    }
}

fn normalize_base_url(raw: String) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(
            "Weaviate base_url must not be empty".to_string(),
        ));
    }
    let parsed = reqwest::Url::parse(&trimmed).map_err(|err| {
        Error::ValidationError(format!("Invalid Weaviate base_url '{trimmed}': {err}"))
    })?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(Error::ValidationError(format!(
            "Weaviate base_url must use http or https, got '{scheme}'"
        )));
    }
    Ok(trimmed)
}

fn normalize_class_name(raw: String) -> Result<String> {
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(
            "Weaviate class_name must not be empty".to_string(),
        ));
    }
    Ok(trimmed)
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

fn normalize_owned_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn deterministic_message_object_id(class_name: &str, message_id: &str) -> String {
    let _ = (class_name, message_id);
    Uuid::new_v4().to_string()
}

pub fn collection_config_for_schema(
    schema: WeaviateCollectionSchema,
    class_name: String,
) -> WeaviateCollectionConfig {
    match schema {
        WeaviateCollectionSchema::MessageRecordSemantic => {
            message_vector_collection_config(class_name)
        }
        WeaviateCollectionSchema::ImageSemantic => image_vector_collection_config(class_name),
    }
}

pub fn validate_collection_schema(
    existing: &Value,
    expected: &WeaviateCollectionConfig,
) -> Result<()> {
    let existing_name = existing
        .get("class")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if existing_name != expected.class_name {
        return Err(Error::ValidationError(format!(
            "Weaviate collection name mismatch: expected '{}', got '{}'",
            expected.class_name, existing_name
        )));
    }
    let expected_vectorizer = expected.vectorizer.as_deref().unwrap_or_default();
    let existing_vectorizer = existing
        .get("vectorizer")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if existing_vectorizer != expected_vectorizer {
        return Err(Error::ValidationError(format!(
            "Weaviate collection '{}' vectorizer mismatch: expected '{}', got '{}'",
            expected.class_name, expected_vectorizer, existing_vectorizer
        )));
    }
    let existing_properties = existing
        .get("properties")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for expected_property in &expected.properties {
        let Some(existing_property) = existing_properties.iter().find(|property| {
            property
                .get("name")
                .and_then(Value::as_str)
                .map(|name| name == expected_property.name)
                .unwrap_or(false)
        }) else {
            return Err(Error::ValidationError(format!(
                "Weaviate collection '{}' missing property '{}'",
                expected.class_name, expected_property.name
            )));
        };
        let existing_data_type = existing_property
            .get("dataType")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if existing_data_type != expected_property.data_type {
            return Err(Error::ValidationError(format!(
                "Weaviate collection '{}' property '{}' dataType mismatch: expected {:?}, got {:?}",
                expected.class_name,
                expected_property.name,
                expected_property.data_type,
                existing_data_type
            )));
        }
    }
    Ok(())
}

fn message_vector_collection_config(class_name: String) -> WeaviateCollectionConfig {
    WeaviateCollectionConfig {
        class_name,
        description: Some("QQ message vector storage".to_string()),
        properties: vec![
            text_property("message_id", "QQ 平台消息 ID"),
            text_property("sender_id", "发送者 ID"),
            text_property("sender_name", "发送者名称"),
            date_property("send_time", "消息发送时间"),
            text_property("group_id", "群 ID，可为空"),
            text_property("group_name", "群名称，可为空"),
            text_property("content", "聚合后的消息文本"),
            text_property("at_target_list", "@ 提及目标列表"),
            text_property("media_json", "消息媒体元数据 JSON"),
        ],
        vectorizer: Some("none".to_string()),
    }
}

fn text_property(name: &str, description: &str) -> WeaviatePropertyConfig {
    WeaviatePropertyConfig {
        name: name.to_string(),
        data_type: vec!["text".to_string()],
        description: Some(description.to_string()),
    }
}

fn date_property(name: &str, description: &str) -> WeaviatePropertyConfig {
    WeaviatePropertyConfig {
        name: name.to_string(),
        data_type: vec!["date".to_string()],
        description: Some(description.to_string()),
    }
}

fn image_vector_collection_config(class_name: String) -> WeaviateCollectionConfig {
    WeaviateCollectionConfig {
        class_name,
        description: Some("Image vector storage".to_string()),
        properties: vec![
            text_property("media_id", "持久化媒体ID"),
            text_property("original_source", "原始来源字符串"),
            text_property("rustfs_path", "RustFS对象路径"),
            text_property("name", "媒体名称"),
            text_property("description", "图片总结说明"),
            text_property("mime_type", "媒体MIME类型"),
            text_property("source", "来源标记，如 upload/qq_chat/web_search"),
        ],
        vectorizer: Some("none".to_string()),
    }
}

fn deterministic_media_object_id(class_name: &str, media_id: &str) -> String {
    let seed = format!("{class_name}:{media_id}");
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    format!("media-object-{:016x}", hasher.finish())
}

fn build_image_record_properties(media: &PersistedMedia) -> Result<Value> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};

    #[test]
    fn image_collection_schema_matches_persisted_media_fields() {
        let config = image_vector_collection_config("ImageCollection".to_string());
        let fields = config
            .properties
            .iter()
            .map(|property| property.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            fields,
            vec![
                "media_id",
                "original_source",
                "rustfs_path",
                "name",
                "description",
                "mime_type",
                "source"
            ]
        );
    }

    #[test]
    fn image_record_properties_map_from_persisted_media() {
        let media = PersistedMedia::new(
            PersistedMediaSource::QqChat,
            "https://multimedia.nt.qq.com.cn/download?fileid=1",
            "qq-images/2026/05/16/1.jpg",
            Some("download".to_string()),
            Some("图片描述".to_string()),
            Some("image/jpeg".to_string()),
        );
        let properties = build_image_record_properties(&media).expect("build image properties");
        assert_eq!(properties["media_id"], media.media_id);
        assert_eq!(properties["original_source"], media.original_source);
        assert_eq!(properties["rustfs_path"], media.rustfs_path);
        assert_eq!(properties["description"], "图片描述");
        assert_eq!(properties["mime_type"], "image/jpeg");
        assert_eq!(properties["source"], "qq_chat");
    }
}
