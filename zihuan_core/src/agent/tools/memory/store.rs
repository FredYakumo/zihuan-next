//! Backend read/write primitives shared by the memory tools.
//!
//! Each backend (local files or the unified retrieval store) stores the same three memory
//! categories, so the tools never branch on the backend themselves.

use chrono::{Duration, Utc};

use crate::error::Result;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::storage::{
    get_memory_in_store, list_memory_in_store, memory_is_accessible, search_memory_in_store,
    upsert_memory_in_store, AgentMemoryAccessContext, AgentMemoryRecord, AgentMemorySearchHit,
    AgentMemoryUpsert, MemoryAttributes, MemoryCategory,
};

use super::resources::MemoryBackend;

/// Searches one category of agent memory.
///
/// A numeric `query` on the person category is treated as a QQ ID and resolved as an exact
/// identity lookup before any fuzzy search: people are identified by QQ ID, so an ID match is
/// authoritative. Behavioral records are readable across the caller's scopes and keep their
/// expiry extended on a real (non-read-only) search.
pub fn search_category(
    backend: &MemoryBackend,
    embedding_model: Option<&dyn EmbeddingBase>,
    access_context: &AgentMemoryAccessContext,
    category: MemoryCategory,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<AgentMemorySearchHit>> {
    let access = AgentMemoryAccessContext {
        category,
        skip_expiry_extend: true,
        ..access_context.clone()
    };
    if category == MemoryCategory::Person
        && query.is_some_and(|query| !query.is_empty() && query.chars().all(|c| c.is_ascii_digit()))
    {
        let identity = AgentMemoryUpsert {
            attributes: MemoryAttributes { category, ..Default::default() },
            key: format!("person-{}", query.unwrap()),
            ..Default::default()
        };
        let record = match backend {
            MemoryBackend::LocalFile(store) => store.get(&identity)?,
            MemoryBackend::RetrievalStore(store) => get_memory_in_store(store, &identity)?,
        };
        if let Some(record) = record.filter(|record| memory_is_accessible(record, &access)) {
            return Ok(vec![AgentMemorySearchHit { record, distance: None }]);
        }
    }

    let mut hits = match backend {
        MemoryBackend::LocalFile(store) => store.list_category(category, query, 100)?,
        MemoryBackend::RetrievalStore(store) => match query {
            Some(query) => search_memory_in_store(
                store,
                &access,
                query,
                &embedding_vector(embedding_model, query)?,
                limit.max(20),
            )?,
            None => list_memory_in_store(store, &access, 100)?,
        },
    };
    hits.retain(|hit| memory_is_accessible(&hit.record, &access));
    if category == MemoryCategory::Concept {
        hits.sort_by(|a, b| {
            b.record
                .attributes
                .starts_at
                .as_ref()
                .unwrap_or(&b.record.updated_at)
                .cmp(a.record.attributes.starts_at.as_ref().unwrap_or(&a.record.updated_at))
        });
    }
    hits.truncate(limit);
    if !access_context.skip_expiry_extend && category == MemoryCategory::Behavioral {
        let _guard = crate::storage::MEMORY_MUTATION_LOCK
            .lock()
            .map_err(|_| crate::string_error!("memory mutation lock poisoned"))?;
        for hit in &hits {
            let identity = AgentMemoryUpsert {
                attributes: hit.record.attributes.clone(),
                key: hit.record.key.clone(),
                sender_id_list: hit.record.sender_id_list.clone(),
                group_id_list: hit.record.group_id_list.clone(),
                ..Default::default()
            };
            let latest = match backend {
                MemoryBackend::LocalFile(store) => store.get(&identity)?,
                MemoryBackend::RetrievalStore(store) => get_memory_in_store(store, &identity)?,
            };
            let Some(record) = latest.filter(|record| memory_is_accessible(record, &access)) else {
                continue;
            };
            if let Some(expiry) = record
                .expires_at
                .as_ref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            {
                let now = Utc::now();
                let next =
                    (now + (expiry.with_timezone(&Utc) - now) * 2).min(now + Duration::days(365));
                let input = AgentMemoryUpsert {
                    attributes: record.attributes,
                    key: record.key,
                    value: record.value,
                    expires_at: Some(next.to_rfc3339()),
                    sender_id_list: record.sender_id_list,
                    group_id_list: record.group_id_list,
                };
                write_memory(backend, embedding_model, &input)?;
            }
        }
    }
    Ok(hits)
}

/// Creates or corrects one memory record.
///
/// A retrieval-store write also refreshes the record's embedding from its key, content and
/// person profile, so a corrected memory is searchable by its new text.
pub fn write_memory(
    backend: &MemoryBackend,
    embedding_model: Option<&dyn EmbeddingBase>,
    input: &AgentMemoryUpsert,
) -> Result<AgentMemoryRecord> {
    match backend {
        MemoryBackend::LocalFile(store) => store.create_or_update(input),
        MemoryBackend::RetrievalStore(store) => upsert_memory_in_store(
            store,
            input,
            embedding_vector(
                embedding_model,
                &format!(
                    "{}\n{}\n{}",
                    input.key,
                    input.value,
                    serde_json::to_string(&input.attributes.person)?
                ),
            )?,
        ),
    }
}

pub(crate) fn embedding_vector(
    embedding_model: Option<&dyn EmbeddingBase>,
    text: &str,
) -> Result<Vec<f32>> {
    embedding_model
        .ok_or_else(|| {
            crate::error::Error::ValidationError(
                "memory backend requires an embedding model".to_string(),
            )
        })?
        .inference(text)
}
