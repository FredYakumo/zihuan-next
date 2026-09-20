use std::sync::Arc;

use async_trait::async_trait;
use zihuan_core::agent::Agent;
use zihuan_core::agent::AgentContext;
use zihuan_core::error::Result;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

use super::run_blocking;
use crate::agent::before_brain_agent::{BeforeBrainAgent, PrepromptContext};

/// QQ before-brain procedure: prepares continuity and emotion context for the reply agent.
///
/// Wraps the `BeforeBrainAgent` implementation (an `Agent`); the produced context block is
/// injected into the main reply prompt, so the procedure is `Blocking`.
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
        let output = agent.run(AgentContext::default(), ()).await?;
        Ok(ProcedureOutput::of(output))
    }
}

/// Execute the QQ before-brain procedure and return its context block for the reply prompt.
pub(crate) fn run_before_brain(
    shared: &mut ProcedureContext,
    context: PrepromptContext<'_>,
) -> Result<Option<String>> {
    let output = run_blocking(Arc::new(QqBeforeBrain::new(context)), shared)?;
    Ok(output.cloned::<Option<String>>().unwrap_or(None))
}
