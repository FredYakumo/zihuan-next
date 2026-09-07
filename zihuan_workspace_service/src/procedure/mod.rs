mod workspace_brain;
mod workspace_session_title;

use std::sync::Arc;

use zihuan_core::agent::service_config::RoleServiceConfig;
use zihuan_core::role::procedure::{Procedure, ProcedureContext};

pub use workspace_brain::WorkspaceBrain;
pub use workspace_session_title::WorkspaceSessionTitle;

/// Collect the workspace procedures of one chat work unit, in execution order.
///
/// **Design:** Applicability is decided here so callers stay branch-free: procedures that do not
/// apply to the current work unit (e.g. naming an already existing conversation) are simply not
/// returned.
pub fn collect(agent: &RoleServiceConfig, context: &ProcedureContext) -> Vec<Arc<dyn Procedure>> {
    workspace_session_title::collect(agent, context)
}
