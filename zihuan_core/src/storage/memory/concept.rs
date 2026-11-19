use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::*;
use crate::storage::{memory_object_id, normalize_memory_scope_lists};

/// A fact/concept memory: a titled record with an effective time and no expiry.
///
/// This is the one model for the category — the domain record, the storage document
/// and the wire shape are the same struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptMemoryRecord {
    pub key: String,
    pub content: String,
    #[serde(default = "default_access_level")]
    pub access_level: u32,
    pub starts_at: String,
    #[serde(default)]
    pub sender_id_list: Vec<String>,
    #[serde(default)]
    pub group_id_list: Vec<String>,
    /// The backend object id, filled in after a document read; it is stored as the
    /// backend envelope id (Weaviate UUID / ES `_id`), not inside the document.
    #[serde(default)]
    pub object_id: String,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) fn default_access_level() -> u32 {
    crate::storage::user_permission::ROLE_SELF_ACCESS_LEVEL
}

/// A concept write as callers construct it; normalization fills the defaults.
#[derive(Debug, Clone)]
pub struct ConceptMemoryWrite {
    pub key: String,
    pub content: String,
    pub access_level: u32,
    pub starts_at: Option<String>,
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
}

/// A validated concept write with every default filled and its object id derived.
#[derive(Debug, Clone)]
pub struct ConceptMemoryPrepared {
    pub key: String,
    pub content: String,
    pub access_level: u32,
    pub starts_at: String,
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
    pub object_id: String,
}

impl ConceptMemoryPrepared {
    /// The text the embedding model vectorizes for this record.
    pub fn embedding_text(&self) -> String {
        format!("{}\n{}", self.key, self.content)
    }
}

/// Validates the write and fills defaults: trims title/content, normalizes scope lists,
/// and defaults `starts_at` to now. The object id is derived from the scoped identity,
/// so correcting a fact rewrites its record instead of creating a duplicate.
pub fn prepare_concept(write: &ConceptMemoryWrite) -> Result<ConceptMemoryPrepared> {
    use crate::error::Error;
    let key = write.key.trim();
    let content = write.content.trim();
    if key.is_empty() || content.is_empty() {
        return Err(crate::error::Error::ValidationError(
            "concept memory requires a title and content".to_string(),
        ));
    }
    let starts_at = match write.starts_at.as_deref() {
        Some(starts_at) => chrono::DateTime::parse_from_rfc3339(starts_at.trim())
            .map_err(|_| Error::ValidationError("invalid memory starts_at".to_string()))?
            .with_timezone(&chrono::Utc)
            .to_rfc3339(),
        None => Utc::now().to_rfc3339(),
    };
    let sender_id_list = normalize_memory_scope_lists(&write.sender_id_list);
    let group_id_list = normalize_memory_scope_lists(&write.group_id_list);
    let object_id = memory_object_id(MemoryCategory::Concept, key, &sender_id_list, &group_id_list);
    Ok(ConceptMemoryPrepared {
        key: key.to_string(),
        content: content.to_string(),
        access_level: write.access_level,
        starts_at,
        sender_id_list,
        group_id_list,
        object_id,
    })
}

impl MemoryBackend {
    /// Exact identity read; the record for this title under this scope, or `None`.
    pub fn get_concept(
        &self,
        key: &str,
        sender_id_list: &[String],
        group_id_list: &[String],
    ) -> Result<Option<ConceptMemoryRecord>> {
        match self {
            Self::LocalFile(store) => store.get_concept(key, sender_id_list, group_id_list),
            Self::RetrievalStore(store) => {
                super::super::get_concept_in_store(store, key, sender_id_list, group_id_list)
            }
        }
    }

    pub fn save_concept(
        &self,
        model: Option<&dyn EmbeddingBase>,
        input: &ConceptMemoryWrite,
    ) -> Result<ConceptMemoryRecord> {
        let prepared = prepare_concept(input)?;
        match self {
            Self::LocalFile(store) => store.save_concept(&prepared),
            Self::RetrievalStore(store) => super::super::upsert_concept_in_store(
                store,
                &prepared,
                embedding_vector(model, &prepared.embedding_text())?,
            ),
        }
    }

    pub fn search_concept(
        &self,
        model: Option<&dyn EmbeddingBase>,
        access: &AgentMemoryAccessContext,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(ConceptMemoryRecord, Option<f64>)>> {
        let mut hits = self.query_records(model, access, MemoryCategory::Concept, query, limit)?;
        if query.is_none() {
            hits.sort_by(|a, b| match (&b.record, &a.record) {
                (MemoryRecord::Concept(right), MemoryRecord::Concept(left)) => {
                    right.starts_at.cmp(&left.starts_at)
                }
                _ => std::cmp::Ordering::Equal,
            });
        }
        hits.truncate(limit);
        Ok(hits
            .into_iter()
            .filter_map(|hit| match hit.record {
                MemoryRecord::Concept(record) => Some((record, hit.distance)),
                _ => None,
            })
            .collect())
    }
}

/// Reads one concept record from a backend document.
pub(crate) fn parse_concept_record(
    value: serde_json::Value,
    object_id: &str,
) -> Result<ConceptMemoryRecord> {
    let mut record: ConceptMemoryRecord = serde_json::from_value(value)?;
    record.object_id = object_id.to_string();
    Ok(record)
}
