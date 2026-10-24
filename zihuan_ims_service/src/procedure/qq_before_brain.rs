use async_trait::async_trait;
use zihuan_core::agent::Agent;
use zihuan_core::agent::AgentContext;
use zihuan_core::error::Result;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

use crate::agent::before_brain_agent::{BeforeBrainAgent, PrepromptContext};
use crate::qq_chat::logging::emit_context_turn_failure;

/// Output of the QQ before-brain procedure for one turn.
///
/// `context_block` is the preprompt context block that anchors the main reply prompt; the
/// brain procedure reads it from [`ProcedureContext::procedure_outputs`].
#[derive(Clone)]
pub(crate) struct QqBeforeBrainOutput {
    pub(crate) context_block: Option<String>,
}

/// QQ before-brain procedure: prepares continuity and emotion context for the reply agent.
///
/// Wraps the `BeforeBrainAgent` implementation (an `Agent`); the produced context block is
/// consumed by the brain procedure through the shared procedure context, so this procedure is
/// `Blocking`.
pub(crate) struct QqBeforeBrain<'a> {
    context: PrepromptContext<'a>,
}

impl<'a> QqBeforeBrain<'a> {
    pub(crate) fn new(context: PrepromptContext<'a>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Procedure for QqBeforeBrain<'_> {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "qq_before_brain",
            name: "QQ Preprompt",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, _context: &ProcedureContext) -> Result<ProcedureOutput> {
        let agent = BeforeBrainAgent::new(self.context.clone());
        let context_block = match agent.run(AgentContext::default(), ()).await {
            Ok(output) => output,
            Err(error) => {
                if let Some(identity) = self.context.identity.as_ref() {
                    emit_context_turn_failure(
                        &identity.agent_id,
                        &identity.sender_id,
                        &identity.turn_id,
                        "Preprompt 阶段",
                        &error,
                    );
                }
                return Err(error);
            }
        };
        self.context.trace.record_graph_phase(
            "Preprompt 阶段",
            serde_json::json!({
                "context": context_block,
            }),
        );
        Ok(ProcedureOutput::of(QqBeforeBrainOutput { context_block }))
    }
}
