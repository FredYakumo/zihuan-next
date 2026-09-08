mod brain_agent;
mod service_manager;
mod service_type_ext;

pub use brain_agent::{
    ContextCompactionEvent, ContextCompactionObserver, InferenceToolContext, InferenceToolProvider,
    RoleBrainAgent,
};
pub use service_manager::{
    build_role_tool_provider, RoleServiceManager, RoleServiceRuntimeInfo, RoleServiceRuntimeStatus,
};
pub use service_type_ext::{
    is_qq_chat, is_qq_chat_agent, is_workspace, is_workspace_agent, optional_qq_chat,
    optional_workspace, primary_llm_ref_id, qq_chat_from, qq_chat_of, role_service_kind_of,
    workspace_from, workspace_of,
};
