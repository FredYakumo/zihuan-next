use std::any::Any;
use std::sync::Arc;

use crate::role_config::{llm_ref_id_for_kind, QqChatRoleServiceConfig};
use zihuan_core::agent::resource_provider::{
    AgentResourceProvider, ConnectionKind, SharedAgentResourceProvider,
};
use zihuan_core::agent::runtime_context::current_agent_resources;
use zihuan_core::error::{Error, Result};

/// QQ 聊天服务在引擎运行时的资源提供者：持有完整配置，只向引擎暴露资源契约；
/// 业务代码需要取回完整配置时通过 downcast（[`Self::config`]）恢复。
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

    fn connection_id(&self, kind: ConnectionKind) -> Option<String> {
        match kind {
            ConnectionKind::Rdb => self.config.resolved_rdb_id().map(ToOwned::to_owned),
            ConnectionKind::S3 => self.config.rustfs_connection_id.clone(),
            ConnectionKind::ImageWeaviate => self.config.weaviate_image_connection_id.clone(),
            ConnectionKind::WebSearch => Some(self.config.web_search_engine_connection_id.clone()),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// 业务代码取回当前 QQ 服务的完整配置（限流规则、情绪维度等引擎无关字段）。
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
