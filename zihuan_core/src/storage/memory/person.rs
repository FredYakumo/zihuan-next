use serde::{Deserialize, Serialize};

use super::*;
use crate::storage::PersonMemory;

/// A person write has an identity in its profile, with no key, scope or expiry.
#[derive(Debug, Clone)]
pub struct PersonMemoryWrite {
    pub profile: PersonMemory,
    pub access_level: u32,
}

/// The one model for a person: the profile is the storage document and the wire shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonMemoryRecord {
    #[serde(flatten)]
    pub profile: PersonMemory,
    #[serde(default = "default_access_level")]
    pub access_level: u32,
    pub object_id: String,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) use super::concept::default_access_level;

pub fn person_search_text(person: &PersonMemory) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}",
        person.qq_id,
        person.name,
        person.preferred_name,
        person.aliases.join(" "),
        person.impression
    )
}

fn validate_profile(profile: &PersonMemory) -> Result<()> {
    if profile.qq_id.is_empty() || !profile.qq_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(crate::error::Error::ValidationError(
            "person memory requires a QQ ID".to_string(),
        ));
    }
    Ok(())
}

/// Validates the profile. Unlike the other categories there are no defaults to fill:
/// the QQ ID is the identity and the profile is the whole record.
pub fn prepare_person(write: &PersonMemoryWrite) -> Result<PersonMemoryWrite> {
    validate_profile(&write.profile)?;
    Ok(write.clone())
}

impl MemoryBackend {
    /// Exact identity read; never falls back to other people or needs an embedding model.
    pub fn get_person(&self, qq_id: &str) -> Result<Option<PersonMemoryRecord>> {
        validate_profile(&PersonMemory {
            qq_id: qq_id.to_string(),
            ..Default::default()
        })?;
        match self {
            Self::LocalFile(store) => store.get_person_record(qq_id),
            Self::RetrievalStore(store) => super::super::get_person_in_store(store, qq_id),
        }
    }

    pub fn save_person(
        &self,
        model: Option<&dyn EmbeddingBase>,
        input: &PersonMemoryWrite,
    ) -> Result<PersonMemoryRecord> {
        let prepared = prepare_person(input)?;
        match self {
            Self::LocalFile(store) => store.save_person(&prepared),
            Self::RetrievalStore(store) => super::super::upsert_person_in_store(
                store,
                &prepared,
                embedding_vector(model, &person_search_text(&prepared.profile))?,
            ),
        }
    }

    /// Text/semantic search is explicitly separate from exact QQ identity lookup.
    pub fn search_people(
        &self,
        model: Option<&dyn EmbeddingBase>,
        access: &AgentMemoryAccessContext,
        query: Option<&MemoryQuery>,
        limit: usize,
    ) -> Result<Vec<(PersonMemoryRecord, Option<f64>)>> {
        let text = match query {
            Some(MemoryQuery::Id(id)) => {
                return Ok(self
                    .get_person(id)?
                    .into_iter()
                    .filter(|record| {
                        crate::storage::memory_is_accessible(
                            &MemoryRecord::Person(record.clone()),
                            access,
                        )
                    })
                    .take(limit)
                    .map(|record| (record, None))
                    .collect())
            }
            Some(MemoryQuery::Query(text)) if text.trim().is_empty() => {
                return Err(crate::error::Error::ValidationError(
                    "query must not be empty".to_string(),
                ))
            }
            Some(MemoryQuery::Query(text)) => Some(text.as_str()),
            None => None,
        };
        Ok(self
            .query_records(model, access, MemoryCategory::Person, text, limit)?
            .into_iter()
            .take(limit)
            .filter_map(|hit| match hit.record {
                MemoryRecord::Person(record) => Some((record, hit.distance)),
                _ => None,
            })
            .collect())
    }
}

/// Reads one person record from a backend document.
pub(crate) fn parse_person_record(
    value: serde_json::Value,
    object_id: &str,
) -> Result<PersonMemoryRecord> {
    let mut record: PersonMemoryRecord = serde_json::from_value(value)?;
    record.object_id = object_id.to_string();
    Ok(record)
}

/// Explicit query mode. Numeric fuzzy text stays fuzzy; identity is never inferred.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryQuery {
    Id(String),
    Query(String),
}

pub fn memory_query_schema() -> serde_json::Value {
    serde_json::json!({"oneOf":[
        {"type":"object","properties":{"id":{"type":"string","minLength":1}},"required":["id"],"additionalProperties":false},
        {"type":"object","properties":{"query":{"type":"string","minLength":1}},"required":["query"],"additionalProperties":false}
    ]})
}
