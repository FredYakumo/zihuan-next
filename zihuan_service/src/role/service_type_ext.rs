//! Typed access helpers over the erased [`RoleServiceType`].
//!
//! `zihuan_core` stores each role service's kind as a stable tag plus its raw config
//! payload. These helpers let `zihuan_service` orchestration code parse the payload
//! back into the concrete config types owned by each service crate.

use zihuan_core::error::{Error, Result};
use zihuan_core::role::service_config::{RoleServiceConfig, RoleServiceKind, RoleServiceType};
use zihuan_ims_service::role_config::QqChatRoleServiceConfig;
use zihuan_workspace_service::role_config::WorkspaceRoleServiceConfig;

pub fn is_qq_chat(role_service_type: &RoleServiceType) -> bool {
    role_service_type.kind == RoleServiceKind::QqChat
}

pub fn role_service_kind_of(role_service_type: &RoleServiceType) -> RoleServiceKind {
    role_service_type.kind
}

pub fn is_workspace(role_service_type: &RoleServiceType) -> bool {
    role_service_type.kind == RoleServiceKind::Workspace
}

pub fn qq_chat_of(role_service_type: &RoleServiceType) -> Result<QqChatRoleServiceConfig> {
    if !is_qq_chat(role_service_type) {
        return Err(Error::ValidationError("role service is not a qq_chat service".to_string()));
    }
    role_service_type.parse_typed_config()
}

pub fn workspace_of(role_service_type: &RoleServiceType) -> Result<WorkspaceRoleServiceConfig> {
    if !is_workspace(role_service_type) {
        return Err(Error::ValidationError("role service is not a workspace service".to_string()));
    }
    role_service_type.parse_typed_config()
}

pub fn qq_chat_from(config: QqChatRoleServiceConfig) -> RoleServiceType {
    RoleServiceType::from_typed_config(RoleServiceKind::QqChat, &config)
        .expect("qq_chat role service config must serialize to an object")
}

pub fn workspace_from(config: WorkspaceRoleServiceConfig) -> RoleServiceType {
    RoleServiceType::from_typed_config(RoleServiceKind::Workspace, &config)
        .expect("workspace role service config must serialize to an object")
}

pub fn optional_qq_chat(role_service_type: &RoleServiceType) -> Option<QqChatRoleServiceConfig> {
    qq_chat_of(role_service_type).ok()
}

pub fn optional_workspace(
    role_service_type: &RoleServiceType,
) -> Option<WorkspaceRoleServiceConfig> {
    workspace_of(role_service_type).ok()
}

pub fn is_workspace_role_service(role_service: &RoleServiceConfig) -> bool {
    is_workspace(&role_service.role_service_type)
}

pub fn is_qq_chat_role_service(role_service: &RoleServiceConfig) -> bool {
    is_qq_chat(&role_service.role_service_type)
}

/// Both QQ and workspace payloads carry a primary `llm_ref_id` at the top level.
pub fn primary_llm_ref_id(role_service_type: &RoleServiceType) -> Option<String> {
    optional_qq_chat(role_service_type)
        .and_then(|config| config.llm_ref_id)
        .or_else(|| optional_workspace(role_service_type).and_then(|config| config.llm_ref_id))
}

/// The retrieval-store connection and embedding model a service resolves against. Only QQ
/// and workspace payloads carry them; anything else resolves to a pair of `None`s.
pub fn retrieval_store_config(
    role_service: &RoleServiceConfig,
) -> (Option<String>, Option<String>) {
    if let Some(config) = optional_qq_chat(&role_service.role_service_type) {
        return (
            config.retrieval_store_connection_id().map(ToOwned::to_owned),
            config.embedding_model_ref_id,
        );
    }
    if let Some(config) = optional_workspace(&role_service.role_service_type) {
        return (
            config.retrieval_store_connection_id().map(ToOwned::to_owned),
            config.embedding_model_ref_id,
        );
    }
    (None, None)
}

/// RustFS connection the service caches its media into; preview URLs name it so the media proxy
/// reads from the same bucket the role service wrote to. Only QQ payloads configure it.
pub fn object_storage_connection_id(role_service: &RoleServiceConfig) -> Option<String> {
    optional_qq_chat(&role_service.role_service_type)
        .and_then(|config| config.rustfs_connection_id)
        .filter(|value| !value.trim().is_empty())
}
