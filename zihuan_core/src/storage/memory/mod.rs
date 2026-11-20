//! Category-specific memory APIs. Each category owns exactly one model — the domain
//! record is the storage document and the wire shape. This module carries the shared
//! backend dispatch, the cross-category hit machinery, and the ranking rules.
use crate::algorithm::rrf::{reciprocal_rank_fusion, DEFAULT_RRF_RANK_CONSTANT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use super::{AgentMemoryAccessContext, LocalMemoryStore, MemoryCategory, RetrievalStoreRef};
use crate::error::Result;
use crate::model_inference::llm::embedding_base::EmbeddingBase;

mod behavioral;
mod concept;
mod person;
pub(crate) use behavioral::parse_behavioral_record;
pub use behavioral::{
    prepare_behavioral, BehavioralMemoryPrepared, BehavioralMemoryRecord, BehavioralMemoryWrite,
};
pub(crate) use concept::{default_access_level, parse_concept_record};
pub use concept::{
    prepare_concept, ConceptMemoryPrepared, ConceptMemoryRecord, ConceptMemoryWrite,
};
pub(crate) use person::parse_person_record;
pub use person::{
    memory_query_schema, person_search_text, prepare_person, MemoryQuery, PersonMemoryRecord,
    PersonMemoryWrite,
};

#[derive(Clone)]
pub enum MemoryBackend {
    LocalFile(Arc<LocalMemoryStore>),
    RetrievalStore(Arc<RetrievalStoreRef>),
}

/// One memory record in the class its category implies. The category tag doubles as the
/// storage discriminator; each variant serializes only its own fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "category", rename_all = "snake_case")]
pub enum MemoryRecord {
    Concept(ConceptMemoryRecord),
    Person(PersonMemoryRecord),
    Behavioral(BehavioralMemoryRecord),
}

impl MemoryRecord {
    pub fn category(&self) -> MemoryCategory {
        match self {
            Self::Concept(_) => MemoryCategory::Concept,
            Self::Person(_) => MemoryCategory::Person,
            Self::Behavioral(_) => MemoryCategory::Behavioral,
        }
    }

    pub fn object_id(&self) -> &str {
        match self {
            Self::Concept(record) => &record.object_id,
            Self::Person(record) => &record.object_id,
            Self::Behavioral(record) => &record.object_id,
        }
    }

    pub fn created_at(&self) -> &str {
        match self {
            Self::Concept(record) => &record.created_at,
            Self::Person(record) => &record.created_at,
            Self::Behavioral(record) => &record.created_at,
        }
    }

    pub fn updated_at(&self) -> &str {
        match self {
            Self::Concept(record) => &record.updated_at,
            Self::Person(record) => &record.updated_at,
            Self::Behavioral(record) => &record.updated_at,
        }
    }

    pub fn access_level(&self) -> u32 {
        match self {
            Self::Concept(record) => record.access_level,
            Self::Person(record) => record.access_level,
            Self::Behavioral(record) => record.access_level,
        }
    }
}

impl From<ConceptMemoryRecord> for MemoryRecord {
    fn from(record: ConceptMemoryRecord) -> Self {
        Self::Concept(record)
    }
}

impl From<PersonMemoryRecord> for MemoryRecord {
    fn from(record: PersonMemoryRecord) -> Self {
        Self::Person(record)
    }
}

impl From<BehavioralMemoryRecord> for MemoryRecord {
    fn from(record: BehavioralMemoryRecord) -> Self {
        Self::Behavioral(record)
    }
}

/// One memory search hit: the record plus the backend's relevance when ranked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchHit {
    #[serde(flatten)]
    pub record: MemoryRecord,
    /// Rank score, not similarity or confidence. Raw backend queries may temporarily
    /// carry their original score before fusion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
}

impl MemorySearchHit {
    pub fn new(record: impl Into<MemoryRecord>, distance: Option<f64>) -> Self {
        Self { record: record.into(), distance }
    }
}

impl MemoryBackend {
    /// Lists accessible records of one category, most effective first, without a ranking
    /// query and without renewing behavioural expiry. Backs the recall phase's key scan.
    pub fn list_records(
        &self,
        access: &AgentMemoryAccessContext,
        category: MemoryCategory,
        limit: usize,
    ) -> Result<Vec<MemorySearchHit>> {
        self.query_records(None, access, category, None, limit)
    }

    fn query_records(
        &self,
        model: Option<&dyn EmbeddingBase>,
        access: &AgentMemoryAccessContext,
        category: MemoryCategory,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemorySearchHit>> {
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
                    limit,
                )?,
                None => super::list_memory_in_store(store, &access, 100)?,
            },
        };
        hits.retain(|hit| super::memory_is_accessible(&hit.record, &access));
        if let Some(query) = query {
            sort_memory_search_hits(query, &mut hits);
            hits.truncate(limit);
        }
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

/// Exact names/titles precede fuzzy recall. Scores only break ties within that
/// tier; time only breaks score ties. This also orders cross-category UI hits.
pub fn sort_memory_search_hits(query: &str, hits: &mut [MemorySearchHit]) {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return;
    }
    let exact = |record: &MemoryRecord| match record {
        MemoryRecord::Person(person) => {
            let profile = &person.profile;
            [&profile.qq_id, &profile.name, &profile.preferred_name]
                .into_iter()
                .chain(profile.aliases.iter())
                .any(|name| name.trim().to_lowercase() == query)
        }
        MemoryRecord::Concept(concept) => concept.key.trim().to_lowercase() == query,
        MemoryRecord::Behavioral(behavioral) => behavioral.key.trim().to_lowercase() == query,
    };
    hits.sort_by(|a, b| {
        exact(&b.record)
            .cmp(&exact(&a.record))
            .then_with(|| b.distance.unwrap_or(0.0).total_cmp(&a.distance.unwrap_or(0.0)))
            .then_with(|| b.record.updated_at().cmp(a.record.updated_at()))
            .then_with(|| a.record.object_id().cmp(b.record.object_id()))
    });
}

pub(crate) fn fuse_memory_hits(rankings: &[Vec<MemorySearchHit>]) -> Vec<MemorySearchHit> {
    let object_ids = rankings
        .iter()
        .map(|hits| hits.iter().map(|hit| hit.record.object_id().to_string()).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let ranking_ids = object_ids.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let fused = reciprocal_rank_fusion(&ranking_ids, DEFAULT_RRF_RANK_CONSTANT);
    let mut by_id = HashMap::new();
    for hit in rankings.iter().flatten() {
        by_id.entry(hit.record.object_id().to_string()).or_insert_with(|| hit.clone());
    }
    fused
        .into_iter()
        .filter_map(|(object_id, score)| {
            by_id.get(&object_id).map(|hit| MemorySearchHit {
                record: hit.record.clone(),
                distance: Some(score),
            })
        })
        .collect()
}
