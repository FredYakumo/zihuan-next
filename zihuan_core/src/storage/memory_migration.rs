use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

use super::{
    AgentMemoryRecord, AgentMemoryUpsert, MemoryAttributes, MemoryCategory, PersonMemory,
    WeaviateClient,
};
use crate::error::{Error, Result};
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::retrieval::{RetrievalBackend, RetrievalSchema, RetrievalStoreRef};

pub fn converted_legacy_record(record: AgentMemoryRecord) -> AgentMemoryUpsert {
    let mut input = AgentMemoryUpsert {
        attributes: MemoryAttributes {
            starts_at: Some(record.updated_at),
            ..record.attributes
        },
        key: record.key,
        value: record.value,
        expires_at: None,
        sender_id_list: record.sender_id_list,
        group_id_list: record.group_id_list,
    };
    if let Some(qq_id) = input
        .key
        .strip_prefix("dream:")
        .filter(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()))
    {
        input.attributes.category = MemoryCategory::Person;
        input.attributes.starts_at = None;
        input.attributes.person = PersonMemory {
            qq_id: qq_id.to_string(),
            impression: input.value.clone(),
            ..Default::default()
        };
        input.key = format!("person-{qq_id}");
        input.sender_id_list.clear();
        input.group_id_list.clear();
        input.value = serde_json::to_string(&input.attributes.person).expect("person serializes");
    }
    input
}

/// Idempotent: keep the original collection and never overwrite an existing target.
pub fn migrate_retrieval_memory(
    store: &RetrievalStoreRef,
    embedding: &dyn EmbeddingBase,
) -> Result<()> {
    static COMPLETED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let mut completed = COMPLETED
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| crate::string_error!("memory migration lock poisoned"))?;
    if completed.contains(store.connection_id()) {
        return Ok(());
    }
    for category in MemoryCategory::ALL {
        match store.backend() {
            RetrievalBackend::Weaviate => {
                super::ensure_collection_schema(
                    store.weaviate(category.schema())?.as_ref(),
                    category.schema(),
                    true,
                )?;
            }
            RetrievalBackend::Elasticsearch => {
                super::ensure_elasticsearch_index(
                    store.elasticsearch(category.schema())?.as_ref(),
                    true,
                )?;
            }
        }
    }
    let fields = [
        "key",
        "value",
        "expires_at",
        "sender_id_list",
        "group_id_list",
        "created_at",
        "updated_at",
    ]
    .map(str::to_string);
    let mut offset = 0;
    loop {
        let values: Vec<Value> = match store.backend() {
            RetrievalBackend::Weaviate => {
                let reference = store.weaviate(RetrievalSchema::AgentMemory)?;
                if !reference.collection_exists(&reference.class_name)? {
                    break;
                }
                let response = reference.query_with_args(&reference.class_name, &format!("limit: 100, offset: {offset}, sort: [{{path: [\"created_at\"], order: asc}}]"), &fields, true)?;
                response
                    .pointer("/data/Get/AgentMemory")
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or_else(|| {
                        Error::ValidationError(format!(
                            "invalid legacy memory response: {response}"
                        ))
                    })?
            }
            RetrievalBackend::Elasticsearch => {
                let reference = store.elasticsearch(RetrievalSchema::AgentMemory)?;
                let response = match reference.request(
                    reqwest::Method::POST,
                    &format!("/{}/_search", reference.index_name),
                    Some(json!({"size":100,"from":offset,"sort":[{"created_at":"asc"}]})),
                ) {
                    Ok(response) => response,
                    Err(Error::NotFound) => break,
                    Err(error) => return Err(error),
                };
                response
                    .pointer("/hits/hits")
                    .and_then(Value::as_array)
                    .cloned()
                    .ok_or_else(|| {
                        Error::ValidationError("invalid legacy memory hits".to_string())
                    })?
                    .into_iter()
                    .map(|hit| json!({"id":hit["_id"],"properties":hit["_source"]}))
                    .collect()
            }
        };
        let count = values.len();
        for value in values {
            let record =
                super::agent_memory_weaviate::parse_memory_record_with_fallback(value, None)?;
            let input = converted_legacy_record(record);
            if super::get_memory_in_store(store, &input)?.is_none() {
                let vector = embedding.inference(&format!("{}\n{}", input.key, input.value))?;
                super::upsert_memory_in_store(store, &input, vector)?;
            }
        }
        if count < 100 {
            break;
        }
        offset += count;
    }
    completed.insert(store.connection_id().to_string());
    Ok(())
}
