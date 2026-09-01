use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::model_inference::llm::tooling::FunctionTool;
use crate::workspace::AskUserRequest;
pub use crate::tool_runtime::ToolRunDuration;

pub mod tool_calling_engine;
pub mod tool_calling_types;
pub(crate) mod tool_progress;
pub mod web_search;

pub use tool_calling_engine::{ToolCallingEngine, MAX_TOOL_ITERATIONS};
pub use tool_calling_types::{
    AgentExecutor, LongTaskContext, LongTaskNotifier, ToolCallingMiddleware, ToolCallingObserver,
    ToolCallingRequest, ToolCallingResult, ToolCallingStopReason,
};
pub use tool_progress::{consume_tool_progress_notification, current_task_progress_message};
pub use web_search::WebSearchTool;

/// A tool that can be invoked during an inference loop.
pub trait Tool: Send + Sync + 'static {
    /// Returns the LLM-facing function specification (name, description, parameters).
    fn spec(&self) -> Arc<dyn FunctionTool>;
    /// Execute the tool call. `call_content` is the assistant's text for this turn.
    fn execute(&self, call_content: &str, arguments: &Value) -> String;
    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        ToolExecutionOutput::text(self.execute(call_content, arguments))
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

impl ToolExecutionOutput {
    pub fn text(result: impl Into<String>) -> Self {
        Self { result: result.into(), ask_user: None }
    }

    pub fn ask_user(result: impl Into<String>, request: AskUserRequest) -> Self {
        Self { result: result.into(), ask_user: Some(request) }
    }
}
