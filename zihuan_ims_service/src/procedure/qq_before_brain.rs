use async_trait::async_trait;
use zihuan_core::error::Result;
use zihuan_core::role::procedure::{
    Procedure, ProcedureContext, ProcedureDescriptor, ProcedureExecution, ProcedureOutput,
};

use super::before_brain::{assemble_before_brain, BeforeBrainContext};

/// QQ before-brain procedure: assembles the turn's system prompt and user input without any
/// model inference.
///
/// Its output (assembled system prompt, current-turn user input, and memory-title hint) is the
/// finished turn context handed to the brain procedure through
/// [`ProcedureContext::procedure_outputs`]. The brain appends history and runs the tool loop; it
/// does not assemble the input itself.
///
/// The input decides how the brain answers, so this procedure is `Blocking` and runs before the
/// brain procedure in the turn chain.
pub(crate) struct QqBeforeBrain<'a> {
    context: BeforeBrainContext<'a>,
}

impl<'a> QqBeforeBrain<'a> {
    pub(crate) fn new(context: BeforeBrainContext<'a>) -> Self {
        Self { context }
    }
}

#[async_trait]
impl Procedure for QqBeforeBrain<'_> {
    fn descriptor(&self) -> ProcedureDescriptor {
        ProcedureDescriptor {
            id: "qq_before_brain",
            name: "QQ Before Brain",
            execution: ProcedureExecution::Blocking,
        }
    }

    async fn run(&self, _context: &ProcedureContext) -> Result<ProcedureOutput> {
        Ok(ProcedureOutput::of(assemble_before_brain(&self.context)))
    }
}
