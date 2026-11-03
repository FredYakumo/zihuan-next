use std::cmp::Ordering;

use crate::error::Result;
use crate::storage::{search_memory_in_store, AgentMemoryAccessContext};

use super::resources::{MemoryAgentResources, MemoryBackend};
use super::store::embedding_vector;

/// One memory title returned by [`relevant_memory_titles`].
#[derive(Debug, Clone)]
pub struct MemoryTitleHit {
    pub title: String,
    /// Relevance after the backend's normalization (`higher is better`); `None` for backends
    /// that do not rank results (local-file memory returns query-filtered titles only).
    pub relevance: Option<f64>,
}

/// Searches agent memory for the titles most relevant to `query`.
///
/// This is the cheap counterpart of [`SearchMemoryTool`](super::SearchMemoryTool): it reads only
/// titles (never the stored content), so a turn can hint at existing memories without loading
/// them. Ranking remains the backend's, and results are ordered by relevance when the backend
/// reports it. The lookup is read-only: unlike an agent-initiated search it never extends the
/// expiry of the records it touches.
pub fn relevant_memory_titles(
    resources: &MemoryAgentResources,
    query: &str,
    top_n: usize,
) -> Result<Vec<MemoryTitleHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let limit = top_n.max(1);
    let access = AgentMemoryAccessContext {
        skip_expiry_extend: true,
        ..resources.access.clone()
    };
    let hits = match &resources.memory_backend {
        MemoryBackend::LocalFile(store) => store.list(Some(query), limit)?,
        MemoryBackend::RetrievalStore(store) => search_memory_in_store(
            store,
            &access,
            query,
            &embedding_vector(resources.embedding_model.as_deref(), query)?,
            limit,
        )?,
    };
    let mut titles = hits
        .into_iter()
        .map(|hit| MemoryTitleHit {
            title: hit.record.key,
            relevance: hit.distance,
        })
        .collect::<Vec<_>>();
    titles.sort_by(|left, right| {
        right
            .relevance
            .unwrap_or_default()
            .partial_cmp(&left.relevance.unwrap_or_default())
            .unwrap_or(Ordering::Equal)
    });
    Ok(titles)
}
