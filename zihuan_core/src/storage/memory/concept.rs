use super::*;
use crate::storage::MemoryAttributes;

#[derive(Debug, Clone)]
pub struct ConceptMemoryWrite {
    pub key: String,
    pub content: String,
    pub access_level: u32,
    pub starts_at: Option<String>,
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ConceptMemoryRecord {
    pub key: String,
    pub content: String,
    pub access_level: u32,
    pub starts_at: Option<String>,
    pub sender_id_list: Vec<String>,
    pub group_id_list: Vec<String>,
    pub object_id: String,
    pub created_at: String,
    pub updated_at: String,
}

impl ConceptMemoryRecord {
    pub(crate) fn from_record(record: AgentMemoryRecord) -> Self {
        Self {
            key: record.key,
            content: record.value,
            access_level: record.attributes.access_level,
            starts_at: record.attributes.starts_at,
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
                category: MemoryCategory::Concept,
                access_level: self.access_level,
                starts_at: self.starts_at,
                ..Default::default()
            },
            expires_at: None,
            sender_id_list: self.sender_id_list,
            group_id_list: self.group_id_list,
            object_id: self.object_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl MemoryBackend {
    pub fn get_concept(
        &self,
        key: &str,
        sender_id_list: &[String],
        group_id_list: &[String],
    ) -> Result<Option<ConceptMemoryRecord>> {
        let identity = AgentMemoryUpsert {
            attributes: MemoryAttributes {
                category: MemoryCategory::Concept,
                ..Default::default()
            },
            key: key.to_string(),
            sender_id_list: sender_id_list.to_vec(),
            group_id_list: group_id_list.to_vec(),
            ..Default::default()
        };
        Ok(self.read_record(&identity)?.map(ConceptMemoryRecord::from_record))
    }

    pub fn save_concept(
        &self,
        model: Option<&dyn EmbeddingBase>,
        input: &ConceptMemoryWrite,
    ) -> Result<ConceptMemoryRecord> {
        let record = AgentMemoryUpsert {
            attributes: MemoryAttributes {
                category: MemoryCategory::Concept,
                access_level: input.access_level,
                starts_at: input.starts_at.clone(),
                ..Default::default()
            },
            key: input.key.clone(),
            value: input.content.clone(),
            expires_at: None,
            sender_id_list: input.sender_id_list.clone(),
            group_id_list: input.group_id_list.clone(),
        };
        self.write_record(model, &record).map(ConceptMemoryRecord::from_record)
    }

    pub fn search_concept(
        &self,
        model: Option<&dyn EmbeddingBase>,
        access: &AgentMemoryAccessContext,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<(ConceptMemoryRecord, Option<f64>)>> {
        let mut hits = self.query_records(model, access, MemoryCategory::Concept, query, limit)?;
        hits.sort_by(|a, b| {
            b.record
                .attributes
                .starts_at
                .as_ref()
                .unwrap_or(&b.record.updated_at)
                .cmp(a.record.attributes.starts_at.as_ref().unwrap_or(&a.record.updated_at))
        });
        hits.truncate(limit);
        Ok(hits
            .into_iter()
            .map(|hit| (ConceptMemoryRecord::from_record(hit.record), hit.distance))
            .collect())
    }
}
