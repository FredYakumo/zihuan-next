//! Category-specific memory APIs. Legacy records are only the persistence/wire format.
use std::sync::Arc;

use super::{
    AgentMemoryAccessContext, AgentMemoryRecord, AgentMemorySearchHit, AgentMemoryUpsert,
    LocalMemoryStore, MemoryCategory, RetrievalStoreRef,
};
use crate::error::Result;
use crate::model_inference::llm::embedding_base::EmbeddingBase;

mod behavioral;
mod concept;
mod person;
pub use behavioral::{behavioral_key, BehavioralMemoryRecord, BehavioralMemoryWrite};
pub use concept::{ConceptMemoryRecord, ConceptMemoryWrite};
pub use person::{
    memory_query_schema, person_search_text, MemoryQuery, PersonMemoryRecord, PersonMemoryWrite,
};

#[derive(Clone)]
pub enum MemoryBackend {
    LocalFile(Arc<LocalMemoryStore>),
    RetrievalStore(Arc<RetrievalStoreRef>),
}

impl MemoryBackend {
    fn read_record(&self, identity: &AgentMemoryUpsert) -> Result<Option<AgentMemoryRecord>> {
        match self {
            Self::LocalFile(store) => store.get(identity),
            Self::RetrievalStore(store) => super::get_memory_in_store(store, identity),
        }
    }

    fn write_record(
        &self,
        model: Option<&dyn EmbeddingBase>,
        input: &AgentMemoryUpsert,
    ) -> Result<AgentMemoryRecord> {
        match self {
            Self::LocalFile(store) => store.create_or_update(input),
            Self::RetrievalStore(store) => {
                let text = if input.attributes.category == MemoryCategory::Person {
                    person_search_text(&input.attributes.person)
                } else {
                    format!("{}\n{}", input.key, input.value)
                };
                super::upsert_memory_in_store(store, input, embedding_vector(model, &text)?)
            }
        }
    }

    fn query_records(
        &self,
        model: Option<&dyn EmbeddingBase>,
        access: &AgentMemoryAccessContext,
        category: MemoryCategory,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AgentMemorySearchHit>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let access = AgentMemoryAccessContext {
            category,
            skip_expiry_extend: true,
            ..access.clone()
        };
        let mut hits = match self {
            Self::LocalFile(store) => store.list_category(category, query, 100)?,
            Self::RetrievalStore(store) => match query {
                Some(query) => super::search_memory_in_store(
                    store,
                    &access,
                    query,
                    &embedding_vector(model, query)?,
                    limit.max(20),
                )?,
                None => super::list_memory_in_store(store, &access, 100)?,
            },
        };
        hits.retain(|hit| super::memory_is_accessible(&hit.record, &access));
        Ok(hits)
    }
}

pub(crate) fn embedding_vector(model: Option<&dyn EmbeddingBase>, text: &str) -> Result<Vec<f32>> {
    model
        .ok_or_else(|| {
            crate::error::Error::ValidationError(
                "memory backend requires an embedding model".to_string(),
            )
        })?
        .inference(text)
}
