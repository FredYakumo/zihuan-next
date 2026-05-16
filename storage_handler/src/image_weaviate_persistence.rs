use crate::weaviate_persistence::upsert_image_record;
use std::collections::HashMap;
use zihuan_core::error::{Error, Result};
use zihuan_core::ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};

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
        Some("Image vector persistence - writes object storage path + vector + summary to Weaviate")
    }

    node_input![
        port! { name = "object_storage_path", ty = String, desc = "Object storage path (object_key/object_url)" },
        port! { name = "description", ty = String, desc = "Image summary description" },
        port! { name = "weaviate_ref", ty = zihuan_core::weaviate::WeaviateRef, desc = "Weaviate connection reference" },
        port! { name = "embedding_model", ty = zihuan_core::llm::embedding_base::EmbeddingModel, desc = "Embedding model for generating name and description vectors", optional },
        port! { name = "vector", ty = Vector, desc = "Image semantic vector (deprecated, prefer embedding_model)", optional },
        port! { name = "source", ty = String, desc = "Optional: image source (qq/tavily/etc)", optional },
        port! { name = "media_id", ty = String, desc = "Optional: persisted media ID", optional },
        port! { name = "original_source", ty = String, desc = "Optional: original source string", optional },
        port! { name = "name", ty = String, desc = "Optional: media name", optional },
        port! { name = "mime_type", ty = String, desc = "Optional: media MIME type", optional },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "Whether storage succeeded" },
        port! { name = "object_storage_path", ty = String, desc = "Pass-through object storage path" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let object_storage_path = required_string(&inputs, "object_storage_path")?;
        let description = required_string(&inputs, "description")?;

        let weaviate_ref = inputs
            .get("weaviate_ref")
            .and_then(|v| match v {
                DataValue::WeaviateRef(r) => Some(r.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("weaviate_ref is required".to_string()))?;

        let embedding_model = inputs.get("embedding_model").and_then(|v| match v {
            DataValue::EmbeddingModel(m) => Some(m.clone()),
            _ => None,
        });

        let description_vector = if let Some(model) = embedding_model.as_ref() {
            model.inference(&description)?
        } else if let Some(DataValue::Vector(v)) = inputs.get("vector") {
            v.clone()
        } else {
            return Err(Error::InvalidNodeInput(
                "either embedding_model or vector is required".to_string(),
            ));
        };

        if description_vector.is_empty() {
            return Err(Error::ValidationError(
                "description_vector must not be empty".to_string(),
            ));
        }

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
                name: name.clone(),
                description: Some(description.clone()),
                mime_type,
            }
        } else {
            PersistedMedia::new(
                source,
                original_source,
                object_storage_path.clone(),
                name.clone(),
                Some(description.clone()),
                mime_type,
            )
        };

        let name_vector = name
            .as_ref()
            .and_then(|n| embedding_model.as_ref()?.inference(n).ok())
            .filter(|v| !v.is_empty());

        let success = match upsert_image_record(
            &weaviate_ref,
            &media,
            &description_vector,
            name_vector.as_deref(),
        ) {
            Ok(_) => true,
            Err(err) => {
                log::error!(
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
