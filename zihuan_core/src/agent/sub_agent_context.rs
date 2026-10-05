//! Shared service context for assembling a declarative sub-agent host.
//!
//! Service crates describe which capabilities they can offer their sub agents — a memory
//! backend, LLM handles, and already-constructed tools — and
//! [`AgentServiceContext::build_host`] turns that description into an [`AgentHost`] whose
//! `tool_ids` resolve uniformly across services. Publishing agent ids stays with the caller,
//! which keeps publish order and tool exposure as service policy.

use std::sync::Arc;

use crate::agent::declarative_agent::AgentHost;
use crate::agent::tools::memory_tools::{
    register_memory_agent_tool, register_memory_tools, MemoryAgentResources, MemoryBackend,
    MEMORY_AGENT_ID, MEMORY_TOOL_IDS,
};
use crate::agent::tools::Tool;
use crate::agent::LLM_KIND_MAIN;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::retrieval::RetrievalStoreRef;
use crate::storage::{AgentMemoryAccessContext, LocalMemoryStore};

const MEMORY_BACKEND_UNAVAILABLE_REASON: &str = "memory backend is not configured";
const MEMORY_LLM_UNAVAILABLE_REASON: &str = "the main LLM is not available";

/// Memory backend plus the embedding model an external backend requires.
#[derive(Clone)]
pub struct MemoryCapability {
    pub backend: MemoryBackend,
    pub embedding_model: Option<Arc<dyn EmbeddingBase>>,
}

impl MemoryCapability {
    /// Resolves the memory capability from the configured stores: the local on-disk store
    /// wins when present; an external retrieval store is only usable with an embedding model.
    pub fn resolve(
        local_memory_store: Option<Arc<LocalMemoryStore>>,
        retrieval_store: Option<Arc<RetrievalStoreRef>>,
        embedding_model: Option<Arc<dyn EmbeddingBase>>,
    ) -> Option<Self> {
        let backend = if let Some(store) = local_memory_store {
            MemoryBackend::LocalFile(store)
        } else if let Some(store) = retrieval_store {
            if embedding_model.is_none() {
                log::warn!(
                    "memory capability disabled because the external retrieval store has no embedding model"
                );
                return None;
            }
            MemoryBackend::RetrievalStore(store)
        } else {
            return None;
        };
        Some(Self { backend, embedding_model })
    }
}

/// Constructed service resources a declarative sub-agent host is built from.
#[derive(Clone, Default)]
pub struct AgentServiceContext {
    memory: Option<MemoryCapability>,
    llms: Vec<(String, Arc<dyn LLMBase>)>,
    tools: Vec<(String, Arc<dyn Tool>)>,
}

impl AgentServiceContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the memory capability; `None` registers the memory tools as disabled
    /// placeholders so definitions that reference them still resolve.
    pub fn with_memory(mut self, memory: Option<MemoryCapability>) -> Self {
        self.memory = memory;
        self
    }

    /// Registers the LLM handle a sub agent's `llm_kind` resolves to.
    pub fn with_llm(mut self, kind: impl Into<String>, llm: Arc<dyn LLMBase>) -> Self {
        self.llms.push((kind.into(), llm));
        self
    }

    /// Registers an already-constructed tool under the id sub agents reference in `tool_ids`.
    pub fn with_tool(mut self, id: impl Into<String>, tool: Arc<dyn Tool>) -> Self {
        self.tools.push((id.into(), tool));
        self
    }

    pub fn memory(&self) -> Option<&MemoryCapability> {
        self.memory.as_ref()
    }

    pub fn llm(&self, kind: &str) -> Option<Arc<dyn LLMBase>> {
        self.llms
            .iter()
            .find(|(registered, _)| registered == kind)
            .map(|(_, llm)| Arc::clone(llm))
    }

    /// Assembles the host: memory tools (real or disabled placeholders), the service's
    /// constructed tools, every registered LLM kind, and the composed memory agent tool.
    pub fn build_host(&self, memory_access: AgentMemoryAccessContext) -> AgentHost {
        let mut host = AgentHost::new();
        let memory_ready = match self.memory.as_ref().zip(self.llm(LLM_KIND_MAIN)) {
            Some((memory, llm)) => {
                register_memory_tools(
                    &mut host,
                    MemoryAgentResources {
                        memory_backend: memory.backend.clone(),
                        embedding_model: memory.embedding_model.clone(),
                        llm,
                        access: memory_access,
                    },
                );
                true
            }
            None => {
                host.register_disabled_tools(
                    MEMORY_TOOL_IDS,
                    if self.memory.is_some() {
                        MEMORY_LLM_UNAVAILABLE_REASON
                    } else {
                        MEMORY_BACKEND_UNAVAILABLE_REASON
                    },
                );
                false
            }
        };
        for (id, tool) in &self.tools {
            host.register_tool(id.clone(), Arc::clone(tool));
        }
        for (kind, llm) in &self.llms {
            host.register_llm(kind.clone(), Arc::clone(llm));
        }
        // Compose the memory agent last: its definition resolves the memory tools and the
        // `main` LLM registered above.
        let memory_agent_registered = memory_ready
            && register_memory_agent_tool(&mut host)
                .map_err(|error| log::warn!("memory agent could not be composed: {error}"))
                .is_ok();
        if !memory_agent_registered {
            host.register_disabled_tool(
                MEMORY_AGENT_ID,
                if self.memory.is_some() {
                    MEMORY_LLM_UNAVAILABLE_REASON
                } else {
                    MEMORY_BACKEND_UNAVAILABLE_REASON
                },
            );
        }
        host
    }
}
