use std::sync::Arc;

use crate::error::{Error, Result};
use crate::ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::storage::weaviate_persistence::upsert_image_record;
use crate::weaviate::WeaviateRef;

pub struct ImagePersistenceRequest<'a> {
    pub object_storage_path: &'a str,
    pub description: &'a str,
    pub embedding_model: Option<&'a dyn EmbeddingBase>,
    pub vector: Option<&'a [f32]>,
    pub source: Option<&'a str>,
    pub media_id: Option<&'a str>,
    pub original_source: Option<&'a str>,
    pub name: Option<&'a str>,
    pub mime_type: Option<&'a str>,
}

pub fn persist_image_record(
    weaviate_ref: &Arc<WeaviateRef>,
    request: ImagePersistenceRequest<'_>,
) -> Result<bool> {
    let object_storage_path = required_string(request.object_storage_path, "object_storage_path")?;
    let description = required_string(request.description, "description")?;
    let description_vector = if let Some(model) = request.embedding_model {
        model.inference(&description)?
    } else if let Some(vector) = request.vector {
        vector.to_vec()
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

    let source = parse_media_source(request.source);
    let name = optional_non_empty_string(request.name);
    let media = match optional_non_empty_string(request.media_id) {
        Some(media_id) => PersistedMedia {
            media_id,
            source,
            original_source: optional_non_empty_string(request.original_source)
                .unwrap_or_else(|| object_storage_path.clone()),
            rustfs_path: object_storage_path.clone(),
            name: name.clone(),
            description: Some(description.clone()),
            mime_type: optional_non_empty_string(request.mime_type),
        },
        None => PersistedMedia::new(
            source,
            optional_non_empty_string(request.original_source)
                .unwrap_or_else(|| object_storage_path.clone()),
            object_storage_path.clone(),
            name.clone(),
            Some(description.clone()),
            optional_non_empty_string(request.mime_type),
        ),
    };
    let name_vector = name
        .as_deref()
        .and_then(|name| request.embedding_model.and_then(|model| model.inference(name).ok()))
        .filter(|vector| !vector.is_empty());

    match upsert_image_record(
        weaviate_ref,
        &media,
        &description_vector,
        name_vector.as_deref(),
    ) {
        Ok(_) => Ok(true),
        Err(error) => {
            log::error!("[image_weaviate_persistence] failed to persist image vector: {error}");
            Ok(false)
        }
    }
}

fn required_string(value: &str, key: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(format!("{key} must not be empty")));
    }
    Ok(value.to_string())
}

fn optional_non_empty_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_media_source(value: Option<&str>) -> PersistedMediaSource {
    match value.unwrap_or("upload").trim() {
        "qq_chat" | "qq" => PersistedMediaSource::QqChat,
        "tavily" | "web_search" => PersistedMediaSource::WebSearch,
        _ => PersistedMediaSource::Upload,
    }
}
