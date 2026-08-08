use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use log::{info, warn};
use model_inference::message_content_utils::{is_transport_error, sanitize_messages_for_inference};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::tooling::ToolCalls;
use zihuan_core::llm::{InferenceParam, LLMMessage, MessagePart, MessageRole, StreamToken};
use zihuan_core::task_context::{
    scope_task_id, scope_task_runtime, AgentTaskRequest, AgentTaskResult, AgentTaskRuntime, AgentTaskStatus,
};
pub use zihuan_core::tool_runtime::ToolRunDuration;
use zihuan_core::workspace::AskUserRequest;

pub const MAX_TOOL_ITERATIONS: usize = 25;
const LOG_PREVIEW_CHARS: usize = 600;

thread_local! {
    static TOOL_PROGRESS_SCOPE_STACK: RefCell<Vec<ToolProgressScopeState>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Clone)]
struct ToolProgressScopeState {
    call_content: String,
    consumed: bool,
}

fn truncate_for_log(text: &str, max_chars: usize) -> String {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }

    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}...(truncated,total_chars={total_chars})")
}

fn format_cache_hit_rate(cached_prompt_tokens: Option<usize>, prompt_tokens: Option<usize>) -> String {
    match (cached_prompt_tokens, prompt_tokens) {
        (Some(cached), Some(prompt)) if prompt > 0 => {
            format!("{:.2}%", (cached as f64 / prompt as f64) * 100.0)
        }
        _ => "unavailable".to_string(),
    }
}

struct ToolProgressScopeGuard;

impl ToolProgressScopeGuard {
    fn enter(call_content: &str) -> Self {
        TOOL_PROGRESS_SCOPE_STACK.with(|stack| {
            stack.borrow_mut().push(ToolProgressScopeState {
                call_content: call_content.to_string(),
                consumed: false,
            });
        });
        Self
    }
}

impl Drop for ToolProgressScopeGuard {
    fn drop(&mut self) {
        TOOL_PROGRESS_SCOPE_STACK.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

pub fn consume_tool_progress_notification(call_content: &str) -> bool {
    let trimmed = call_content.trim();
    if trimmed.is_empty() {
        return false;
    }

    TOOL_PROGRESS_SCOPE_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        let Some(scope) = stack.last_mut() else {
            return true;
        };
        if scope.call_content.trim() != trimmed {
            return true;
        }
        if scope.consumed {
            return false;
        }
        scope.consumed = true;
        true
    })
}

pub fn current_task_progress_message(call_content: &str) -> Option<String> {
    let trimmed = call_content.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !consume_tool_progress_notification(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

/// Notification hook for long-running tool calls.
///
/// Purpose: host runtimes can expose task lifecycle updates to the user while
/// the Brain still waits synchronously for the real tool result.
pub trait LongTaskNotifier: Send + Sync + 'static {
    fn on_start(&self, _task_id: &str, _task_name: &str, _call_content: &str) {}

    fn on_complete(&self, _task_id: &str, _task_name: &str, _result: &str) {}
}

/// Context required to track long-running tools inside the Brain loop.
///
/// Purpose: carries the task runtime and notifier needed when a tool opts into
/// [`ToolRunDuration::Long`]. The Brain still returns the actual tool result to
/// the LLM in the same turn.
#[derive(Clone)]
pub struct LongTaskContext {
    pub task_runtime: Arc<dyn AgentTaskRuntime>,
    pub owner_id: Option<String>,
    pub agent_id: String,
    pub agent_name: String,
    pub notifier: Arc<dyn LongTaskNotifier>,
    pub task_db_connection_id: Option<String>,
}

/// A tool that [`Brain`] can invoke during an inference loop.
pub trait BrainTool: Send + Sync + 'static {
    /// Returns the LLM-facing function specification (name, description, parameters).
    fn spec(&self) -> Arc<dyn FunctionTool>;
    /// Execute the tool call. `call_content` is the assistant's text for this turn
    /// (used e.g. to send a progress notification before doing the actual work).
    fn execute(&self, call_content: &str, arguments: &Value) -> String;
    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        ToolExecutionOutput::text(self.execute(call_content, arguments))
    }
    /// Declares whether this tool should be treated as short or long running.
    /// Long tools may emit task lifecycle updates, but still execute
    /// synchronously so the LLM receives the real result immediately.
    fn run_duration(&self) -> ToolRunDuration {
        ToolRunDuration::Short
    }

    /// Describes the resource touched by a call so independent calls can run concurrently.
    /// Tools default to concurrent execution until they explicitly opt into a conflicting resource class.
    fn execution_resource(&self, _arguments: &Value) -> ToolExecutionResource {
        ToolExecutionResource::Concurrent
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolExecutionResource {
    Concurrent,
    Read(PathBuf),
    Write(PathBuf),
    Exclusive,
}

#[derive(Clone)]
struct PreparedToolCall {
    index: usize,
    call_id: String,
    name: String,
    arguments: Value,
    tool: Option<Arc<dyn BrainTool>>,
}

struct PreparedToolResult {
    index: usize,
    call_id: String,
    name: String,
    result: ToolExecutionOutput,
}

pub trait BrainObserver: Send + Sync + 'static {
    fn on_assistant_tool_request(&self, _iteration: usize, _content: &str, _tool_calls: &[ToolCalls]) {}

    fn on_tool_start(&self, _name: &str, _call_id: &str, _arguments: &Value) {}

    fn on_tool_finish(&self, _name: &str, _call_id: &str, _result: &str) {}

    fn on_ask_user(&self, _call_id: &str, _request: &AskUserRequest) {}

    fn on_final_assistant(&self, _response: &LLMMessage, _stop_reason: &BrainStopReason) {}
}

pub trait BrainIterationHook: Send + Sync + 'static {
    fn on_before_inference(&self, _iteration: usize, _conversation: &[LLMMessage]) -> Vec<LLMMessage> {
        Vec::new()
    }
}

/// The reason a [`Brain::run`] call returned.
#[derive(Debug)]
pub enum BrainStopReason {
    /// Normal completion: the last response had no tool calls.
    Done,
    /// Transport-level LLM error detected in response content.
    TransportError(String),
    /// Reached [`MAX_TOOL_ITERATIONS`] without a final assistant message.
    MaxIterationsReached,
    /// A tool needs follow-up user input before the next LLM iteration can continue.
    AwaitUserInput(AskUserRequest),
}

#[derive(Debug, Clone)]
pub struct ToolExecutionOutput {
    pub result: String,
    pub ask_user: Option<AskUserRequest>,
}

impl ToolExecutionOutput {
    pub fn text(result: impl Into<String>) -> Self {
        Self {
            result: result.into(),
            ask_user: None,
        }
    }

    pub fn ask_user(result: impl Into<String>, request: AskUserRequest) -> Self {
        Self {
            result: result.into(),
            ask_user: Some(request),
        }
    }
}

/// Orchestrates a multi-turn LLM ↔ tool call loop.
///
/// Create a `Brain`, register tools with [`Brain::with_tool`] or [`Brain::add_tool`],
/// then call [`Brain::run`] with the initial conversation messages.
pub struct Brain {
    llm: Arc<dyn LLMBase>,
    tools: Vec<Arc<dyn BrainTool>>,
    observer: Option<Arc<dyn BrainObserver>>,
    iteration_hook: Option<Arc<dyn BrainIterationHook>>,
    long_task_context: Option<LongTaskContext>,
}

impl Brain {
    pub fn new(llm: Arc<dyn LLMBase>) -> Self {
        Self {
            llm,
            tools: Vec::new(),
            observer: None,
            iteration_hook: None,
            long_task_context: None,
        }
    }

    /// Register a tool, consuming and returning `self` for builder-style chaining.
    pub fn with_tool(mut self, tool: impl BrainTool) -> Self {
        self.tools.push(Arc::new(tool));
        self
    }

    /// Register a tool in-place.
    pub fn add_tool(&mut self, tool: impl BrainTool) {
        self.tools.push(Arc::new(tool));
    }

    /// Attach a long-task execution context.
    pub fn set_long_task_context(&mut self, ctx: LongTaskContext) {
        self.long_task_context = Some(ctx);
    }

    pub fn with_observer(mut self, observer: Arc<dyn BrainObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn set_observer(&mut self, observer: Arc<dyn BrainObserver>) {
        self.observer = Some(observer);
    }

    pub fn with_iteration_hook(mut self, hook: Arc<dyn BrainIterationHook>) -> Self {
        self.iteration_hook = Some(hook);
        self
    }

    pub fn set_iteration_hook(&mut self, hook: Arc<dyn BrainIterationHook>) {
        self.iteration_hook = Some(hook);
    }

    /// Execute a single tool call, creating a tracked task entry when the tool's
    /// run duration is `Long` and a [`LongTaskContext`] is available.
    fn execute_tool_call_with_context(
        tool: &Arc<dyn BrainTool>,
        call_content: &str,
        arguments: &Value,
        tool_name: &str,
        long_task_context: Option<&LongTaskContext>,
    ) -> ToolExecutionOutput {
        if tool.run_duration() == ToolRunDuration::Long {
            if let Some(long_ctx) = long_task_context {
                let task_name = format!("工具: {tool_name}");
                let handle = long_ctx.task_runtime.start_task(AgentTaskRequest {
                    task_name: task_name.clone(),
                    agent_id: long_ctx.agent_id.clone(),
                    agent_name: long_ctx.agent_name.clone(),
                    user_ip: None,
                    owner_id: long_ctx.owner_id.clone(),
                    task_db_connection_id: long_ctx.task_db_connection_id.clone(),
                });
                let task_id = handle.task_id.clone();
                if let Some(progress_text) = current_task_progress_message(call_content) {
                    long_ctx.task_runtime.append_task_progress(&task_id, progress_text);
                }
                long_ctx.notifier.on_start(&task_id, &task_name, call_content);
                let result = scope_task_runtime(Arc::clone(&long_ctx.task_runtime), || {
                    scope_task_id(task_id.clone(), || tool.execute_with_outcome(call_content, arguments))
                });
                handle.finish(AgentTaskResult {
                    status: Some(AgentTaskStatus::Success),
                    result_summary: Some(result.result.clone()),
                    error_message: None,
                });
                long_ctx.notifier.on_complete(&task_id, &task_name, &result.result);
                info!("[Brain] tool '{}' completed as long task_id={}", tool_name, task_id);
                return result;
            }
        }
        tool.execute_with_outcome(call_content, arguments)
    }

    fn prepare_tool_calls(&self, tool_calls: &[ToolCalls]) -> Vec<PreparedToolCall> {
        tool_calls
            .iter()
            .enumerate()
            .map(|(index, call)| PreparedToolCall {
                index,
                call_id: call.id.clone(),
                name: call.function.name.clone(),
                arguments: call.function.arguments.clone(),
                tool: self
                    .tools
                    .iter()
                    .find(|tool| tool.spec().name() == call.function.name)
                    .cloned(),
            })
            .collect()
    }

    fn can_execute_in_parallel(calls: &[PreparedToolCall]) -> bool {
        if calls.len() < 2 {
            return false;
        }
        for call in calls {
            let Some(tool) = call.tool.as_ref() else {
                return false;
            };
            if matches!(tool.execution_resource(&call.arguments), ToolExecutionResource::Exclusive) {
                return false;
            }
        }
        for (left_index, left) in calls.iter().enumerate() {
            let Some(left_tool) = left.tool.as_ref() else {
                return false;
            };
            let left_resource = left_tool.execution_resource(&left.arguments);
            for right in calls.iter().skip(left_index + 1) {
                let Some(right_tool) = right.tool.as_ref() else {
                    return false;
                };
                let right_resource = right_tool.execution_resource(&right.arguments);
                if resources_conflict(&left_resource, &right_resource) {
                    return false;
                }
            }
        }
        true
    }

    fn execute_prepared_call(&self, call_content: &str, call: PreparedToolCall) -> PreparedToolResult {
        Self::execute_prepared_call_with_context(call_content, call, self.long_task_context.as_ref())
    }

    fn execute_prepared_call_with_context(
        call_content: &str,
        call: PreparedToolCall,
        long_task_context: Option<&LongTaskContext>,
    ) -> PreparedToolResult {
        let result = if let Some(tool) = call.tool.as_ref() {
            Self::execute_tool_call_with_context(
                tool,
                call_content,
                &call.arguments,
                &call.name,
                long_task_context,
            )
        } else {
            warn!(
                "[Brain] Tool '{}' not found for call id={} arguments={}",
                call.name, call.call_id, call.arguments
            );
            ToolExecutionOutput::text(
                serde_json::json!({"error": format!("Tool '{}' not found", call.name)}).to_string(),
            )
        };
        PreparedToolResult {
            index: call.index,
            call_id: call.call_id,
            name: call.name,
            result,
        }
    }

    fn notify_tool_finish(&self, call: &PreparedToolResult) {
        info!(
            "[Brain] tool call id={} name={} result: {}",
            call.call_id,
            call.name,
            truncate_for_log(&call.result.result, LOG_PREVIEW_CHARS)
        );
        if let Some(observer) = self.observer.as_ref() {
            observer.on_tool_finish(&call.name, &call.call_id, &call.result.result);
        }
    }

    fn append_tool_results(
        &self,
        results: &mut [PreparedToolResult],
        conversation: &mut Vec<LLMMessage>,
        output: &mut Vec<LLMMessage>,
    ) -> Option<(String, AskUserRequest)> {
        results.sort_by_key(|result| result.index);
        for call in results {
            let msg = LLMMessage::tool_result(call.call_id.clone(), call.result.result.clone());
            conversation.push(msg.clone());
            output.push(msg);
            if let Some(request) = call.result.ask_user.clone() {
                return Some((call.call_id.clone(), request));
            }
        }
        None
    }

    fn log_llm_usage(&self, response: &LLMMessage) {
        let Some(usage) = response.usage.as_ref() else {
            return;
        };

        if let Some(reasoning) = &response.reasoning_content {
            info!(
                "[Brain] llm reasoning ({} chars): {}",
                reasoning.len(),
                truncate_for_log(reasoning, LOG_PREVIEW_CHARS)
            );
        }

        info!(
            "[Brain] llm usage model={} prompt_tokens={} cached_prompt_tokens={} prompt_cache_miss_tokens={} completion_tokens={} total_tokens={} cache_hit_rate={}",
            self.llm.get_model_name(),
            usage
                .prompt_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            usage
                .cached_prompt_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            usage
                .prompt_cache_miss_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            usage
                .completion_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            usage
                .total_tokens
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            format_cache_hit_rate(
                usage.cached_prompt_tokens,
                usage.prompt_tokens.or_else(|| {
                    usage
                        .cached_prompt_tokens
                        .zip(usage.prompt_cache_miss_tokens)
                        .map(|(hit, miss)| hit + miss)
                }),
            ),
        );
    }

    /// Run the inference loop and return `(new_messages, stop_reason)`.
    ///
    /// `new_messages` contains all assistant and tool-result messages produced
    /// during this run. The caller's original `messages` are not included.
    pub fn run(&self, messages: Vec<LLMMessage>) -> (Vec<LLMMessage>, BrainStopReason) {
        let tool_specs: Vec<Arc<dyn FunctionTool>> = self.tools.iter().map(|t| t.spec()).collect();
        let mut conversation = sanitize_messages_for_inference(messages);
        let mut output: Vec<LLMMessage> = Vec::new();
        for iteration in 0..MAX_TOOL_ITERATIONS {
            if iteration > 0 {
                self.append_iteration_messages(iteration + 1, &mut conversation);
            }
            let is_last_iteration = iteration == MAX_TOOL_ITERATIONS - 1;

            if is_last_iteration {
                let counts = count_tool_calls(&conversation);
                append_tool_summary_to_system(&mut conversation, &counts);
            }

            let response = self.llm.inference(&InferenceParam {
                messages: &conversation,
                tools: if is_last_iteration || tool_specs.is_empty() {
                    None
                } else {
                    Some(&tool_specs)
                },
            });

            if let Some(content) = response.content_text() {
                if is_transport_error(content) {
                    warn!("[Brain] Transport error on iteration {iteration}: {content}");
                    let msg = content.to_string();
                    if let Some(observer) = self.observer.as_ref() {
                        observer.on_final_assistant(&response, &BrainStopReason::TransportError(msg.clone()));
                    }
                    output.push(response);
                    return (output, BrainStopReason::TransportError(msg));
                }
            }

            self.log_llm_usage(&response);

            if response.tool_calls.is_empty() {
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_final_assistant(&response, &BrainStopReason::Done);
                }
                output.push(response);
                return (output, BrainStopReason::Done);
            }

            if is_last_iteration {
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_final_assistant(&response, &BrainStopReason::MaxIterationsReached);
                }
                output.push(response);
                return (output, BrainStopReason::MaxIterationsReached);
            }

            let tool_call_content = response.content_text_owned().unwrap_or_default();
            if let Some(reasoning) = &response.reasoning_content {
                info!(
                    "[Brain] iteration {} reasoning ({} chars): {}",
                    iteration + 1,
                    reasoning.len(),
                    truncate_for_log(reasoning, LOG_PREVIEW_CHARS)
                );
            }
            if !tool_call_content.is_empty() {
                info!(
                    "[Brain] iteration {} assistant content: {}",
                    iteration + 1,
                    truncate_for_log(&tool_call_content, LOG_PREVIEW_CHARS)
                );
            }
            info!(
                "[Brain] iteration {} processing {} tool call(s)",
                iteration + 1,
                response.tool_calls.len()
            );
            if let Some(observer) = self.observer.as_ref() {
                observer.on_assistant_tool_request(iteration + 1, &tool_call_content, &response.tool_calls);
            }
            conversation.push(response.clone());
            output.push(response.clone());

            let _tool_progress_scope = ToolProgressScopeGuard::enter(&tool_call_content);
            let prepared_calls = self.prepare_tool_calls(&response.tool_calls);
            for call in &prepared_calls {
                info!(
                    "[Brain] tool call id={} name={} arguments={}",
                    call.call_id,
                    call.name,
                    truncate_for_log(&call.arguments.to_string(), LOG_PREVIEW_CHARS)
                );
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_tool_start(&call.name, &call.call_id, &call.arguments);
                }
            }

            let mut results: Vec<PreparedToolResult> = if Self::can_execute_in_parallel(&prepared_calls) {
                std::thread::scope(|scope| {
                    let handles = prepared_calls.into_iter().map(|call| {
                        let call_content = tool_call_content.clone();
                        scope.spawn(move || {
                            let result = self.execute_prepared_call(&call_content, call);
                            self.notify_tool_finish(&result);
                            result
                        })
                    });
                    handles
                        .map(|handle| handle.join().expect("parallel tool execution panicked"))
                        .collect::<Vec<PreparedToolResult>>()
                })
            } else {
                prepared_calls
                    .into_iter()
                    .map(|call| {
                        let result = self.execute_prepared_call(&tool_call_content, call);
                        self.notify_tool_finish(&result);
                        result
                    })
                    .collect::<Vec<PreparedToolResult>>()
            };

            if let Some((call_id, request)) = self.append_tool_results(&mut results, &mut conversation, &mut output) {
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_ask_user(&call_id, &request);
                }
                return (output, BrainStopReason::AwaitUserInput(request));
            }
        }

        warn!("[Brain] Tool loop exceeded max iterations ({MAX_TOOL_ITERATIONS})");
        (output, BrainStopReason::MaxIterationsReached)
    }

    pub async fn run_streaming(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
    ) -> (Vec<LLMMessage>, BrainStopReason) {
        let tool_specs: Vec<Arc<dyn FunctionTool>> = self.tools.iter().map(|t| t.spec()).collect();
        let mut conversation = sanitize_messages_for_inference(messages);
        let mut output: Vec<LLMMessage> = Vec::new();

        let streaming_llm = self.llm.as_streaming();

        for iteration in 0..MAX_TOOL_ITERATIONS {
            if iteration > 0 {
                self.append_iteration_messages(iteration + 1, &mut conversation);
            }
            let is_last_iteration = iteration == MAX_TOOL_ITERATIONS - 1;

            if is_last_iteration {
                let counts = count_tool_calls(&conversation);
                append_tool_summary_to_system(&mut conversation, &counts);
            }

            let tools_param: Option<&Vec<Arc<dyn FunctionTool>>> = if is_last_iteration || tool_specs.is_empty() {
                None
            } else {
                Some(&tool_specs)
            };

            let response = if let Some(streaming) = streaming_llm {
                streaming
                    .inference_streaming(
                        &InferenceParam {
                            messages: &conversation,
                            tools: tools_param,
                        },
                        token_tx.clone(),
                    )
                    .await
            } else {
                self.llm.inference(&InferenceParam {
                    messages: &conversation,
                    tools: tools_param,
                })
            };

            if let Some(content) = response.content_text() {
                if is_transport_error(content) {
                    warn!("[Brain] Transport error on iteration {iteration}: {content}");
                    let msg = content.to_string();
                    if let Some(observer) = self.observer.as_ref() {
                        observer.on_final_assistant(&response, &BrainStopReason::TransportError(msg.clone()));
                    }
                    output.push(response);
                    return (output, BrainStopReason::TransportError(msg));
                }
            }

            self.log_llm_usage(&response);

            if response.tool_calls.is_empty() {
                let response_preview = response.content_text_owned().unwrap_or_default();
                if !response_preview.is_empty() {
                    info!(
                        "[Brain] final assistant response: {}",
                        truncate_for_log(&response_preview, LOG_PREVIEW_CHARS)
                    );
                }
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_final_assistant(&response, &BrainStopReason::Done);
                }
                output.push(response);
                return (output, BrainStopReason::Done);
            }

            if is_last_iteration {
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_final_assistant(&response, &BrainStopReason::MaxIterationsReached);
                }
                output.push(response);
                return (output, BrainStopReason::MaxIterationsReached);
            }

            let tool_call_content = response.content_text_owned().unwrap_or_default();
            if let Some(reasoning) = &response.reasoning_content {
                info!(
                    "[Brain] iteration {} reasoning ({} chars): {}",
                    iteration + 1,
                    reasoning.len(),
                    truncate_for_log(reasoning, LOG_PREVIEW_CHARS)
                );
            }
            if !tool_call_content.is_empty() {
                info!(
                    "[Brain] iteration {} assistant content: {}",
                    iteration + 1,
                    truncate_for_log(&tool_call_content, LOG_PREVIEW_CHARS)
                );
            }
            info!(
                "[Brain] iteration {} processing {} tool call(s)",
                iteration + 1,
                response.tool_calls.len()
            );
            if let Some(observer) = self.observer.as_ref() {
                observer.on_assistant_tool_request(iteration + 1, &tool_call_content, &response.tool_calls);
            }
            conversation.push(response.clone());
            output.push(response.clone());

            let _tool_progress_scope = ToolProgressScopeGuard::enter(&tool_call_content);
            let prepared_calls = self.prepare_tool_calls(&response.tool_calls);
            for call in &prepared_calls {
                info!(
                    "[Brain] tool call id={} name={} arguments={}",
                    call.call_id,
                    call.name,
                    truncate_for_log(&call.arguments.to_string(), LOG_PREVIEW_CHARS)
                );
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_tool_start(&call.name, &call.call_id, &call.arguments);
                }
            }

            let mut results: Vec<PreparedToolResult> = if Self::can_execute_in_parallel(&prepared_calls) {
                let long_task_context = self.long_task_context.clone();
                let mut tasks = JoinSet::new();
                for call in prepared_calls {
                    let call_content = tool_call_content.clone();
                    let long_task_context = long_task_context.clone();
                    tasks.spawn_blocking(move || {
                        Self::execute_prepared_call_with_context(
                            &call_content,
                            call,
                            long_task_context.as_ref(),
                        )
                    });
                }
                let mut results = Vec::new();
                while let Some(result) = tasks.join_next().await {
                    let result = result.expect("parallel streaming tool execution panicked");
                    self.notify_tool_finish(&result);
                    results.push(result);
                }
                results
            } else {
                prepared_calls
                    .into_iter()
                    .map(|call| {
                        let result = self.execute_prepared_call(&tool_call_content, call);
                        self.notify_tool_finish(&result);
                        result
                    })
                    .collect::<Vec<PreparedToolResult>>()
            };

            if let Some((call_id, request)) = self.append_tool_results(&mut results, &mut conversation, &mut output) {
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_ask_user(&call_id, &request);
                }
                return (output, BrainStopReason::AwaitUserInput(request));
            }
        }

        warn!("[Brain] Tool loop exceeded max iterations ({MAX_TOOL_ITERATIONS})");
        (output, BrainStopReason::MaxIterationsReached)
    }

    fn append_iteration_messages(&self, iteration: usize, conversation: &mut Vec<LLMMessage>) {
        let Some(hook) = self.iteration_hook.as_ref() else {
            return;
        };

        let mut appended = hook.on_before_inference(iteration, conversation);
        if appended.is_empty() {
            return;
        }

        info!(
            "[Brain] iteration {} appended {} external message(s) before inference",
            iteration,
            appended.len()
        );
        conversation.append(&mut appended);
    }
}

fn resources_conflict(left: &ToolExecutionResource, right: &ToolExecutionResource) -> bool {
    match (left, right) {
        (ToolExecutionResource::Concurrent, _) | (_, ToolExecutionResource::Concurrent) => false,
        (ToolExecutionResource::Read(_), ToolExecutionResource::Read(_)) => false,
        (ToolExecutionResource::Write(left), ToolExecutionResource::Write(right))
        | (ToolExecutionResource::Read(left), ToolExecutionResource::Write(right))
        | (ToolExecutionResource::Write(left), ToolExecutionResource::Read(right)) => {
            left == right || left.starts_with(right) || right.starts_with(left)
        }
        (ToolExecutionResource::Exclusive, _) | (_, ToolExecutionResource::Exclusive) => true,
    }
}

/// Count tool calls already present in `messages` by tool name.
fn count_tool_calls(messages: &[LLMMessage]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for msg in messages {
        for tc in &msg.tool_calls {
            *counts.entry(tc.function.name.clone()).or_insert(0) += 1;
        }
    }
    counts
}

/// Append a tool-call summary to the first system message in `messages`,
/// or push a new system message if none exists.
fn append_tool_summary_to_system(messages: &mut Vec<LLMMessage>, counts: &HashMap<String, usize>) {
    if counts.is_empty() {
        return;
    }

    let mut items: Vec<_> = counts.iter().collect();
    items.sort_by(|a, b| a.0.cmp(b.0));
    let lines: Vec<String> = items.iter().map(|(name, count)| format!("  - {name}: {count} 次")).collect();
    let summary = format!(
        "工具调用次数已达上限。目前已调用的工具及次数如下：\n{}\n\n请基于已获取的信息直接作答，不再调用任何工具。",
        lines.join("\n")
    );

    for msg in messages.iter_mut() {
        if matches!(msg.role, MessageRole::System) {
            if let Some(MessagePart::Text { text }) = msg.parts.first_mut() {
                text.push('\n');
                text.push('\n');
                text.push_str(&summary);
                return;
            }
            msg.parts.push(MessagePart::text(summary));
            return;
        }
    }

    messages.push(LLMMessage::system(summary));
}

#[cfg(test)]
mod tests {
    use super::*;
    use zihuan_core::llm::tooling::StaticFunctionToolSpec;

    #[derive(Debug)]
    struct TestTool {
        name: &'static str,
        resource: ToolExecutionResource,
    }

    impl BrainTool for TestTool {
        fn spec(&self) -> Arc<dyn FunctionTool> {
            Arc::new(StaticFunctionToolSpec {
                name: self.name,
                description: "test tool",
                parameters: serde_json::json!({"type": "object"}),
            })
        }

        fn execute(&self, _call_content: &str, _arguments: &Value) -> String {
            "ok".to_string()
        }

        fn execution_resource(&self, _arguments: &Value) -> ToolExecutionResource {
            self.resource.clone()
        }
    }

    fn prepared(index: usize, tool: TestTool, arguments: Value) -> PreparedToolCall {
        PreparedToolCall {
            index,
            call_id: format!("call-{index}"),
            name: tool.name.to_string(),
            arguments,
            tool: Some(Arc::new(tool)),
        }
    }

    #[test]
    fn independent_reads_can_run_in_parallel() {
        let calls = vec![
            prepared(0, TestTool { name: "read_a", resource: ToolExecutionResource::Read("a".into()) }, Value::Null),
            prepared(1, TestTool { name: "read_b", resource: ToolExecutionResource::Read("b".into()) }, Value::Null),
        ];
        assert!(Brain::can_execute_in_parallel(&calls));
    }

    #[test]
    fn overlapping_read_write_calls_are_serialized() {
        let calls = vec![
            prepared(0, TestTool { name: "read", resource: ToolExecutionResource::Read("workspace".into()) }, Value::Null),
            prepared(1, TestTool { name: "write", resource: ToolExecutionResource::Write("workspace/file.txt".into()) }, Value::Null),
        ];
        assert!(!Brain::can_execute_in_parallel(&calls));
    }

    #[test]
    fn exclusive_calls_are_serialized() {
        let calls = vec![
            prepared(0, TestTool { name: "one", resource: ToolExecutionResource::Concurrent }, Value::Null),
            prepared(1, TestTool { name: "two", resource: ToolExecutionResource::Exclusive }, Value::Null),
        ];
        assert!(!Brain::can_execute_in_parallel(&calls));
    }

    #[test]
    fn tool_results_are_appended_in_original_call_order() {
        let brain = Brain {
            llm: panic_llm_for_test(),
            tools: Vec::new(),
            observer: None,
            iteration_hook: None,
            long_task_context: None,
        };
        let mut results = vec![
            PreparedToolResult {
                index: 1,
                call_id: "call-1".to_string(),
                name: "second".to_string(),
                result: ToolExecutionOutput::text("second result"),
            },
            PreparedToolResult {
                index: 0,
                call_id: "call-0".to_string(),
                name: "first".to_string(),
                result: ToolExecutionOutput::text("first result"),
            },
        ];
        let mut conversation = Vec::new();
        let mut output = Vec::new();
        assert!(brain.append_tool_results(&mut results, &mut conversation, &mut output).is_none());
        assert_eq!(conversation[0].tool_call_id.as_deref(), Some("call-0"));
        assert_eq!(conversation[1].tool_call_id.as_deref(), Some("call-1"));
    }

    fn panic_llm_for_test() -> Arc<dyn LLMBase> {
        #[derive(Debug)]
        struct UnusedLlm;
        impl LLMBase for UnusedLlm {
            fn get_model_name(&self) -> &str {
                "test"
            }

            fn inference(&self, _param: &InferenceParam) -> LLMMessage {
                panic!("unused test LLM")
            }

            fn as_streaming(&self) -> Option<&dyn zihuan_core::llm::llm_base::StreamingLLMBase> {
                None
            }
        }
        Arc::new(UnusedLlm)
    }
}
