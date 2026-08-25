use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::steer::PendingSteerStore;
use zihuan_core::graph::tool_spec::ToolDefinition;
use zihuan_core::graph::function_graph::FunctionPortDef;

use crate::qq_chat::model::context::QqChatAgentServiceRuntimeConfig;

/// Internal mutable state for the QQ chat agent service.
pub struct QqChatAgentServiceInner {
    pub(crate) id: String,
    pub(crate) default_tools_enabled: HashMap<String, bool>,
    pub(crate) shared_inputs: Vec<FunctionPortDef>,
    pub(crate) tool_definitions: Vec<ToolDefinition>,
}

/// Top-level QQ chat agent service that owns the runtime config and dispatches events.
pub struct QqChatAgentService {
    pub(crate) inner: QqChatAgentServiceInner,
    pub(crate) config: QqChatAgentServiceRuntimeConfig,
    pub(crate) pending_steer: Arc<PendingSteerStore>,
}
