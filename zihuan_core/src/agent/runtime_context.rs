use std::cell::RefCell;

use crate::agent::qq_chat::QqChatAgentServiceConfig;
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub enum AgentRuntimeContext {
    QqChat(QqChatAgentServiceConfig),
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

pub fn current_qq_chat_agent_service_config() -> Result<QqChatAgentServiceConfig> {
    CURRENT_AGENT_RUNTIME_CONTEXT.with(|slot| match slot.borrow().last() {
        Some(AgentRuntimeContext::QqChat(config)) => Ok(config.clone()),
        None => Err(Error::ValidationError(
            "当前节点不在 Agent 工具调用上下文中，无法读取 Agent 配置".to_string(),
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(bot_name: &str) -> QqChatAgentServiceConfig {
        QqChatAgentServiceConfig {
            ims_bot_adapter_connection_id: "adapter".to_string(),
            bot_name: bot_name.to_string(),
            web_search_engine_connection_id: "search".to_string(),
            ..serde_json::from_value(serde_json::json!({
                "ims_bot_adapter_connection_id": "adapter",
                "web_search_engine_connection_id": "search"
            }))
            .expect("minimal QQ config must deserialize")
        }
    }

    #[test]
    fn qq_context_is_required_and_nested_context_is_restored() {
        assert!(current_qq_chat_agent_service_config().is_err());
        let outer = config("outer");
        let inner = config("inner");

        with_current_agent_runtime_context(AgentRuntimeContext::QqChat(outer), || {
            assert_eq!(current_qq_chat_agent_service_config().unwrap().bot_name, "outer");
            with_current_agent_runtime_context(AgentRuntimeContext::QqChat(inner), || {
                assert_eq!(current_qq_chat_agent_service_config().unwrap().bot_name, "inner");
            });
            assert_eq!(current_qq_chat_agent_service_config().unwrap().bot_name, "outer");
        });
        assert!(current_qq_chat_agent_service_config().is_err());
    }
}
