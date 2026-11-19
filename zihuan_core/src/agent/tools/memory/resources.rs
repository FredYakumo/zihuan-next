use std::sync::Arc;

use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::storage::AgentMemoryAccessContext;

/// Everything a memory tool needs to read or write the agent's memory.
///
/// Recall and search run in pure Rust over the backend and its embedding model; no
/// chat LLM is involved.
#[derive(Clone)]
pub struct MemoryAgentResources {
    pub memory_backend: MemoryBackend,
    pub embedding_model: Option<Arc<dyn EmbeddingBase>>,
    pub access: AgentMemoryAccessContext,
}

pub use crate::storage::MemoryBackend;
