use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;

/// Stable metadata for a domain agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDescriptor {
    pub id: &'static str,
    pub name: &'static str,
    pub capabilities: Vec<&'static str>,
}

impl AgentDescriptor {
    pub fn new(id: &'static str, name: &'static str, capabilities: Vec<&'static str>) -> Self {
        Self {
            id,
            name,
            capabilities,
        }
    }
}

/// Request-scoped services shared by domain agents.
///
/// Domain-specific inputs belong in each agent's `Input` type. This context
/// intentionally only carries correlation and cancellation concerns.
#[derive(Clone, Default)]
pub struct AgentContext {
    pub request_id: Option<String>,
    pub session_id: Option<String>,
    pub parent_agent_id: Option<String>,
    pub cancellation: Option<Arc<dyn AgentCancellation>>,
}

impl AgentContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.as_ref().is_some_and(|cancellation| cancellation.is_cancelled())
    }
}

pub trait AgentCancellation: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

#[async_trait]
pub trait Agent: Send + Sync {
    type Input: Send;
    type Output: Send;

    fn descriptor(&self) -> AgentDescriptor;

    async fn run(&self, context: AgentContext, input: Self::Input) -> Result<Self::Output>;
}
