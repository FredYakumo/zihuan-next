use async_trait::async_trait;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleServiceKind {
    QqChat,
    Workspace,
}

/// metadata for a configured, externally reachable role service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleServiceDescriptor {
    pub id: String,
    pub name: String,
    pub kind: RoleServiceKind,
}

/// request-scoped metadata owned by a RoleService.
///
/// Transport-specific state belongs in the role's typed input and output values.
#[derive(Debug, Clone, Default)]
pub struct RoleServiceContext {
    pub request_id: Option<String>,
    pub session_id: Option<String>,
}

/// Lifecycle owner for one externally reachable role.
///
/// A RoleService owns channel resources and invokes its internal agents at the
/// appropriate lifecycle points. Transports only adapt external protocols to
/// the service's typed input and output.
#[async_trait]
pub trait RoleService: Send + Sync {
    type Input: Send;
    type Output: Send;

    fn descriptor(&self) -> RoleServiceDescriptor;

    async fn handle(&self, context: RoleServiceContext, input: Self::Input)
        -> Result<Self::Output>;
}
