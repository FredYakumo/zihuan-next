use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use zihuan_core::agent::tools::ToolCallingObserver;
use zihuan_core::agent::{Agent, AgentCancellation, AgentContext, ContextCompactionObserver};
use zihuan_core::error::{Error, Result};
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

/// The turn's brain invocation as a Procedure (documents/procedure.md).
///
/// **Design:** Created per turn by the transport adapter with a concrete agent whose model
/// bindings are already fixed for this turn. [`WorkspaceBrain::run`] drives the streaming turn
/// through the [`Agent`] trait, wiring the transport's token sink plus the per-turn observers
/// into the [`AgentContext`]; its [`ProcedureOutput`] carries
/// `(Vec<LLMMessage>, ToolCallingStopReason)` for the transport adapter and the following
/// procedures.
pub struct WorkspaceBrain<A>
where
    A: Agent<Input = Vec<LLMMessage>>,
{
    agent: Arc<A>,
    messages: Mutex<Option<Vec<LLMMessage>>>,
    session_id: Option<String>,
    workspace_path: Option<String>,
    cancellation: Option<Arc<dyn AgentCancellation>>,
    observer: Option<Arc<dyn ToolCallingObserver>>,
    compaction_observer: Option<ContextCompactionObserver>,
}

impl<A> WorkspaceBrain<A>
where
    A: Agent<Input = Vec<LLMMessage>>,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent: Arc<A>,
        messages: Vec<LLMMessage>,
        session_id: Option<String>,
        workspace_path: Option<String>,
        cancellation: Option<Arc<dyn AgentCancellation>>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
        compaction_observer: Option<ContextCompactionObserver>,
    ) -> Self {
        Self {
            agent,
            messages: Mutex::new(Some(messages)),
            session_id,
            workspace_path,
            cancellation,
            observer,
            compaction_observer,
        }
    }
}

#[async_trait]
impl<A> Procedure for WorkspaceBrain<A>
where
    A: Agent<Input = Vec<LLMMessage>> + Send + Sync + 'static,
    A::Output: Send + Sync + 'static,
{
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "workspace_brain",
            name: "Workspace Brain",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, context: &ProcedureContext) -> Result<ProcedureOutput> {
        let messages = self.messages.lock().unwrap().take().ok_or_else(|| {
            Error::ValidationError("workspace brain messages already consumed".to_string())
        })?;
        let sink = context.transport_out.clone().ok_or_else(|| {
            Error::ValidationError("workspace brain requires a transport out sink".to_string())
        })?;
        let agent_context = AgentContext {
            session_id: self.session_id.clone(),
            workspace_path: self.workspace_path.clone(),
            cancellation: self.cancellation.clone(),
            token_tx: Some(sink.token_sender()),
            observer: self.observer.clone(),
            compaction_observer: self.compaction_observer.clone(),
            ..Default::default()
        };
        let output = self.agent.run_streaming(agent_context, messages).await?;
        Ok(ProcedureOutput::of(output))
    }
}
