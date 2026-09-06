mod workspace_session_title;

use std::sync::Arc;

use zihuan_core::agent::service_config::RoleServiceConfig;
use zihuan_core::role::procedure::{Procedure, ProcedureContext, ProcedurePhase};

pub use workspace_session_title::WorkspaceSessionTitle;

/// Collect the workspace procedures to execute for the given lifecycle phase.
///
/// **Design:** Applicability is decided here so callers stay branch-free: procedures that do not
/// apply to the current work unit (e.g. naming an already existing conversation) are simply not
/// returned.
pub fn collect(
    phase: ProcedurePhase,
    agent: &RoleServiceConfig,
    context: &ProcedureContext,
) -> Vec<Arc<dyn Procedure>> {
    match phase {
        ProcedurePhase::BeforeBrain => workspace_session_title::collect(agent, context),
        ProcedurePhase::AfterBrain => Vec::new(),
    }
}
