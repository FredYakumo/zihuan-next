use std::any::Any;
use std::sync::Arc;

/// Identifies a connection resource understood by the agent runtime.
///
/// Implementations decide how each kind maps to their service configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentConnectionSlot {
    Rdb,
    S3,
    ImageWeaviate,
    WebSearch,
}

/// Provides the model and connection resources required by an agent runtime.
///
/// Runtime consumers, such as subgraph tools and script nodes, use this trait
/// without depending on concrete service configuration types. Business logic
/// that requires the complete configuration can downcast the provider through
/// [`AgentResourceProvider::as_any`].
pub trait AgentResourceProvider: Send + Sync {
    /// Returns the LLM reference ID for the requested usage.
    ///
    /// Implementations are responsible for applying their fallback policy.
    fn llm_ref_id(&self, kind: &str) -> Option<String>;

    /// Returns the embedding model reference ID.
    fn embedding_model_ref_id(&self) -> Option<String>;

    /// Returns the connection ID for the requested resource kind.
    fn connection_id(&self, kind: AgentConnectionSlot) -> Option<String>;

    /// Exposes the concrete provider for business-specific downcasting.
    fn as_any(&self) -> &dyn Any;
}

///  shared agent resource provider.
pub type SharedAgentResourceProvider = Arc<dyn AgentResourceProvider>;
