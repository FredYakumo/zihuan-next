mod brain_agent;
mod service_manager;

pub use brain_agent::{InferenceToolContext, InferenceToolProvider, RoleBrainAgent};
pub use service_manager::{
    build_role_tool_provider, RoleServiceManager, RoleServiceRuntimeInfo, RoleServiceRuntimeStatus,
};
