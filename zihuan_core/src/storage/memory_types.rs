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
#[serde(default)]
pub struct PersonMemory {
    pub qq_id: String,
    pub name: String,
    pub preferred_name: String,
    pub aliases: Vec<String>,
    pub impression: String,
    pub guideline: String,
}

/// The identity input of one memory record: its category plus the field that names it
/// (a concept/behavioral title or a person QQ ID). Scope lists are normalized inside,
/// so records derived from equal identity inputs always share one object id.
pub fn memory_object_id(
    category: MemoryCategory,
    identity: &str,
    sender_id_list: &[String],
    group_id_list: &[String],
) -> String {
    use sha2::{Digest, Sha256};
    let mut senders = super::agent_memory_weaviate::normalize_memory_scope_lists(sender_id_list);
    senders.sort();
    senders.dedup();
    let mut groups = super::agent_memory_weaviate::normalize_memory_scope_lists(group_id_list);
    groups.sort();
    groups.dedup();
    let bytes = serde_json::to_vec(&(category, identity.trim(), senders, groups))
        .expect("memory identity serializes");
    let digest = Sha256::digest(bytes);
    uuid::Uuid::from_slice(&digest[..16]).expect("16 bytes").to_string()
}

/// Serializes permission check + mutation across concurrent conversations in this process.
pub static MEMORY_MUTATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
