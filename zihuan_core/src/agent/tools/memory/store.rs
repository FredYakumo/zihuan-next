//! Dispatch at the category-tagged tool boundary. Domain reads/writes live in storage::memory.
use super::resources::MemoryBackend;
use crate::error::Result;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::storage::{AgentMemoryAccessContext, MemoryCategory, MemoryQuery, MemorySearchHit};

pub fn search_category(
    backend: &MemoryBackend,
    model: Option<&dyn EmbeddingBase>,
    access: &AgentMemoryAccessContext,
    category: MemoryCategory,
    query: Option<&MemoryQuery>,
    limit: usize,
) -> Result<Vec<MemorySearchHit>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let hits: Vec<MemorySearchHit> = match category {
        MemoryCategory::Person => backend
            .search_people(model, access, query, limit)?
            .into_iter()
            .map(|(record, distance)| MemorySearchHit::new(record, distance))
            .collect(),
        MemoryCategory::Concept => backend
            .search_concept(model, access, text_query(query)?, limit)?
            .into_iter()
            .map(|(record, distance)| MemorySearchHit::new(record, distance))
            .collect(),
        MemoryCategory::Behavioral => backend
            .search_behavioral(model, access, text_query(query)?, limit)?
            .into_iter()
            .map(|(record, distance)| MemorySearchHit::new(record, distance))
            .collect(),
    };
    Ok(hits)
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
