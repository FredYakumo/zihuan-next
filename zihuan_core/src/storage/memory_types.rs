use serde::{Deserialize, Serialize};

use crate::retrieval::RetrievalSchema;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    #[default]
    Concept,
    Person,
    Behavioral,
}

impl MemoryCategory {
    pub const ALL: [Self; 3] = [Self::Concept, Self::Person, Self::Behavioral];

    pub fn schema(self) -> RetrievalSchema {
        match self {
            Self::Concept => RetrievalSchema::ConceptMemory,
            Self::Person => RetrievalSchema::PersonMemory,
            Self::Behavioral => RetrievalSchema::BehavioralMemory,
        }
    }
}

/// QQ ID is the identity. Names and aliases are retrieval hints only.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PersonMemory {
    #[serde(default)]
    pub qq_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub preferred_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub impression: String,
    #[serde(default)]
    pub guideline: String,
}

/// Common metadata carried by the three physical memory collections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAttributes {
    #[serde(default)]
    pub category: MemoryCategory,
    #[serde(default = "bot_access_level")]
    pub access_level: u32,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(flatten)]
    pub person: PersonMemory,
}

pub fn bot_access_level() -> u32 {
    2
}

impl Default for MemoryAttributes {
    fn default() -> Self {
        Self {
            category: MemoryCategory::Concept,
            access_level: 2,
            starts_at: None,
            person: PersonMemory::default(),
        }
    }
}

/// Stable scoped identity prevents corrections from creating duplicate facts.
pub fn memory_object_id(input: &super::AgentMemoryUpsert) -> String {
    use sha2::{Digest, Sha256};
    let mut senders = input.sender_id_list.clone();
    senders.sort();
    senders.dedup();
    let mut groups = input.group_id_list.clone();
    groups.sort();
    groups.dedup();
    let bytes = serde_json::to_vec(&(input.attributes.category, input.key.trim(), senders, groups))
        .expect("memory identity serializes");
    let digest = Sha256::digest(bytes);
    uuid::Uuid::from_slice(&digest[..16]).expect("16 bytes").to_string()
}

pub fn normalize_memory_input(
    input: &super::AgentMemoryUpsert,
) -> crate::error::Result<super::AgentMemoryUpsert> {
    use crate::error::Error;
    use chrono::{DateTime, Duration, Utc};
    let mut input = input.clone();
    input.sender_id_list = super::normalize_memory_scope_lists(&input.sender_id_list);
    input.group_id_list = super::normalize_memory_scope_lists(&input.group_id_list);
    match input.attributes.category {
        MemoryCategory::Concept => {
            let starts_at =
                input.attributes.starts_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339());
            input.attributes.starts_at = Some(
                DateTime::parse_from_rfc3339(&starts_at)
                    .map_err(|_| Error::ValidationError("invalid memory starts_at".to_string()))?
                    .with_timezone(&Utc)
                    .to_rfc3339(),
            );
            input.expires_at = None;
            input.attributes.person = PersonMemory::default();
        }
        MemoryCategory::Person => {
            let person = &input.attributes.person;
            if person.qq_id.is_empty() || !person.qq_id.chars().all(|c| c.is_ascii_digit()) {
                return Err(Error::ValidationError("person memory requires a QQ ID".to_string()));
            }
            input.key = format!("person-{}", person.qq_id);
            input.value = serde_json::to_string(person)?;
            input.expires_at = None;
            input.attributes.starts_at = None;
            input.sender_id_list.clear();
            input.group_id_list.clear();
        }
        MemoryCategory::Behavioral => {
            if input.key.trim().is_empty() {
                let mut identity = input.clone();
                identity.key = input.value.clone();
                input.key = format!("behavior-{}", memory_object_id(&identity));
            }
            input.attributes.starts_at = None;
            input.attributes.person = PersonMemory::default();
            let expiry = input
                .expires_at
                .clone()
                .unwrap_or_else(|| (Utc::now() + Duration::days(2)).to_rfc3339());
            input.expires_at = Some(
                DateTime::parse_from_rfc3339(&expiry)
                    .map_err(|_| Error::ValidationError("invalid memory expires_at".to_string()))?
                    .with_timezone(&Utc)
                    .to_rfc3339(),
            );
        }
    }
    Ok(input)
}

/// Serializes permission check + mutation across concurrent conversations in this process.
pub static MEMORY_MUTATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
