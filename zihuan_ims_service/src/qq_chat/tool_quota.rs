use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use zihuan_core::agent::tools::{Tool, ToolExecutionOutput};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::tool_runtime::ToolRunDuration;

use super::tool_quota_store::decrement_tool_quota_if_needed_blocking;

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

    pub fn reset(&mut self) {
        self.counts.clear();
    }
}

#[derive(Clone)]
pub(crate) struct QqChatToolQuotaContext {
    pub agent_id: String,
    pub sender_id: String,
    pub rdb_pool: Option<RelationalDbConnection>,
    pub session_limits: HashMap<String, usize>,
    pub session_limit_message: Option<String>,
    pub session_state: Arc<Mutex<SessionToolQuotaState>>,
}

impl QqChatToolQuotaContext {
    pub fn limit_for(&self, tool_name: &str) -> Option<usize> {
        self.session_limits.get(tool_name).copied().filter(|limit| *limit > 0)
    }
}

pub(crate) struct QuotaMaybeWrappedTool<T> {
    tool: T,
    quota: Option<QqChatToolQuotaContext>,
}

impl<T> QuotaMaybeWrappedTool<T> {
    fn limit_message(quota: &QqChatToolQuotaContext, scope: &str) -> String {
        match quota.session_limit_message.as_deref() {
            Some(msg) if !msg.is_empty() => msg.replace("{limit_scope}", scope),
            _ => TOOL_LIMIT_MESSAGE_TEMPLATE.replace("{limit_scope}", scope),
        }
    }

    fn rejection_output(message: String) -> ToolExecutionOutput {
        ToolExecutionOutput::text(serde_json::json!({ "ok": false, "error": message }).to_string())
    }

    fn try_acquire(quota: &QqChatToolQuotaContext, tool_name: &str) -> Result<(), String> {
        if let Some(limit) = quota.limit_for(tool_name) {
            let current = quota.session_state.lock().unwrap().get(tool_name);
            if current >= limit {
                return Err(Self::limit_message(quota, TOOL_LIMIT_SCOPE_SESSION));
            }
        }

        let Some(rdb_pool) = quota.rdb_pool.as_ref() else {
            return Ok(());
        };

        let allowed = decrement_tool_quota_if_needed_blocking(
            rdb_pool,
            &quota.agent_id,
            &quota.sender_id,
            tool_name,
        )
        .map_err(|err| err.to_string())?;
        if !allowed {
            return Err(Self::limit_message(quota, TOOL_LIMIT_SCOPE_USER));
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
) -> impl Tool
where
    T: Tool,
{
    QuotaMaybeWrappedTool { tool, quota }
}

impl<T> Tool for QuotaMaybeWrappedTool<T>
where
    T: Tool,
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
                return Self::rejection_output(message);
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
