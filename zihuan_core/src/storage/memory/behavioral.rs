use super::*;
use crate::storage::MemoryAttributes;
use chrono::{Duration, Utc};

#[derive(Debug, Clone)]
pub struct BehavioralMemoryWrite {
    pub key: String,
    pub content: String,
    pub access_level: u32,
    pub expires_at: Option<String>,
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BehavioralMemoryRecord {
    pub key: String,
    pub content: String,
    pub access_level: u32,
    pub expires_at: Option<String>,
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
    pub object_id: String,
    pub created_at: String,
    pub updated_at: String,
}

impl BehavioralMemoryRecord {
    pub(crate) fn from_record(record: AgentMemoryRecord) -> Self {
        Self {
            key: record.key,
            content: record.value,
            access_level: record.attributes.access_level,
            expires_at: record.expires_at,
            sender_id_list: record.sender_id_list,
            group_id_list: record.group_id_list,
            object_id: record.object_id,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
    pub fn into_record(self) -> AgentMemoryRecord {
        AgentMemoryRecord {
            key: self.key,
            value: self.content,
            attributes: MemoryAttributes {
                category: MemoryCategory::Behavioral,
                access_level: self.access_level,
                ..Default::default()
            },
            expires_at: self.expires_at,
            sender_id_list: self.sender_id_list,
            group_id_list: self.group_id_list,
            object_id: self.object_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl MemoryBackend {
    pub fn get_behavioral(
        &self,
        key: &str,
        sender_id_list: &[String],
        group_id_list: &[String],
    ) -> Result<Option<BehavioralMemoryRecord>> {
        let identity = AgentMemoryUpsert {
            attributes: MemoryAttributes {
                category: MemoryCategory::Behavioral,
                ..Default::default()
            },
            key: key.to_string(),
            sender_id_list: sender_id_list.to_vec(),
            group_id_list: group_id_list.to_vec(),
            ..Default::default()
        };
        Ok(self.read_record(&identity)?.map(BehavioralMemoryRecord::from_record))
    }

    pub fn save_behavioral(
        &self,
        model: Option<&dyn EmbeddingBase>,
        input: &BehavioralMemoryWrite,
    ) -> Result<BehavioralMemoryRecord> {
        let record = AgentMemoryUpsert {
            attributes: MemoryAttributes {
                category: MemoryCategory::Behavioral,
                access_level: input.access_level,
                ..Default::default()
            },
            key: input.key.clone(),
            value: input.content.clone(),
            expires_at: input.expires_at.clone(),
            sender_id_list: input.sender_id_list.clone(),
            group_id_list: input.group_id_list.clone(),
        };
        self.write_record(model, &record).map(BehavioralMemoryRecord::from_record)
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
                let Some(record) = self.get_behavioral(
                    &hit.record.key,
                    &hit.record.sender_id_list,
                    &hit.record.group_id_list,
                )?
                else {
                    continue;
                };
                if !crate::storage::memory_is_accessible(&record.clone().into_record(), access) {
                    continue;
                }
                if let Some(expiry) = record
                    .expires_at
                    .as_ref()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                {
                    let now = Utc::now();
                    let next = (now + (expiry.with_timezone(&Utc) - now) * 2)
                        .min(now + Duration::days(365));
                    self.save_behavioral(
                        model,
                        &BehavioralMemoryWrite {
                            key: record.key,
                            content: record.content,
                            access_level: record.access_level,
                            expires_at: Some(next.to_rfc3339()),
                            sender_id_list: record.sender_id_list,
                            group_id_list: record.group_id_list,
                        },
                    )?;
                }
            }
        }
        Ok(hits
            .into_iter()
            .map(|hit| (BehavioralMemoryRecord::from_record(hit.record), hit.distance))
            .collect())
    }
}

/// Stable identity for a new behavioral rule when the caller has no existing key.
pub fn behavioral_key(
    content: &str,
    sender_id_list: &[String],
    group_id_list: &[String],
) -> String {
    let identity = AgentMemoryUpsert {
        attributes: MemoryAttributes {
            category: MemoryCategory::Behavioral,
            ..Default::default()
        },
        key: content.to_string(),
        sender_id_list: sender_id_list.to_vec(),
        group_id_list: group_id_list.to_vec(),
        ..Default::default()
    };
    format!("behavior-{}", crate::storage::memory_object_id(&identity))
}
