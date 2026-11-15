//! Computes the running role services affected by saving/deleting a config.
//!
//! Save/delete handlers call [`affected_running_services`] before persisting
//! the change and attach the result to their response, so the frontend can ask
//! the user to restart the affected role services.

use zihuan_core::agent::declarative_agent::load_agent_definitions_lenient;
use zihuan_core::config::role_services::load_role_services;
use zihuan_core::connection::utils::utilsdep_analyze::{
    analyze_dependent_role_services, ConfigDependencyKind, DependentRoleService,
};
use zihuan_service::RoleServiceRuntimeStatus;

use log::warn;

use crate::api::state::AppState;

/// Role services that reference `config_id` and are currently running.
///
/// A failed config load degrades to an empty list (with a warning): dependency
/// analysis is advisory and must not fail the surrounding save/delete.
pub fn affected_running_services(
    state: &AppState,
    kind: ConfigDependencyKind,
    config_id: &str,
) -> Vec<DependentRoleService> {
    let role_services = match load_role_services() {
        Ok(role_services) => role_services,
        Err(err) => {
            warn!(
                "[dependency_check] failed to load role services while analyzing kind={:?} id={}: {}",
                kind, config_id, err
            );
            return Vec::new();
        }
    };
    let agent_definitions = load_agent_definitions_lenient();

    analyze_dependent_role_services(kind, config_id, &role_services, &agent_definitions)
        .into_iter()
        .filter(|service| {
            state.role_service_manager.runtime_info(&service.id).status
                == RoleServiceRuntimeStatus::Running
        })
        .collect()
}
