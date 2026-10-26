use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::model_inference::llm::tooling::FunctionTool;
pub use crate::tool_runtime::ToolRunDuration;
use crate::workspace::AskUserRequest;

pub mod memory_tools;
pub mod node_graph_tool;
pub mod tool_calling_engine;
pub mod tool_calling_types;
pub(crate) mod tool_progress;
pub mod web_search;

pub use memory_tools::{register_memory_tools, MemoryAgentResources, MemoryBackend};
pub use node_graph_tool::NodeGraphTool;
pub use tool_calling_engine::{ToolCallingEngine, MAX_TOOL_ITERATIONS};
pub use tool_calling_types::{
    AgentExecutor, LongTaskContext, LongTaskNotifier, NestedToolCallingEvent,
    ToolCallingMiddleware, ToolCallingObserver, ToolCallingRequest, ToolCallingResult,
    ToolCallingStopReason,
};
pub use tool_progress::{consume_tool_progress_notification, current_task_progress_message};
pub use web_search::WebSearchTool;

/// A tool that can be invoked during an inference loop.
pub trait Tool: Send + Sync + 'static {
    /// Returns the LLM-facing function specification (name, description, parameters).
    fn spec(&self) -> Arc<dyn FunctionTool>;
    /// Execute the tool call. `call_content` is the assistant's text for this turn.
    /// Failures propagate natively: the tool loop records them and hands the calling
    /// model only a downgraded notice, so error details never reach the conversation.
    fn execute(&self, call_content: &str, arguments: &Value) -> Result<String>;
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
        Self { result: result.into(), ask_user: None }
    }

    pub fn ask_user(result: impl Into<String>, request: AskUserRequest) -> Self {
        Self {
            result: result.into(),
            ask_user: Some(request),
        }
    }
}
