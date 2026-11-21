use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use zihuan_core::agent::tools::{
    tool_failure_envelope, Tool, ToolControlResult, ToolExecutionOutput,
};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::Result;
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::tool_runtime::ToolRunDuration;

use super::tool_quota_store::{decrement_tool_quota_if_needed_blocking, get_tool_quota};

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
        ToolExecutionOutput::control(
            ToolControlResult::QuotaExhausted,
            format!("{message}。请停止调用该工具，修改参数不会恢复额度；根据已有结果完成回复。"),
        )
    }

    fn try_acquire(
        quota: &QqChatToolQuotaContext,
        tool_name: &str,
    ) -> std::result::Result<(), ToolExecutionOutput> {
        {
            let mut state = quota.session_state.lock().unwrap();
            if let Some(limit) = quota.limit_for(tool_name) {
                if state.get(tool_name) >= limit {
                    return Err(Self::rejection_output(Self::limit_message(
                        quota,
                        TOOL_LIMIT_SCOPE_SESSION,
                    )));
                }
            }
            state.increment(tool_name);
        }

        let Some(rdb_pool) = quota.rdb_pool.as_ref() else {
            return Ok(());
        };

        let allowed = decrement_tool_quota_if_needed_blocking(
            rdb_pool,
            &quota.agent_id,
            &quota.sender_id,
            tool_name,
        );
        match allowed {
            Ok(true) => Ok(()),
            result => {
                let mut state = quota.session_state.lock().unwrap();
                if let Some(count) = state.counts.get_mut(tool_name) {
                    *count = count.saturating_sub(1);
                }
                Err(match result {
                    Ok(false) => {
                        Self::rejection_output(Self::limit_message(quota, TOOL_LIMIT_SCOPE_USER))
                    }
                    Err(error) => ToolExecutionOutput::text(tool_failure_envelope(error)),
                    Ok(true) => unreachable!(),
                })
            }
        }
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
    fn is_available(&self) -> bool {
        if !self.tool.is_available() {
            return false;
        }
        let Some(quota) = &self.quota else {
            return true;
        };
        let tool_name = self.tool.spec().name().to_string();
        if quota
            .limit_for(&tool_name)
            .is_some_and(|limit| quota.session_state.lock().unwrap().get(&tool_name) >= limit)
        {
            return false;
        }
        let Some(connection) = &quota.rdb_pool else {
            return true;
        };
        match zihuan_core::runtime::block_async(get_tool_quota(
            connection,
            &quota.agent_id,
            &quota.sender_id,
            &tool_name,
        )) {
            Ok(Some(record)) => record.remaining_calls.is_none_or(|remaining| remaining > 0),
            _ => true,
        }
    }

    fn prepare_execution(&self) -> Option<ToolExecutionOutput> {
        if let Some(quota) = &self.quota {
            let tool_name = self.tool.spec().name().to_string();
            if let Err(rejection) = Self::try_acquire(quota, &tool_name) {
                return Some(rejection);
            }
        }
        self.tool.prepare_execution()
    }

    fn execute_prepared_with_progress(
        &self,
        call_content: &str,
        arguments: &Value,
        on_output: Arc<dyn Fn(&str, &str) + Send + Sync>,
    ) -> ToolExecutionOutput {
        self.tool.execute_prepared_with_progress(call_content, arguments, on_output)
    }

    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.tool.spec()
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> Result<String> {
        Ok(self.execute_with_outcome(call_content, arguments).result)
    }

    fn returns_failure_details_to_agent(&self) -> bool {
        self.tool.returns_failure_details_to_agent()
    }

    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        if let Some(rejection) = self.prepare_execution() {
            return rejection;
        }
        self.execute_prepared_with_progress(call_content, arguments, Arc::new(|_, _| {}))
    }

    fn run_duration(&self) -> ToolRunDuration {
        self.tool.run_duration()
    }
}
