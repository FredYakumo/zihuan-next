use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::model_inference::llm::tooling::FunctionTool;
pub use crate::tool_runtime::ToolRunDuration;
use crate::workspace::AskUserRequest;

pub mod memory;
pub mod node_graph_tool;
pub mod plugin_tool;
pub mod tool_calling_engine;
pub mod tool_calling_types;
pub(crate) mod tool_progress;
pub mod web_search;

pub use memory::{
    register_memory_tools, MemoryAgentResources, MemoryBackend, MemoryRecall, RecalledMemory,
};
pub use node_graph_tool::NodeGraphTool;
pub use plugin_tool::{
    plugin_brain_tool, plugin_brain_tools, selected_plugin_brain_tools, PluginTool,
};
pub use tool_calling_engine::{ToolCallingEngine, MAX_TOOL_ITERATIONS};
pub use tool_calling_types::{
    AgentExecutor, LongTaskContext, LongTaskNotifier, NestedToolCallingEvent,
    ToolCallingMiddleware, ToolCallingObserver, ToolCallingRequest, ToolCallingResult,
    ToolCallingStopReason,
};
pub use tool_progress::{consume_tool_progress_notification, current_task_progress_message};
pub use web_search::WebSearchTool;

pub type AsyncToolOutput =
    std::pin::Pin<Box<dyn std::future::Future<Output = ToolExecutionOutput> + Send + 'static>>;

/// A tool that can be invoked during an inference loop.
pub trait Tool: Send + Sync + 'static {
    /// Read-only availability for the next model request. Execution must still enforce limits.
    fn is_available(&self) -> bool {
        true
    }
    /// Reserve execution quota before the engine creates a long task or sends notifications.
    /// Return a rejection without running the tool when admission fails.
    fn prepare_execution(&self) -> Option<ToolExecutionOutput> {
        None
    }
    /// Execute after successful admission, without reserving the same quota again.
    fn execute_prepared_with_progress(
        &self,
        call_content: &str,
        arguments: &Value,
        on_output: Arc<dyn Fn(&str, &str) + Send + Sync>,
    ) -> ToolExecutionOutput {
        self.execute_with_progress(call_content, arguments, on_output)
    }
    /// Returns the LLM-facing function specification (name, description, parameters).
    fn spec(&self) -> Arc<dyn FunctionTool>;
    /// Execute the tool call. `call_content` is the assistant's text for this turn.
    /// Ordinary failures are downgraded for the model. Safe runtime control results and
    /// tools that explicitly opt into failure details retain their guidance.
    fn execute(&self, call_content: &str, arguments: &Value) -> Result<String>;
    /// Optional asynchronous execution for tools that await external task results.
    /// The engine retains the conversation and tool-call position across this future.
    fn execute_async(&self, _content: &str, _arguments: &Value) -> Option<AsyncToolOutput> {
        None
    }
    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        match self.execute(call_content, arguments) {
            Ok(text) => ToolExecutionOutput::text(text),
            Err(error) => ToolExecutionOutput::text(tool_failure_envelope(error)),
        }
    }

    /// Whether this tool's failure payloads are guidance the calling model must receive
    /// verbatim — command stderr, edit conflicts, path validation. Default `false`: the
    /// tool loop logs the failure and downgrades it to `<tool> 执行失败` for the model.
    fn returns_failure_details_to_agent(&self) -> bool {
        false
    }
    fn execute_with_progress(
        &self,
        call_content: &str,
        arguments: &Value,
        _on_output: Arc<dyn Fn(&str, &str) + Send + Sync>,
    ) -> ToolExecutionOutput {
        self.execute_with_outcome(call_content, arguments)
    }
    /// Declares whether this tool should be treated as short or long running.
    fn run_duration(&self) -> ToolRunDuration {
        ToolRunDuration::Short
    }
    /// Describes the resource touched by a call so independent calls can run concurrently.
    fn execution_resource(&self, _arguments: &Value) -> ToolExecutionResource {
        ToolExecutionResource::Concurrent
    }
    /// Declares whether this call blocks waiting for user confirmation. The
    /// engine forces such calls to run serially so that at most one
    /// confirmation dialog is shown at a time.
    fn requires_user_confirmation(&self, _arguments: &Value) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolExecutionResource {
    Concurrent,
    Read(PathBuf),
    Write(PathBuf),
    Exclusive,
}

#[derive(Debug, Clone)]
pub struct ToolExecutionOutput {
    pub result: String,
    pub ask_user: Option<AskUserRequest>,
    /// Trusted runtime control, never inferred from arbitrary tool-returned JSON.
    pub control: Option<ToolControlResult>,
}

#[derive(Debug, Clone, Copy)]
pub enum ToolControlResult {
    PermissionDenied,
    QuotaExhausted,
}

/// Detects the shared tool failure envelopes: `{"ok":false,...}` and the workspace
/// tool family's `{"error":"..."}` (only when `ok` is absent, so an `ok:true` payload
/// carrying an `error` field stays a success).
///
/// Tools report failures inside a successful string return, so the engine uses this to
/// record the task as failed and downgrade the payload for the calling model.
pub fn tool_result_reports_failure(result: &str) -> bool {
    let Some(value) = serde_json::from_str::<Value>(result.trim()).ok() else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    match object.get("ok").and_then(Value::as_bool) {
        Some(ok) => !ok,
        None => matches!(object.get("error"), Some(Value::String(_))),
    }
}

/// Builds the shared tool failure envelope from a raw error. Used by the trait bridge
/// that turns a tool's propagated `Err` into the string shape the engine detects.
pub fn tool_failure_envelope(error: impl std::fmt::Display) -> String {
    serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
}

impl ToolExecutionOutput {
    pub fn text(result: impl Into<String>) -> Self {
        Self {
            result: result.into(),
            ask_user: None,
            control: None,
        }
    }

    /// The message is passed to the calling model verbatim; include only safe guidance.
    pub fn control(control: ToolControlResult, message: impl Into<String>) -> Self {
        let code = match control {
            ToolControlResult::PermissionDenied => "PERMISSION_DENIED",
            ToolControlResult::QuotaExhausted => "TOOL_QUOTA_EXHAUSTED",
        };
        let message = message.into();
        Self {
            result: serde_json::json!({
                "ok": false,
                "code": code,
                "retryable": false,
                "error": message,
            })
            .to_string(),
            ask_user: None,
            control: Some(control),
        }
    }

    pub fn ask_user(result: impl Into<String>, request: AskUserRequest) -> Self {
        Self {
            result: result.into(),
            ask_user: Some(request),
            control: None,
        }
    }
}
