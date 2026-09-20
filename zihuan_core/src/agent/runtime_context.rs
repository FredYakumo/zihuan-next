use std::cell::RefCell;
use std::sync::Arc;

use crate::agent::resource_provider::SharedAgentResourceProvider;
use crate::error::{Error, Result};

#[derive(Clone)]
pub struct AgentRuntimeContext {
    pub resources: SharedAgentResourceProvider,
}

impl AgentRuntimeContext {
    pub fn from_resources(resources: SharedAgentResourceProvider) -> Self {
        Self { resources }
    }
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

/// Clone the runtime context currently installed on this thread, if any.
///
/// Callers that will later execute work on a different thread (e.g. a tool
/// engine spawning workers) capture the context here and re-enter it with
/// [`scope_agent_runtime_context`], because thread-locals do not cross threads.
pub fn current_agent_runtime_context() -> Option<AgentRuntimeContext> {
    CURRENT_AGENT_RUNTIME_CONTEXT.with(|slot| slot.borrow().last().cloned())
}

/// Run `f` with `context` installed, or plainly when it is `None`.
pub fn scope_agent_runtime_context<T>(
    context: Option<AgentRuntimeContext>,
    f: impl FnOnce() -> T,
) -> T {
    match context {
        Some(context) => with_current_agent_runtime_context(context, f),
        None => f(),
    }
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
