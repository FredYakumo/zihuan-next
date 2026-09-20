use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::UnboundedSender;

use crate::agent::tools::ToolCallingObserver;
use crate::error::Result;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::StreamToken;

/// Milestone events reported by context compaction inside a streaming turn.
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

/// Request-scoped execution environment handed to one agent turn.
///
/// Carries correlation, cancellation, and — for streaming or externally reviewable turns —
/// the token and tool-event channels through which the agent reports its intermediate
/// execution. Domain-specific turn inputs belong in each agent's `Input`; this context
/// intentionally only carries cross-cutting concerns.
#[derive(Clone, Default)]
pub struct AgentContext {
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub parent_agent_id: Option<String>,
    pub workspace_path: Option<String>,
    pub cancellation: Option<Arc<dyn AgentCancellation>>,
    /// Token sink for streaming turns; `None` for non-streaming execution.
    pub token_tx: Option<UnboundedSender<StreamToken>>,
    /// Observer receiving tool-call events during execution. Streaming and reviewable turns
    /// report through it; non-streaming turns may ignore it.
    pub observer: Option<Arc<dyn ToolCallingObserver>>,
    /// Observer receiving context-compaction milestones during a turn.
    pub compaction_observer: Option<ContextCompactionObserver>,
}

impl AgentContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|cancellation| cancellation.is_cancelled())
    }
}

pub trait AgentCancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

/// One reasoning unit in the system: configurable, self-describing, and executable as a turn.
///
/// **Design:** Every agent declares its own construction config, typed turn input, and typed
/// turn output. A Brain procedure (documents/procedure.md) depends on this trait so each
/// RoleService crate can assemble its own main agent while the procedure chain stays
/// role-agnostic. `run` executes one turn to completion; `run_streaming` executes the same turn
/// while reporting intermediate tokens and tool events through the context channels, which is
/// the uniform hook for dynamic review of procedure execution. Implementations must provide both
/// for real — a non-streaming sub-agent still reports its tool calls when invoked through
/// `run_streaming` — rather than degrading one to the other silently.
#[async_trait]
pub trait Agent: Send + Sync {
    type Config;
    type Input: Send;
    type Output: Send;

    /// Agent identity, used in logs and resource-resolution errors.
    fn name(&self) -> &str;

    /// The agent's own configured LLM.
    fn llm(&self) -> Arc<dyn LLMBase>;

    /// Construct the agent from its configuration.
    fn new(config: Self::Config) -> Result<Self>
    where
        Self: Sized;

    /// Run one turn to completion without streaming, returning the full output.
    async fn run(&self, context: AgentContext, input: Self::Input) -> Result<Self::Output>;

    /// Run one turn while reporting tokens and tool events through the context channels.
    async fn run_streaming(
        &self,
        context: AgentContext,
        input: Self::Input,
    ) -> Result<Self::Output>;
}
