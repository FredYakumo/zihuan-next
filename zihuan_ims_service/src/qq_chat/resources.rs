use std::any::Any;
use std::sync::Arc;

use crate::role_config::{llm_ref_id_for_kind, QqChatRoleServiceConfig};
use zihuan_core::agent::resource_provider::{
    AgentConnectionSlot, AgentResourceProvider, SharedAgentResourceProvider,
};
use zihuan_core::agent::runtime_context::current_agent_resources;
use zihuan_core::error::{Error, Result};

/// Resource provider for the QQ chat service in the engine runtime: holds the full config
/// and only exposes the resource contract to the engine; business code recovers the full
/// config via downcast ([`Self::config`]).
#[derive(Clone)]
pub struct QqChatRoleServiceResources {
    config: QqChatRoleServiceConfig,
}

impl QqChatRoleServiceResources {
    pub fn new(config: QqChatRoleServiceConfig) -> Self {
        Self { config }
    }

    pub fn into_shared(self) -> SharedAgentResourceProvider {
        Arc::new(self)
    }

    pub fn config(&self) -> &QqChatRoleServiceConfig {
        &self.config
    }
}

impl AgentResourceProvider for QqChatRoleServiceResources {
    fn llm_ref_id(&self, kind: &str) -> Option<String> {
        llm_ref_id_for_kind(&self.config, kind).map(ToOwned::to_owned)
    }

    fn embedding_model_ref_id(&self) -> Option<String> {
        self.config.embedding_model_ref_id.clone()
    }

    fn connection_id(&self, kind: AgentConnectionSlot) -> Option<String> {
        match kind {
            AgentConnectionSlot::Rdb => self.config.resolved_rdb_id().map(ToOwned::to_owned),
            AgentConnectionSlot::S3 => self.config.rustfs_connection_id.clone(),
            AgentConnectionSlot::RetrievalStore => {
                self.config.retrieval_store_connection_id().map(ToOwned::to_owned)
            }
            AgentConnectionSlot::WebSearch => {
                Some(self.config.web_search_engine_connection_id.clone())
            }
        }
    }

    fn retrieval_store_is_local(&self) -> bool {
        self.config.retrieval_store_is_local()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub fn current_qq_chat_role_service_config() -> Result<QqChatRoleServiceConfig> {
    let resources = current_agent_resources()?;
    resources
        .as_any()
        .downcast_ref::<QqChatRoleServiceResources>()
        .map(|wrapper| wrapper.config.clone())
        .ok_or_else(|| {
            Error::ValidationError(
                "当前 Agent 运行时上下文不是 QQ 聊天服务，无法读取 QQ 配置".to_string(),
            )
        })
}
