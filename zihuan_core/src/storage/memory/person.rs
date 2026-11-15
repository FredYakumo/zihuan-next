use super::*;
use crate::storage::{MemoryAttributes, PersonMemory};

/// A person write has an identity in its profile, with no key, scope or expiry.
#[derive(Debug, Clone)]
pub struct PersonMemoryWrite {
    pub profile: PersonMemory,
    pub access_level: u32,
}

#[derive(Debug, Clone)]
pub struct PersonMemoryRecord {
    pub profile: PersonMemory,
    pub access_level: u32,
    pub object_id: String,
    pub created_at: String,
    pub updated_at: String,
}

impl PersonMemoryRecord {
    pub(crate) fn from_record(record: AgentMemoryRecord) -> Self {
        Self {
            profile: record.attributes.person,
            access_level: record.attributes.access_level,
            object_id: record.object_id,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
    /// Convert at an existing JSON/tool boundary; the domain model has no key.
    pub fn into_record(self) -> AgentMemoryRecord {
        AgentMemoryRecord {
            key: String::new(),
            value: serde_json::to_string(&self.profile).expect("person serializes"),
            attributes: MemoryAttributes {
                category: MemoryCategory::Person,
                access_level: self.access_level,
                person: self.profile,
                ..Default::default()
            },
            object_id: self.object_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            expires_at: None,
            sender_id_list: vec![],
            group_id_list: vec![],
        }
    }
}

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

fn identity(qq_id: &str) -> Result<AgentMemoryUpsert> {
    if qq_id.is_empty() || !qq_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(crate::error::Error::ValidationError(
            "person memory requires a QQ ID".to_string(),
        ));
    }
    Ok(AgentMemoryUpsert {
        attributes: MemoryAttributes {
            category: MemoryCategory::Person,
            person: PersonMemory {
                qq_id: qq_id.to_string(),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    })
}

impl MemoryBackend {
    /// Exact identity read; never falls back to other people or needs an embedding model.
    pub fn get_person(&self, qq_id: &str) -> Result<Option<PersonMemoryRecord>> {
        identity(qq_id)?;
        let record = match self {
            Self::LocalFile(store) => store.get_person_record(qq_id)?,
            Self::RetrievalStore(store) => crate::storage::get_person_in_store(store, qq_id)?,
        };
        Ok(record.map(PersonMemoryRecord::from_record))
    }

    pub fn save_person(
        &self,
        model: Option<&dyn EmbeddingBase>,
        input: &PersonMemoryWrite,
    ) -> Result<PersonMemoryRecord> {
        let mut record = identity(&input.profile.qq_id)?;
        record.attributes.person = input.profile.clone();
        record.attributes.access_level = input.access_level;
        record.value = serde_json::to_string(&input.profile)?;
        self.write_record(model, &record).map(PersonMemoryRecord::from_record)
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
                        crate::storage::memory_is_accessible(&record.clone().into_record(), access)
                    })
                    .take(limit)
                    .map(|record| (record, None))
                    .collect())
            }
            Some(MemoryQuery::Query(text)) if text.trim().is_empty() => {
                return Err(crate::string_error!("query must not be empty"))
            }
            Some(MemoryQuery::Query(text)) => Some(text.as_str()),
            None => None,
        };
        Ok(self
            .query_records(model, access, MemoryCategory::Person, text, limit)?
            .into_iter()
            .take(limit)
            .map(|hit| (PersonMemoryRecord::from_record(hit.record), hit.distance))
            .collect())
    }
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
