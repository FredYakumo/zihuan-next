use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use zihuan_agent::brain::{BrainTool, ToolExecutionOutput};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::tool_runtime::ToolRunDuration;

use super::qq_chat_agent_service_tool_quota_store::decrement_tool_quota_if_needed_blocking;

pub(crate) const TOOL_LIMIT_MESSAGE_TEMPLATE: &str =
    "当前工具已经达到 [{limit_scope}]的调用次数限制，但是你不要把这个直接当成自然语言输出给用户，仅当用户问及详细原因时，再告知";
pub(crate) const TOOL_LIMIT_SCOPE_SESSION: &str = "单次会话";
pub(crate) const TOOL_LIMIT_SCOPE_USER: &str = "用户";

#[derive(Debug, Default)]
pub(crate) struct SessionToolQuotaState {
    counts: HashMap<String, usize>,
}

impl SessionToolQuotaState {
    fn get(&self, tool_name: &str) -> usize {
        self.counts.get(tool_name).copied().unwrap_or(0)
    }

    fn increment(&mut self, tool_name: &str) {
        *self.counts.entry(tool_name.to_string()).or_insert(0) += 1;
    }
}

#[derive(Clone)]
pub(crate) struct QqChatToolQuotaContext {
    pub agent_id: String,
    pub sender_id: String,
    pub rdb_pool: Option<RelationalDbConnection>,
    pub session_limits: HashMap<String, usize>,
    pub session_state: Arc<Mutex<SessionToolQuotaState>>,
}

impl QqChatToolQuotaContext {
    pub fn limit_for(&self, tool_name: &str) -> Option<usize> {
        self.session_limits.get(tool_name).copied().filter(|limit| *limit > 0)
    }
}

pub(crate) struct QuotaMaybeWrappedBrainTool<T> {
    tool: T,
    quota: Option<QqChatToolQuotaContext>,
}

impl<T> QuotaMaybeWrappedBrainTool<T> {
    fn limit_message(scope: &str) -> String {
        TOOL_LIMIT_MESSAGE_TEMPLATE.replace("{limit_scope}", scope)
    }

    fn try_acquire(quota: &QqChatToolQuotaContext, tool_name: &str) -> Result<(), String> {
        if let Some(limit) = quota.limit_for(tool_name) {
            let current = quota.session_state.lock().unwrap().get(tool_name);
            if current >= limit {
                return Err(Self::limit_message(TOOL_LIMIT_SCOPE_SESSION));
            }
        }

        let Some(rdb_pool) = quota.rdb_pool.as_ref() else {
            return Ok(());
        };

        let allowed = decrement_tool_quota_if_needed_blocking(rdb_pool, &quota.agent_id, &quota.sender_id, tool_name)
            .map_err(|err| err.to_string())?;
        if !allowed {
            return Err(Self::limit_message(TOOL_LIMIT_SCOPE_USER));
        }

        Ok(())
    }

    fn record_session_usage(quota: &QqChatToolQuotaContext, tool_name: &str) {
        quota.session_state.lock().unwrap().increment(tool_name);
    }
}

pub(crate) fn wrap_brain_tool_with_quota<T>(
    tool: T,
    quota: Option<QqChatToolQuotaContext>,
) -> impl BrainTool
where
    T: BrainTool,
{
    QuotaMaybeWrappedBrainTool { tool, quota }
}

impl<T> BrainTool for QuotaMaybeWrappedBrainTool<T>
where
    T: BrainTool,
{
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.tool.spec()
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.execute_with_outcome(call_content, arguments).result
    }

    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        if let Some(quota) = &self.quota {
            let tool_name = self.tool.spec().name().to_string();
            if let Err(message) = Self::try_acquire(quota, &tool_name) {
                return ToolExecutionOutput::text(message);
            }

            let output = self.tool.execute_with_outcome(call_content, arguments);
            Self::record_session_usage(quota, &tool_name);
            return output;
        }

        self.tool.execute_with_outcome(call_content, arguments)
    }

    fn run_duration(&self) -> ToolRunDuration {
        self.tool.run_duration()
    }
}

#[cfg(test)]
mod tests {
    use std::fmt;
    use std::sync::Arc;

    use serde_json::json;

    use super::{
        wrap_brain_tool_with_quota, QqChatToolQuotaContext, SessionToolQuotaState, TOOL_LIMIT_SCOPE_SESSION,
        TOOL_LIMIT_SCOPE_USER,
    };
    use zihuan_agent::brain::BrainTool;
    use zihuan_core::llm::tooling::FunctionTool;

    #[derive(Debug)]
    struct EchoTool;

    impl BrainTool for EchoTool {
        fn spec(&self) -> Arc<dyn FunctionTool> {
            Arc::new(EchoToolSpec)
        }

        fn execute(&self, _call_content: &str, _arguments: &serde_json::Value) -> String {
            "ok".to_string()
        }
    }

    struct EchoToolSpec;

    impl fmt::Debug for EchoToolSpec {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("EchoToolSpec").finish()
        }
    }

    impl FunctionTool for EchoToolSpec {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "echo"
        }

        fn parameters(&self) -> serde_json::Value {
            json!({"type": "object"})
        }

        fn call(&self, _arguments: serde_json::Value) -> zihuan_core::error::Result<serde_json::Value> {
            Ok(json!({}))
        }
    }

    #[test]
    fn session_limit_blocks_second_call() {
        let quota = QqChatToolQuotaContext {
            agent_id: "agent".to_string(),
            sender_id: "sender".to_string(),
            rdb_pool: None,
            session_limits: [("echo".to_string(), 1usize)].into_iter().collect(),
            session_state: Arc::new(std::sync::Mutex::new(SessionToolQuotaState::default())),
        };
        let tool = wrap_brain_tool_with_quota(EchoTool, Some(quota));

        let first = tool.execute("", &json!({}));
        let second = tool.execute("", &json!({}));

        assert_eq!(first, "ok");
        assert!(second.contains(TOOL_LIMIT_SCOPE_SESSION));
    }

    #[test]
    fn no_quota_context_keeps_tool_unlimited() {
        let tool = wrap_brain_tool_with_quota(EchoTool, None);
        assert_eq!(tool.execute("", &json!({})), "ok");
        assert_eq!(tool.execute("", &json!({})), "ok");
    }

    #[test]
    fn limit_message_mentions_user_scope_label() {
        let message = super::QuotaMaybeWrappedBrainTool::<EchoTool>::limit_message(TOOL_LIMIT_SCOPE_USER);
        assert!(message.contains(TOOL_LIMIT_SCOPE_USER));
    }
}
