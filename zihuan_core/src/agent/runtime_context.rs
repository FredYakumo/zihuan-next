use std::cell::RefCell;
use std::sync::Arc;

use crate::agent::resource_provider::SharedAgentResourceProvider;
use crate::error::{Error, Result};

#[derive(Clone)]
pub struct AgentRuntimeContext {
    pub resources: SharedAgentResourceProvider,
}

thread_local! {
    static CURRENT_AGENT_RUNTIME_CONTEXT: RefCell<Vec<AgentRuntimeContext>> = const { RefCell::new(Vec::new()) };
}

pub fn with_current_agent_runtime_context<T>(
    context: AgentRuntimeContext,
    f: impl FnOnce() -> T,
) -> T {
    CURRENT_AGENT_RUNTIME_CONTEXT.with(|slot| slot.borrow_mut().push(context));
    let result = f();
    CURRENT_AGENT_RUNTIME_CONTEXT.with(|slot| {
        slot.borrow_mut().pop();
    });
    result
}

pub fn current_agent_resources() -> Result<SharedAgentResourceProvider> {
    CURRENT_AGENT_RUNTIME_CONTEXT.with(|slot| {
        slot.borrow()
            .last()
            .map(|context| Arc::clone(&context.resources))
            .ok_or_else(|| {
                Error::ValidationError(
                    "当前节点不在 Agent 工具调用上下文中，无法读取 Agent 配置".to_string(),
                )
            })
    })
}
