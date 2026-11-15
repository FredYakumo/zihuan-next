use std::sync::Arc;

use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::storage::AgentMemoryAccessContext;

/// Everything a memory tool needs to read or write the agent's memory.
#[derive(Clone)]
pub struct MemoryAgentResources {
    pub memory_backend: MemoryBackend,
    pub embedding_model: Option<Arc<dyn EmbeddingBase>>,
    pub llm: Arc<dyn LLMBase>,
    pub access: AgentMemoryAccessContext,
}

pub use crate::storage::MemoryBackend;
