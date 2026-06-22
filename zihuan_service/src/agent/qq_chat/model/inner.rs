use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::steer::PendingSteerStore;
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;
use zihuan_graph_engine::function_graph::FunctionPortDef;

use crate::agent::qq_chat::model::context::QqChatAgentServiceRuntimeConfig;

/// Internal mutable state for the QQ chat agent service.
pub struct QqChatAgentServiceInner {
    pub(crate) id: String,
    pub(crate) default_tools_enabled: HashMap<String, bool>,
    pub(crate) shared_inputs: Vec<FunctionPortDef>,
    pub(crate) tool_definitions: Vec<BrainToolDefinition>,
}

/// Top-level QQ chat agent service that owns the runtime config and dispatches events.
pub struct QqChatAgentService {
    pub(crate) inner: QqChatAgentServiceInner,
    pub(crate) config: QqChatAgentServiceRuntimeConfig,
    pub(crate) pending_steer: Arc<PendingSteerStore>,
}
