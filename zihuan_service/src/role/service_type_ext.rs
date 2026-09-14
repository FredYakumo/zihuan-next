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

/// 由已构造的具体配置对象构建擦除类型。配置均为纯 serde 结构体，
/// 序列化为对象不可能失败，故此处不可失败。
pub fn qq_chat_from(config: QqChatRoleServiceConfig) -> RoleServiceType {
    RoleServiceType::from_typed_config(RoleServiceKind::QqChat, &config)
        .expect("qq_chat role service config must serialize to an object")
}

/// 见 [`qq_chat_from`]。
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

pub fn is_workspace_agent(agent: &RoleServiceConfig) -> bool {
    is_workspace(&agent.role_service_type)
}

pub fn is_qq_chat_agent(agent: &RoleServiceConfig) -> bool {
    is_qq_chat(&agent.role_service_type)
}

/// Both QQ and workspace payloads carry a primary `llm_ref_id` at the top level.
pub fn primary_llm_ref_id(role_service_type: &RoleServiceType) -> Option<String> {
    optional_qq_chat(role_service_type)
        .and_then(|config| config.llm_ref_id)
        .or_else(|| optional_workspace(role_service_type).and_then(|config| config.llm_ref_id))
}
