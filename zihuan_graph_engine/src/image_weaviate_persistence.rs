use crate::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use log::error;
use std::collections::HashMap;
use zihuan_core::error::{Error, Result};
use zihuan_core::ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};

pub struct ImageWeaviatePersistenceNode {
    id: String,
    name: String,
}

impl ImageWeaviatePersistenceNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ImageWeaviatePersistenceNode {
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("图片向量持久化 - 将对象存储路径 + 向量 + 总结写入 Weaviate")
    }

    node_input![
        port! { name = "object_storage_path", ty = String, desc = "对象存储路径（object_key/object_url）" },
        port! { name = "vector", ty = Vector, desc = "图片语义向量" },
        port! { name = "description", ty = String, desc = "图片总结说明" },
        port! { name = "weaviate_ref", ty = zihuan_core::weaviate::WeaviateRef, desc = "Weaviate连接配置引用" },
        port! { name = "source", ty = String, desc = "可选：图片来源（qq/tavily等）", optional },
        port! { name = "media_id", ty = String, desc = "可选：持久化媒体ID", optional },
        port! { name = "original_source", ty = String, desc = "可选：原始来源字符串", optional },
        port! { name = "name", ty = String, desc = "可选：媒体名称", optional },
        port! { name = "mime_type", ty = String, desc = "可选：媒体MIME类型", optional },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "是否存储成功" },
        port! { name = "object_storage_path", ty = String, desc = "透传对象存储路径" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let object_storage_path = required_string(&inputs, "object_storage_path")?;
        let description = required_string(&inputs, "description")?;
        let vector = inputs
            .get("vector")
            .and_then(|v| match v {
                DataValue::Vector(value) => Some(value.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("vector is required".to_string()))?;

        if vector.is_empty() {
            return Err(Error::ValidationError(
                "vector must not be empty".to_string(),
            ));
        }

        let weaviate_ref = inputs
            .get("weaviate_ref")
            .and_then(|v| match v {
                DataValue::WeaviateRef(r) => Some(r.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("weaviate_ref is required".to_string()))?;

        let source = parse_media_source(optional_non_empty_string(&inputs, "source").as_deref());
        let media_id = optional_non_empty_string(&inputs, "media_id");
        let original_source = optional_non_empty_string(&inputs, "original_source")
            .unwrap_or_else(|| object_storage_path.clone());
        let name = optional_non_empty_string(&inputs, "name");
        let mime_type = optional_non_empty_string(&inputs, "mime_type");
        let media = if let Some(media_id) = media_id {
            PersistedMedia {
                media_id,
                source,
                original_source,
                rustfs_path: object_storage_path.clone(),
                name,
                description: Some(description.clone()),
                mime_type,
            }
        } else {
            PersistedMedia::new(
                source,
                original_source,
                object_storage_path.clone(),
                name,
                Some(description.clone()),
                mime_type,
            )
        };

        let success = match weaviate_ref.upsert_image_record(&media, &vector) {
            Ok(_) => true,
            Err(err) => {
                error!(
                    "[ImageWeaviatePersistenceNode] Failed to persist image vector: {}",
                    err
                );
                false
            }
        };

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert(
            "object_storage_path".to_string(),
            DataValue::String(object_storage_path),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

fn required_string(inputs: &HashMap<String, DataValue>, key: &str) -> Result<String> {
    let value = inputs
        .get(key)
        .and_then(|v| match v {
            DataValue::String(s) => Some(s.trim().to_string()),
            _ => None,
        })
        .ok_or_else(|| Error::InvalidNodeInput(format!("{key} is required")))?;

    if value.is_empty() {
        return Err(Error::ValidationError(format!("{key} must not be empty")));
    }

    Ok(value)
}

fn optional_non_empty_string(inputs: &HashMap<String, DataValue>, key: &str) -> Option<String> {
    inputs.get(key).and_then(|v| match v {
        DataValue::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    })
}

fn parse_media_source(value: Option<&str>) -> PersistedMediaSource {
    match value.unwrap_or("upload").trim() {
        "qq_chat" | "qq" => PersistedMediaSource::QqChat,
        "tavily" | "web_search" => PersistedMediaSource::WebSearch,
        _ => PersistedMediaSource::Upload,
    }
}
