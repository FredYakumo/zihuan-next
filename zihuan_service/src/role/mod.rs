mod brain_agent;
mod service_manager;

pub use brain_agent::{
    ContextCompactionEvent, ContextCompactionObserver, InferenceToolContext, InferenceToolProvider,
    RoleBrainAgent,
};
pub use service_manager::{
    build_role_tool_provider, RoleServiceManager, RoleServiceRuntimeInfo, RoleServiceRuntimeStatus,
};
