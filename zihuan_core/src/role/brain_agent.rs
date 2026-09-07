use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use crate::agent::tools::{ToolCallingObserver, ToolCallingStopReason};
use crate::agent::AgentCancellation;
use crate::error::Result;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::{LLMMessage, StreamToken};

#[derive(Debug, Clone)]
pub enum ContextCompactionEvent {
    Started,
    Completed {
        estimated_tokens_before: usize,
        estimated_tokens_after: usize,
        duration: Duration,
    },
    Failed,
}

pub type ContextCompactionObserver = Arc<dyn Fn(ContextCompactionEvent) + Send + Sync>;

/// The turn contract of a RoleService's main brain (documents/role-service.md).
///
/// **Design:** A Brain procedure (documents/procedure.md) depends on this trait instead of a
/// concrete brain implementation, so each RoleService crate can assemble its own brain while
/// the procedure chain stays role-agnostic. One call is one streaming brain turn: the
/// implementation owns everything shape-specific (context compaction, per-turn context
/// injection, tool assembly); the caller only supplies the turn inputs and receives the full
/// message trace plus the stop reason.
#[async_trait]
pub trait BrainAgent: Send + Sync {
    /// Role identity, used in logs and resource-resolution errors.
    fn name(&self) -> &str;

    /// The role's own configured LLM, used by turns without a model override.
    fn llm(&self) -> Arc<dyn LLMBase>;

    /// Run one streamed brain turn, returning the complete message trace and the stop reason.
    #[allow(clippy::too_many_arguments)]
    async fn run_streaming(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: UnboundedSender<StreamToken>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
        compaction_observer: Option<ContextCompactionObserver>,
        llm: Arc<dyn LLMBase>,
        image_understand_llm: Option<Arc<dyn LLMBase>>,
        workspace_path: Option<String>,
        session_id: Option<String>,
        cancellation: Option<Arc<dyn AgentCancellation>>,
    ) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)>;
}
