mod qq_after_brain;
mod qq_before_brain;

use std::sync::Arc;

use zihuan_core::error::{Error, Result};
use zihuan_core::role::procedure::{
    execute_blocking_procedures, Procedure, ProcedureContext, ProcedureOutput,
};
use zihuan_core::runtime::block_async;

pub(crate) use qq_after_brain::{run_after_brain, QqAfterBrainContext};
pub(crate) use qq_before_brain::run_before_brain;

/// Build the shared procedure context for a QQ turn.
///
/// QQ turns have no dashboard session: the chat history key stands in as the session identity.
/// QQ procedure payloads are bundled into their procedures directly, so the shared context only
/// carries informational facts.
pub(crate) fn qq_procedure_context(
    session_key: &str,
    latest_user_text: Option<String>,
) -> ProcedureContext {
    ProcedureContext {
        session_id: session_key.to_string(),
        is_new_conversation: false,
        latest_user_text,
        workspace_path: None,
        role_context: None,
    }
}

/// Run one blocking procedure to completion on the calling thread and return its output.
///
/// **Design:** The QQ turn pipeline is synchronous; `block_async` bridges it to the async
/// procedure executor exactly like the existing `run_preprompt` internals already do. Blocking
/// procedures execute their synchronous bodies inline, so thread semantics stay identical to
/// the previous direct calls.
fn run_blocking(
    procedure: Arc<dyn Procedure + '_>,
    shared: &ProcedureContext,
) -> Result<ProcedureOutput> {
    let mut outputs = block_async(execute_blocking_procedures(vec![procedure], shared))?;
    outputs
        .pop()
        .ok_or_else(|| Error::ValidationError("procedure produced no output".to_string()))
}
