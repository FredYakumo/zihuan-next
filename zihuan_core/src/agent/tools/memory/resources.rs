use std::sync::Arc;

use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::retrieval::RetrievalStoreRef;
use crate::storage::{AgentMemoryAccessContext, LocalMemoryStore};

/// Everything a memory tool needs to read or write the agent's memory.
#[derive(Clone)]
pub struct MemoryAgentResources {
    pub memory_backend: MemoryBackend,
    pub embedding_model: Option<Arc<dyn EmbeddingBase>>,
    pub llm: Arc<dyn LLMBase>,
    pub access: AgentMemoryAccessContext,
}

/// Where agent memory is stored.
///
/// External backends are reached through the agent's unified retrieval store,
/// which selects the concrete Weaviate/Elasticsearch collection at call time.
#[derive(Clone)]
pub enum MemoryBackend {
    LocalFile(Arc<LocalMemoryStore>),
    RetrievalStore(Arc<RetrievalStoreRef>),
}
