use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::AgentContext;
use crate::llm::tooling::ToolCalls;
use crate::llm::LLMMessage;
use crate::task_context::AgentTaskRuntime;
use crate::workspace::AskUserRequest;

#[derive(Debug, Clone)]
pub struct ToolCallingRequest {
    pub messages: Vec<LLMMessage>,
}

#[derive(Debug)]
pub struct ToolCallingResult {
    pub messages: Vec<LLMMessage>,
    pub stop_reason: ToolCallingStopReason,
}

#[async_trait]
pub trait AgentExecutor: Send + Sync {
    async fn execute(&self, context: AgentContext, request: ToolCallingRequest) -> crate::error::Result<ToolCallingResult>;
}

pub trait LongTaskNotifier: Send + Sync + 'static {
    fn on_start(&self, _task_id: &str, _task_name: &str, _call_content: &str) {}
    fn on_complete(&self, _task_id: &str, _task_name: &str, _result: &str) {}
}

#[derive(Clone)]
pub struct LongTaskContext {
    pub task_runtime: Arc<dyn AgentTaskRuntime>,
    pub owner_id: Option<String>,
    pub agent_id: String,
    pub agent_name: String,
    pub notifier: Arc<dyn LongTaskNotifier>,
    pub task_db_connection_id: Option<String>,
}

pub trait ToolCallingObserver: Send + Sync + 'static {
    fn on_assistant_tool_request(&self, _iteration: usize, _content: &str, _tool_calls: &[ToolCalls]) {}
    fn on_tool_start(&self, _name: &str, _call_id: &str, _arguments: &Value) {}
    fn on_tool_output(&self, _name: &str, _call_id: &str, _stream: &str, _chunk: &str) {}
    fn on_tool_finish(&self, _name: &str, _call_id: &str, _result: &str) {}
    fn on_ask_user(&self, _call_id: &str, _request: &AskUserRequest) {}
    fn on_final_assistant(&self, _response: &LLMMessage, _stop_reason: &ToolCallingStopReason) {}
}

pub trait ToolCallingMiddleware: Send + Sync + 'static {
    fn on_before_inference(&self, _iteration: usize, _conversation: &[LLMMessage]) -> Vec<LLMMessage> {
        Vec::new()
    }
}

#[derive(Debug)]
pub enum ToolCallingStopReason {
    Done,
    TransportError(String),
    MaxIterationsReached,
    AwaitUserInput(AskUserRequest),
}
