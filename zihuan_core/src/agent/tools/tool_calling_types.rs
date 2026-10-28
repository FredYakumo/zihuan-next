use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::AgentContext;
use crate::model_inference::llm::tooling::ToolCalls;
use crate::model_inference::llm::LLMMessage;
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
    async fn execute(
        &self,
        context: AgentContext,
        request: ToolCallingRequest,
    ) -> crate::error::Result<ToolCallingResult>;
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

/// One step inside a nested agent's own tool-calling loop.
///
/// Nested agents run their own [`ToolCallingEngine`](super::tool_calling_engine::ToolCallingEngine),
/// which has no direct link to the turn that started it. These events travel through
/// [`ToolCallingObserver::on_nested_event`] together with the chain of enclosing tool calls, so
/// the outer observer can show them under the sub-agent call that produced them.
#[derive(Debug, Clone)]
pub enum NestedToolCallingEvent {
    Input {
        system_prompt: String,
        user_prompt: String,
        tools: Vec<Value>,
    },
    /// A delta of the nested agent's streaming response. The complete text follows in an
    /// [`Self::Assistant`] event, which settles the message the deltas built up.
    AssistantChunk { content: String, reasoning: String },
    /// The nested agent's model produced a complete assistant message.
    Assistant {
        /// Zero-based position of the response in the nested loop; `None` for responses the
        /// engine reports without an iteration number.
        iteration: Option<usize>,
        content: String,
        reasoning: Option<String>,
        /// The response ended the loop (it carried no tool calls).
        final_response: bool,
    },
    ToolStarted {
        call_id: String,
        name: String,
        arguments: Value,
    },
    ToolFinished {
        call_id: String,
        name: String,
        result: String,
        duration_ms: u128,
    },
}

thread_local! {
    /// Observer of the enclosing tool loop, installed while a tool body runs. A nested agent
    /// reads it to report its own steps under the tool call that started it.
    static CURRENT_NESTED_OBSERVER: RefCell<Vec<NestedObserverScope>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
pub(crate) struct NestedObserverScope {
    observer: Arc<dyn ToolCallingObserver>,
    path: Vec<String>,
}

/// The observer and enclosing call path visible on this thread, if a tool is running.
pub(crate) fn current_nested_observer() -> Option<(Arc<dyn ToolCallingObserver>, Vec<String>)> {
    CURRENT_NESTED_OBSERVER.with(|slot| {
        slot.borrow()
            .last()
            .map(|scope| (Arc::clone(&scope.observer), scope.path.clone()))
    })
}

/// Run `f` with the nested-observer scope installed, or plainly when there is none.
pub(crate) fn scope_nested_observer<T>(
    scope: Option<NestedObserverScope>,
    f: impl FnOnce() -> T,
) -> T {
    let Some(scope) = scope else {
        return f();
    };
    CURRENT_NESTED_OBSERVER.with(|slot| slot.borrow_mut().push(scope));
    let result = f();
    CURRENT_NESTED_OBSERVER.with(|slot| {
        slot.borrow_mut().pop();
    });
    result
}

/// Builds the scope a tool call hands to a nested agent: the enclosing observer plus the
/// absolute path of enclosing call ids ending with this call.
pub(crate) fn nested_observer_scope(
    observer: Option<&Arc<dyn ToolCallingObserver>>,
    observer_path: &[String],
    call_id: &str,
) -> Option<NestedObserverScope> {
    let observer = observer?;
    let mut path = observer_path.to_vec();
    path.push(call_id.to_string());
    Some(NestedObserverScope { observer: Arc::clone(observer), path })
}

pub trait ToolCallingObserver: Send + Sync + 'static {
    fn on_assistant_tool_request(
        &self,
        _iteration: usize,
        _content: &str,
        _tool_calls: &[ToolCalls],
    ) {
    }
    fn on_tool_start(&self, _name: &str, _call_id: &str, _arguments: &Value) {}
    fn on_tool_output(&self, _name: &str, _call_id: &str, _stream: &str, _chunk: &str) {}
    fn on_tool_finish(&self, _name: &str, _call_id: &str, _result: &str) {}
    fn on_ask_user(&self, _call_id: &str, _request: &AskUserRequest) {}
    fn on_final_assistant(&self, _response: &LLMMessage, _stop_reason: &ToolCallingStopReason) {}

    /// One assistant response of this loop, tool-requesting or final. Unlike
    /// [`Self::on_assistant_tool_request`] this also carries the model's reasoning and is
    /// called for the response that ends the loop, which is what a nested agent reports.
    fn on_assistant_message(&self, _iteration: usize, _response: &LLMMessage) {}

    /// One inference iteration finished. `elapsed` is the wall time of the model call and
    /// `first_token` how long until the first streamed token (unavailable for blocking calls).
    /// Observers that surface throughput to a UI can use these to compute tokens per second.
    fn on_inference_timing(
        &self,
        _iteration: usize,
        _elapsed: Duration,
        _first_token: Option<Duration>,
    ) {
    }

    /// One step of a nested agent that a tool call of this observer started. `path` is the
    /// chain of enclosing tool call ids, outermost first, ending with the sub-agent call that
    /// owns the event.
    fn on_nested_event(&self, _path: &[String], _event: &NestedToolCallingEvent) {}
}

pub trait ToolCallingMiddleware: Send + Sync + 'static {
    fn on_before_inference(
        &self,
        _iteration: usize,
        _conversation: &[LLMMessage],
    ) -> Vec<LLMMessage> {
        Vec::new()
    }
}

#[derive(Debug, Clone)]
pub enum ToolCallingStopReason {
    Done,
    TransportError(String),
    MaxIterationsReached,
    AwaitUserInput(AskUserRequest),
    ToolCallLimitReached(AskUserRequest),
}
