//! Dispatch at the category-tagged tool boundary. Domain reads/writes live in storage::memory.
use super::resources::MemoryBackend;
use crate::error::Result;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::storage::{
    AgentMemoryAccessContext, AgentMemoryRecord, AgentMemorySearchHit, AgentMemoryUpsert,
    BehavioralMemoryWrite, ConceptMemoryWrite, MemoryCategory, MemoryQuery, PersonMemoryWrite,
};

pub fn search_category(
    backend: &MemoryBackend,
    model: Option<&dyn EmbeddingBase>,
    access: &AgentMemoryAccessContext,
    category: MemoryCategory,
    query: Option<&MemoryQuery>,
    limit: usize,
) -> Result<Vec<AgentMemorySearchHit>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let hits: Vec<_> = match category {
        MemoryCategory::Person => backend
            .search_people(model, access, query, limit)?
            .into_iter()
            .map(|(record, distance)| AgentMemorySearchHit {
                record: record.into_record(),
                distance,
            })
            .collect(),
        MemoryCategory::Concept => backend
            .search_concept(model, access, text_query(query)?, limit)?
            .into_iter()
            .map(|(record, distance)| AgentMemorySearchHit {
                record: record.into_record(),
                distance,
            })
            .collect(),
        MemoryCategory::Behavioral => backend
            .search_behavioral(model, access, text_query(query)?, limit)?
            .into_iter()
            .map(|(record, distance)| AgentMemorySearchHit {
                record: record.into_record(),
                distance,
            })
            .collect(),
    };
    Ok(hits)
}

/// Legacy script/migration input adapter. Category APIs do not expose this write shape.
pub fn write_memory(
    backend: &MemoryBackend,
    model: Option<&dyn EmbeddingBase>,
    input: &AgentMemoryUpsert,
) -> Result<AgentMemoryRecord> {
    match input.attributes.category {
        MemoryCategory::Person => backend
            .save_person(
                model,
                &PersonMemoryWrite {
                    profile: input.attributes.person.clone(),
                    access_level: input.attributes.access_level,
                },
            )
            .map(|record| record.into_record()),
        MemoryCategory::Concept => backend
            .save_concept(
                model,
                &ConceptMemoryWrite {
                    key: input.key.clone(),
                    content: input.value.clone(),
                    access_level: input.attributes.access_level,
                    starts_at: input.attributes.starts_at.clone(),
                    sender_id_list: input.sender_id_list.clone(),
                    group_id_list: input.group_id_list.clone(),
                },
            )
            .map(|record| record.into_record()),
        MemoryCategory::Behavioral => backend
            .save_behavioral(
                model,
                &BehavioralMemoryWrite {
                    key: input.key.clone(),
                    content: input.value.clone(),
                    access_level: input.attributes.access_level,
                    expires_at: input.expires_at.clone(),
                    sender_id_list: input.sender_id_list.clone(),
                    group_id_list: input.group_id_list.clone(),
                },
            )
            .map(|record| record.into_record()),
    }
}

fn text_query(query: Option<&MemoryQuery>) -> Result<Option<&str>> {
    match query {
        None => Ok(None),
        Some(MemoryQuery::Query(text)) if !text.trim().is_empty() => Ok(Some(text)),
        Some(MemoryQuery::Query(_)) => {
            Err(crate::error::Error::ValidationError("query must not be empty".to_string()))
        }
        Some(MemoryQuery::Id(_)) => Err(crate::error::Error::ValidationError(
            "id query requires person category".to_string(),
        )),
    }
}
