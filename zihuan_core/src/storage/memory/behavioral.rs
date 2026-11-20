use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use super::*;
use crate::storage::{memory_object_id, normalize_memory_scope_lists};

/// The one model for a behavioral rule: a short rule with an expiry that renews on recall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralMemoryRecord {
    pub key: String,
    pub content: String,
    #[serde(default = "default_access_level")]
    pub access_level: u32,
    pub expires_at: String,
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

/// A behavioral write as callers construct it; normalization fills the defaults.
#[derive(Debug, Clone)]
pub struct BehavioralMemoryWrite {
    pub key: String,
    pub content: String,
    pub access_level: u32,
    pub expires_at: Option<String>,
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
}

/// A validated behavioral write with every default filled and its object id derived.
#[derive(Debug, Clone)]
pub struct BehavioralMemoryPrepared {
    pub key: String,
    pub content: String,
    pub access_level: u32,
    pub expires_at: String,
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
    pub object_id: String,
}

impl BehavioralMemoryPrepared {
    /// The text the embedding model vectorizes for this record.
    pub fn embedding_text(&self) -> String {
        format!("{}\n{}", self.key, self.content)
    }
}

/// Validates the write and fills defaults: the key is required identity and never
/// derived, the expiry defaults to two days, and scope lists normalize.
pub fn prepare_behavioral(write: &BehavioralMemoryWrite) -> Result<BehavioralMemoryPrepared> {
    use crate::error::Error;
    let content = write.content.trim();
    if content.is_empty() {
        return Err(Error::ValidationError("behavioral memory requires content".to_string()));
    }
    let key = write.key.trim();
    if key.is_empty() {
        return Err(Error::ValidationError("behavioral memory requires key".to_string()));
    }
    let sender_id_list = normalize_memory_scope_lists(&write.sender_id_list);
    let group_id_list = normalize_memory_scope_lists(&write.group_id_list);
    let key = key.to_string();
    let expires_at = match write.expires_at.as_deref() {
        Some(expires_at) => chrono::DateTime::parse_from_rfc3339(expires_at.trim())
            .map_err(|_| Error::ValidationError("invalid memory expires_at".to_string()))?
            .with_timezone(&Utc)
            .to_rfc3339(),
        None => (Utc::now() + Duration::days(2)).to_rfc3339(),
    };
    let object_id =
        memory_object_id(MemoryCategory::Behavioral, &key, &sender_id_list, &group_id_list);
    Ok(BehavioralMemoryPrepared {
        key,
        content: content.to_string(),
        access_level: write.access_level,
        expires_at,
        sender_id_list,
        group_id_list,
        object_id,
    })
}

impl MemoryBackend {
    /// Exact identity read; the record for this rule title under this scope, or `None`.
    pub fn get_behavioral(
        &self,
        key: &str,
        sender_id_list: &[String],
        group_id_list: &[String],
    ) -> Result<Option<BehavioralMemoryRecord>> {
        match self {
            Self::LocalFile(store) => store.get_behavioral(key, sender_id_list, group_id_list),
            Self::RetrievalStore(store) => {
                super::super::get_behavioral_in_store(store, key, sender_id_list, group_id_list)
            }
        }
    }

    pub fn save_behavioral(
        &self,
        model: Option<&dyn EmbeddingBase>,
        input: &BehavioralMemoryWrite,
    ) -> Result<BehavioralMemoryRecord> {
        let prepared = prepare_behavioral(input)?;
        match self {
            Self::LocalFile(store) => store.save_behavioral(&prepared),
            Self::RetrievalStore(store) => super::super::upsert_behavioral_in_store(
                store,
                &prepared,
                embedding_vector(model, &prepared.embedding_text())?,
            ),
        }
    }

    pub fn search_behavioral(
        &self,
        model: Option<&dyn EmbeddingBase>,
        access: &AgentMemoryAccessContext,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(BehavioralMemoryRecord, Option<f64>)>> {
        let mut hits =
            self.query_records(model, access, MemoryCategory::Behavioral, query, limit)?;
        hits.truncate(limit);
        if !access.skip_expiry_extend {
            let _guard = crate::storage::MEMORY_MUTATION_LOCK
                .lock()
                .map_err(|_| crate::string_error!("memory mutation lock poisoned"))?;
            for hit in &hits {
                let MemoryRecord::Behavioral(record) = &hit.record else {
                    continue;
                };
                if !crate::storage::memory_is_accessible(&hit.record, access) {
                    continue;
                }
                let expiry = chrono::DateTime::parse_from_rfc3339(&record.expires_at)
                    .ok()
                    .map(|expiry| expiry.with_timezone(&Utc));
                let Some(expiry) = expiry else {
                    continue;
                };
                let now = Utc::now();
                let next = (now + (expiry - now) * 2).min(now + Duration::days(365));
                self.save_behavioral(
                    model,
                    &BehavioralMemoryWrite {
                        key: record.key.clone(),
                        content: record.content.clone(),
                        access_level: record.access_level,
                        expires_at: Some(next.to_rfc3339()),
                        sender_id_list: record.sender_id_list.clone(),
                        group_id_list: record.group_id_list.clone(),
                    },
                )?;
            }
        }
        Ok(hits
            .into_iter()
            .filter_map(|hit| match hit.record {
                MemoryRecord::Behavioral(record) => Some((record, hit.distance)),
                _ => None,
            })
            .collect())
    }
}

/// Reads one behavioral record from a backend document.
pub(crate) fn parse_behavioral_record(
    value: serde_json::Value,
    object_id: &str,
) -> Result<BehavioralMemoryRecord> {
    let mut record: BehavioralMemoryRecord = serde_json::from_value(value)?;
    record.object_id = object_id.to_string();
    Ok(record)
}
