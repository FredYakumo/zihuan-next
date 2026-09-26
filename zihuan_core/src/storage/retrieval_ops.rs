use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::ims_bot_adapter::models::message::{Message, PersistedMedia, PersistedMediaSource};
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::retrieval::{RetrievalBackend, RetrievalSchema, RetrievalStoreRef};
use crate::storage::{
    search_elasticsearch_images, search_elasticsearch_memory, search_images,
    search_memory_content_by_vector, upsert_elasticsearch_image, upsert_image_record,
    AgentMemoryAccessContext, AgentMemorySearchHit,
};

/// One image-persistence request, independent of the retrieval backend.
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

/// Persists one already-built media record into the store.
pub fn persist_media_to_store(
    store: &RetrievalStoreRef,
    media: &PersistedMedia,
    description_vector: &[f32],
    name_vector: Option<&[f32]>,
) -> Result<bool> {
    if description_vector.is_empty() {
        return Err(Error::ValidationError("description_vector must not be empty".to_string()));
    }
    match store.backend() {
        RetrievalBackend::Weaviate => {
            let reference = store.weaviate(RetrievalSchema::ImageSemantic)?;
            upsert_image_record(&reference, media, description_vector, name_vector)?;
            Ok(true)
        }
        RetrievalBackend::Elasticsearch => {
            let reference = store.elasticsearch(RetrievalSchema::ImageSemantic)?;
            upsert_elasticsearch_image(
                &reference,
                media,
                description_vector.to_vec(),
                name_vector.map(<[f32]>::to_vec),
            )?;
            Ok(true)
        }
    }
}

/// Persists one image record into the store described by `request`.
pub fn persist_image_record_to_store(
    store: &RetrievalStoreRef,
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
        return Err(Error::ValidationError("description_vector must not be empty".to_string()));
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

    match store.backend() {
        RetrievalBackend::Weaviate => {
            let reference = store.weaviate(RetrievalSchema::ImageSemantic)?;
            match upsert_image_record(
                &reference,
                &media,
                &description_vector,
                name_vector.as_deref(),
            ) {
                Ok(_) => Ok(true),
                Err(error) => {
                    log::error!("[retrieval] failed to persist image vector: {error}");
                    Ok(false)
                }
            }
        }
        RetrievalBackend::Elasticsearch => {
            let reference = store.elasticsearch(RetrievalSchema::ImageSemantic)?;
            match upsert_elasticsearch_image(&reference, &media, description_vector, name_vector) {
                Ok(_) => Ok(true),
                Err(error) => {
                    log::error!("[retrieval] failed to persist image vector: {error}");
                    Ok(false)
                }
            }
        }
    }
}

/// Persists one QQ message list into the store, dispatching on the backend.
///
/// Only Weaviate currently stores QQ message vectors; Elasticsearch returns an
/// explicit error rather than silently dropping the write.
#[allow(clippy::too_many_arguments)]
pub fn persist_qq_message_list_to_store(
    store: &RetrievalStoreRef,
    messages: &[Message],
    message_id: &str,
    sender_id: &str,
    sender_name: &str,
    group_id: Option<&str>,
    group_name: Option<&str>,
    embedding_model: &dyn EmbeddingBase,
) -> Result<bool> {
    match store.backend() {
        RetrievalBackend::Weaviate => {
            let reference = store.weaviate(RetrievalSchema::QqMessage)?;
            crate::storage::persist_qq_message_list(
                &reference,
                embedding_model,
                messages,
                message_id,
                sender_id,
                sender_name,
                group_id,
                group_name,
            )
        }
        RetrievalBackend::Elasticsearch => Err(Error::ValidationError(
            "QQ message vector persistence is not supported by the elasticsearch retrieval backend"
                .to_string(),
        )),
    }
}

/// Runs a semantic image search against the store, normalized to a `{images, has_results}` payload.
pub fn search_images_in_store(
    store: &RetrievalStoreRef,
    embedding_model: &dyn EmbeddingBase,
    query: &str,
    limit: usize,
    max_distance: Option<f64>,
    target_vector: Option<&str>,
) -> Result<Value> {
    let query = query.trim();
    if query.is_empty() {
        return Err(Error::ValidationError("query is required".to_string()));
    }
    if limit == 0 {
        return Err(Error::ValidationError("limit must be greater than 0".to_string()));
    }
    let images = match store.backend() {
        RetrievalBackend::Weaviate => {
            let reference = store.weaviate(RetrievalSchema::ImageSemantic)?;
            search_images(&reference, embedding_model, query, limit, max_distance, target_vector)?
        }
        RetrievalBackend::Elasticsearch => {
            let reference = store.elasticsearch(RetrievalSchema::ImageSemantic)?;
            let vector = embedding_model.inference(query)?;
            search_elasticsearch_images(&reference, None, Some(query), None, Some(&vector), limit)?
                .into_iter()
                .map(|hit| normalized_elasticsearch_image(hit.properties))
                .collect()
        }
    };
    Ok(json!({"images": images, "has_results": !images.is_empty()}))
}

/// Searches agent memory in the store, returning normalized hits.
pub fn search_memory_in_store(
    store: &RetrievalStoreRef,
    access: &AgentMemoryAccessContext,
    query: &str,
    query_vector: &[f32],
    limit: usize,
) -> Result<Vec<AgentMemorySearchHit>> {
    match store.backend() {
        RetrievalBackend::Weaviate => {
            let reference = store.weaviate(RetrievalSchema::AgentMemory)?;
            search_memory_content_by_vector(&reference, access, query_vector, limit)
        }
        RetrievalBackend::Elasticsearch => {
            let reference = store.elasticsearch(RetrievalSchema::AgentMemory)?;
            search_elasticsearch_memory(&reference, access, query, query_vector, limit)
        }
    }
}

/// Lists recent agent memory records when no query is provided.
pub fn list_memory_in_store(
    store: &RetrievalStoreRef,
    access: &AgentMemoryAccessContext,
    limit: usize,
) -> Result<Vec<AgentMemorySearchHit>> {
    match store.backend() {
        RetrievalBackend::Weaviate => {
            let reference = store.weaviate(RetrievalSchema::AgentMemory)?;
            crate::storage::list_recent_memory_keys(&reference, access, limit, None)
        }
        RetrievalBackend::Elasticsearch => {
            let reference = store.elasticsearch(RetrievalSchema::AgentMemory)?;
            crate::storage::list_elasticsearch_memory_keys(&reference, access, limit, None)
        }
    }
}

/// Creates or updates one agent memory record in the store.
pub fn upsert_memory_in_store(
    store: &RetrievalStoreRef,
    input: &crate::storage::AgentMemoryUpsert,
    vector: Vec<f32>,
) -> Result<crate::storage::AgentMemoryRecord> {
    match store.backend() {
        RetrievalBackend::Weaviate => {
            let reference = store.weaviate(RetrievalSchema::AgentMemory)?;
            crate::storage::create_memory_record_with_vector(&reference, input, Some(vector))
        }
        RetrievalBackend::Elasticsearch => {
            let reference = store.elasticsearch(RetrievalSchema::AgentMemory)?;
            crate::storage::create_elasticsearch_memory_record(&reference, input, vector)
        }
    }
}

fn normalized_elasticsearch_image(properties: Value) -> Value {
    let mut object = serde_json::Map::new();
    for key in ["media_id", "original_source", "name", "description", "mime_type", "rustfs_path"] {
        object.insert(key.to_string(), properties.get(key).cloned().unwrap_or(Value::Null));
    }
    object.insert(
        "source".to_string(),
        properties
            .get("source")
            .cloned()
            .unwrap_or_else(|| Value::String("elasticsearch".to_string())),
    );
    Value::Object(object)
}

fn required_string(value: &str, key: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(format!("{key} must not be empty")));
    }
    Ok(value.to_string())
}

fn optional_non_empty_string(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned)
}

fn parse_media_source(value: Option<&str>) -> PersistedMediaSource {
    match value.unwrap_or("upload").trim() {
        "qq_chat" | "qq" => PersistedMediaSource::QqChat,
        "tavily" | "web_search" => PersistedMediaSource::WebSearch,
        _ => PersistedMediaSource::Upload,
    }
}
