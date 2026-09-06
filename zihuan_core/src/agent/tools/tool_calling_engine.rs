use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::model_inference::message_content_utils::{
    is_transport_error, sanitize_messages_for_inference,
};
use async_trait::async_trait;
use log::{info, warn};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use super::tool_calling_types::{
    AgentExecutor, LongTaskContext, ToolCallingMiddleware, ToolCallingObserver, ToolCallingRequest,
    ToolCallingResult, ToolCallingStopReason,
};
use super::tool_progress::{current_task_progress_message, ToolProgressScopeGuard};
use crate::agent::tools::{Tool, ToolExecutionOutput, ToolExecutionResource, ToolRunDuration};
use crate::agent::{AgentCancellation, AgentContext};
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::tooling::FunctionTool;
use crate::model_inference::llm::tooling::ToolCalls;
use crate::model_inference::llm::{
    InferenceParam, LLMMessage, MessagePart, MessageRole, StreamToken,
};
use crate::task_context::{
    scope_task_id, scope_task_runtime, AgentTaskRequest, AgentTaskResult, AgentTaskStatus,
};
use crate::workspace::{AskUserRequest, ToolCallLimitRequest};

pub const MAX_TOOL_ITERATIONS: usize = 25;
const LOG_PREVIEW_CHARS: usize = 600;

fn truncate_for_log(text: &str, max_chars: usize) -> String {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }

    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}...(truncated,total_chars={total_chars})")
}

fn format_cache_hit_rate(
    cached_prompt_tokens: Option<usize>,
    prompt_tokens: Option<usize>,
) -> String {
    match (cached_prompt_tokens, prompt_tokens) {
        (Some(cached), Some(prompt)) if prompt > 0 => {
            format!("{:.2}%", (cached as f64 / prompt as f64) * 100.0)
        }
        _ => "unavailable".to_string(),
    }
}

#[derive(Clone)]
struct PreparedToolCall {
    index: usize,
    call_id: String,
    name: String,
    arguments: Value,
    tool: Option<Arc<dyn Tool>>,
}

struct PreparedToolResult {
    index: usize,
    call_id: String,
    name: String,
    result: ToolExecutionOutput,
}

fn tool_output_callback(
    observer: Option<&Arc<dyn ToolCallingObserver>>,
    tool_name: &str,
    call_id: &str,
) -> Arc<dyn Fn(&str, &str) + Send + Sync> {
    let Some(observer) = observer else {
        return Arc::new(|_, _| {});
    };
    let observer = Arc::clone(observer);
    let tool_name = tool_name.to_string();
    let call_id = call_id.to_string();
    Arc::new(move |stream, chunk| observer.on_tool_output(&tool_name, &call_id, stream, chunk))
}

/// Orchestrates a multi-turn LLM tool call loop.
///
/// Create a `ToolCallingEngine`, register tools with [`ToolCallingEngine::with_tool`] or [`ToolCallingEngine::add_tool`],
/// then call [`ToolCallingEngine::run`] with the initial conversation messages.
pub struct ToolCallingEngine {
    llm: Arc<dyn LLMBase>,
    tools: Vec<Arc<dyn Tool>>,
    observer: Option<Arc<dyn ToolCallingObserver>>,
    iteration_hook: Option<Arc<dyn ToolCallingMiddleware>>,
    long_task_context: Option<LongTaskContext>,
    cancellation: Option<Arc<dyn AgentCancellation>>,
    confirmation_gate: Arc<Mutex<()>>,
}

impl ToolCallingEngine {
    pub fn new(llm: Arc<dyn LLMBase>) -> Self {
        Self {
            llm,
            tools: Vec::new(),
            observer: None,
            iteration_hook: None,
            long_task_context: None,
            cancellation: None,
            confirmation_gate: Arc::new(Mutex::new(())),
        }
    }

    /// Register a tool, consuming and returning `self` for builder-style chaining.
    pub fn with_tool(mut self, tool: impl Tool) -> Self {
        self.tools.push(Arc::new(tool));
        self
    }

    /// Register a tool in-place.
    pub fn add_tool(&mut self, tool: impl Tool) {
        self.tools.push(Arc::new(tool));
    }

    /// Attach a long-task execution context.
    pub fn set_long_task_context(&mut self, ctx: LongTaskContext) {
        self.long_task_context = Some(ctx);
    }

    pub fn set_cancellation(&mut self, cancellation: Arc<dyn AgentCancellation>) {
        self.cancellation = Some(cancellation);
    }

    fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|cancellation| cancellation.is_cancelled())
    }

    pub fn with_observer(mut self, observer: Arc<dyn ToolCallingObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn set_observer(&mut self, observer: Arc<dyn ToolCallingObserver>) {
        self.observer = Some(observer);
    }

    pub fn with_iteration_hook(mut self, hook: Arc<dyn ToolCallingMiddleware>) -> Self {
        self.iteration_hook = Some(hook);
        self
    }

    pub fn set_iteration_hook(&mut self, hook: Arc<dyn ToolCallingMiddleware>) {
        self.iteration_hook = Some(hook);
    }

    /// Execute a single tool call, creating a tracked task entry when the tool's
    /// run duration is `Long` and a [`LongTaskContext`] is available.
    fn execute_tool_call_with_context(
        tool: &Arc<dyn Tool>,
        call_content: &str,
        arguments: &Value,
        tool_name: &str,
        call_id: &str,
        observer: Option<&Arc<dyn ToolCallingObserver>>,
        long_task_context: Option<&LongTaskContext>,
        confirmation_gate: Option<&Mutex<()>>,
    ) -> ToolExecutionOutput {
        // Calls that block waiting for user confirmation must run serially so
        // that at most one confirmation dialog is shown at a time. The guard is
        // held across registration, confirmation event emission, and the wait.
        let _confirmation_guard = if tool.requires_user_confirmation(arguments) {
            confirmation_gate
                .map(|gate| gate.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
        } else {
            None
        };
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
                let on_output = tool_output_callback(observer, tool_name, call_id);
                let result = scope_task_runtime(Arc::clone(&long_ctx.task_runtime), || {
                    scope_task_id(task_id.clone(), || {
                        tool.execute_with_progress(call_content, arguments, on_output)
                    })
                });
                handle.finish(AgentTaskResult {
                    status: Some(AgentTaskStatus::Success),
                    result_summary: Some(result.result.clone()),
                    error_message: None,
                });
                long_ctx.notifier.on_complete(&task_id, &task_name, &result.result);
                info!(
                    "[ToolCallingEngine] tool '{}' completed as long task_id={}",
                    tool_name, task_id
                );
                return result;
            }
        }
        let on_output = tool_output_callback(observer, tool_name, call_id);
        tool.execute_with_progress(call_content, arguments, on_output)
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
            if tool.requires_user_confirmation(&call.arguments) {
                return false;
            }
        }
        for call in calls {
            let Some(tool) = call.tool.as_ref() else {
                return false;
            };
            if matches!(tool.execution_resource(&call.arguments), ToolExecutionResource::Exclusive)
            {
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

    fn execute_prepared_call(
        &self,
        call_content: &str,
        call: PreparedToolCall,
    ) -> PreparedToolResult {
        if self.is_cancelled() {
            return PreparedToolResult {
                index: call.index,
                call_id: call.call_id,
                name: call.name,
                result: ToolExecutionOutput::text("tool execution cancelled".to_string()),
            };
        }
        Self::execute_prepared_call_with_context(
            call_content,
            call,
            self.long_task_context.as_ref(),
            self.observer.as_ref(),
            Some(&self.confirmation_gate),
        )
    }

    fn execute_prepared_call_with_context(
        call_content: &str,
        call: PreparedToolCall,
        long_task_context: Option<&LongTaskContext>,
        observer: Option<&Arc<dyn ToolCallingObserver>>,
        confirmation_gate: Option<&Mutex<()>>,
    ) -> PreparedToolResult {
        let result = if let Some(tool) = call.tool.as_ref() {
            Self::execute_tool_call_with_context(
                tool,
                call_content,
                &call.arguments,
                &call.name,
                &call.call_id,
                observer,
                long_task_context,
                confirmation_gate,
            )
        } else {
            warn!(
                "[ToolCallingEngine] Tool '{}' not found for call id={} arguments={}",
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
            "[ToolCallingEngine] tool call id={} name={} result: {}",
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
                "[ToolCallingEngine] llm reasoning ({} chars): {}",
                reasoning.len(),
                truncate_for_log(reasoning, LOG_PREVIEW_CHARS)
            );
        }

        info!(
            "[ToolCallingEngine] llm usage model={} prompt_tokens={} cached_prompt_tokens={} prompt_cache_miss_tokens={} completion_tokens={} total_tokens={} cache_hit_rate={}",
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
    pub fn run(&self, messages: Vec<LLMMessage>) -> (Vec<LLMMessage>, ToolCallingStopReason) {
        let tool_specs: Vec<Arc<dyn FunctionTool>> = self.tools.iter().map(|t| t.spec()).collect();
        let mut conversation = sanitize_messages_for_inference(messages);
        let mut output: Vec<LLMMessage> = Vec::new();
        for iteration in 0..MAX_TOOL_ITERATIONS {
            if self.is_cancelled() {
                return (
                    output,
                    ToolCallingStopReason::TransportError(
                        "tool-calling execution cancelled".to_string(),
                    ),
                );
            }
            if iteration > 0 {
                self.append_iteration_messages(iteration + 1, &mut conversation);
            }
            let response = self.llm.inference(&InferenceParam {
                messages: &conversation,
                tools: if tool_specs.is_empty() {
                    None
                } else {
                    Some(&tool_specs)
                },
            });

            if let Some(content) = response.content_text() {
                if is_transport_error(content) {
                    warn!(
                        "[ToolCallingEngine] Transport error on iteration {iteration}: {content}"
                    );
                    let msg = content.to_string();
                    if let Some(observer) = self.observer.as_ref() {
                        observer.on_final_assistant(
                            &response,
                            &ToolCallingStopReason::TransportError(msg.clone()),
                        );
                    }
                    output.push(response);
                    return (output, ToolCallingStopReason::TransportError(msg));
                }
            }

            self.log_llm_usage(&response);

            if response.tool_calls.is_empty() {
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_final_assistant(&response, &ToolCallingStopReason::Done);
                }
                output.push(response);
                return (output, ToolCallingStopReason::Done);
            }

            let tool_call_content = response.content_text_owned().unwrap_or_default();
            if let Some(reasoning) = &response.reasoning_content {
                info!(
                    "[ToolCallingEngine] iteration {} reasoning ({} chars): {}",
                    iteration + 1,
                    reasoning.len(),
                    truncate_for_log(reasoning, LOG_PREVIEW_CHARS)
                );
            }
            if !tool_call_content.is_empty() {
                info!(
                    "[ToolCallingEngine] iteration {} assistant content: {}",
                    iteration + 1,
                    truncate_for_log(&tool_call_content, LOG_PREVIEW_CHARS)
                );
            }
            info!(
                "[ToolCallingEngine] iteration {} processing {} tool call(s)",
                iteration + 1,
                response.tool_calls.len()
            );
            if let Some(observer) = self.observer.as_ref() {
                observer.on_assistant_tool_request(
                    iteration + 1,
                    &tool_call_content,
                    &response.tool_calls,
                );
            }
            conversation.push(response.clone());
            output.push(response.clone());

            let _tool_progress_scope = ToolProgressScopeGuard::enter(&tool_call_content);
            if self.is_cancelled() {
                return (
                    output,
                    ToolCallingStopReason::TransportError(
                        "tool-calling execution cancelled".to_string(),
                    ),
                );
            }
            let prepared_calls = self.prepare_tool_calls(&response.tool_calls);
            for call in &prepared_calls {
                info!(
                    "[ToolCallingEngine] tool call id={} name={} arguments={}",
                    call.call_id,
                    call.name,
                    truncate_for_log(&call.arguments.to_string(), LOG_PREVIEW_CHARS)
                );
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_tool_start(&call.name, &call.call_id, &call.arguments);
                }
            }

            let mut results: Vec<PreparedToolResult> =
                if Self::can_execute_in_parallel(&prepared_calls) {
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
                            let result = std::thread::scope(|scope| {
                                scope
                                    .spawn(|| self.execute_prepared_call(&tool_call_content, call))
                                    .join()
                                    .expect("serial tool execution panicked")
                            });
                            self.notify_tool_finish(&result);
                            result
                        })
                        .collect::<Vec<PreparedToolResult>>()
                };

            if let Some((call_id, request)) =
                self.append_tool_results(&mut results, &mut conversation, &mut output)
            {
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_ask_user(&call_id, &request);
                }
                return (output, ToolCallingStopReason::AwaitUserInput(request));
            }
            if iteration + 1 == MAX_TOOL_ITERATIONS {
                let request = AskUserRequest {
                    question: "工具调用已达到本段上限，是否继续执行？".to_string(),
                    details: Some(format!("本段已执行 {MAX_TOOL_ITERATIONS} 次工具调用。")),
                    placeholder: None,
                    command_confirmation: None,
                    tool_call_limit: Some(ToolCallLimitRequest { used_calls: MAX_TOOL_ITERATIONS }),
                };
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_ask_user("tool_call_limit", &request);
                }
                return (output, ToolCallingStopReason::ToolCallLimitReached(request));
            }
        }

        warn!("[ToolCallingEngine] Tool loop exceeded max iterations ({MAX_TOOL_ITERATIONS})");
        (output, ToolCallingStopReason::MaxIterationsReached)
    }

    pub async fn run_streaming(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
    ) -> (Vec<LLMMessage>, ToolCallingStopReason) {
        let tool_specs: Vec<Arc<dyn FunctionTool>> = self.tools.iter().map(|t| t.spec()).collect();
        let mut conversation = sanitize_messages_for_inference(messages);
        let mut output: Vec<LLMMessage> = Vec::new();

        let streaming_llm = self.llm.as_streaming();

        for iteration in 0..MAX_TOOL_ITERATIONS {
            if self.is_cancelled() {
                return (
                    output,
                    ToolCallingStopReason::TransportError(
                        "tool-calling execution cancelled".to_string(),
                    ),
                );
            }
            if iteration > 0 {
                self.append_iteration_messages(iteration + 1, &mut conversation);
            }
            let tools_param: Option<&Vec<Arc<dyn FunctionTool>>> = if tool_specs.is_empty() {
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
                    warn!(
                        "[ToolCallingEngine] Transport error on iteration {iteration}: {content}"
                    );
                    let msg = content.to_string();
                    if let Some(observer) = self.observer.as_ref() {
                        observer.on_final_assistant(
                            &response,
                            &ToolCallingStopReason::TransportError(msg.clone()),
                        );
                    }
                    output.push(response);
                    return (output, ToolCallingStopReason::TransportError(msg));
                }
            }

            self.log_llm_usage(&response);

            if response.tool_calls.is_empty() {
                let response_preview = response.content_text_owned().unwrap_or_default();
                if !response_preview.is_empty() {
                    info!(
                        "[ToolCallingEngine] final assistant response: {}",
                        truncate_for_log(&response_preview, LOG_PREVIEW_CHARS)
                    );
                }
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_final_assistant(&response, &ToolCallingStopReason::Done);
                }
                output.push(response);
                return (output, ToolCallingStopReason::Done);
            }

            let tool_call_content = response.content_text_owned().unwrap_or_default();
            if let Some(reasoning) = &response.reasoning_content {
                info!(
                    "[ToolCallingEngine] iteration {} reasoning ({} chars): {}",
                    iteration + 1,
                    reasoning.len(),
                    truncate_for_log(reasoning, LOG_PREVIEW_CHARS)
                );
            }
            if !tool_call_content.is_empty() {
                info!(
                    "[ToolCallingEngine] iteration {} assistant content: {}",
                    iteration + 1,
                    truncate_for_log(&tool_call_content, LOG_PREVIEW_CHARS)
                );
            }
            info!(
                "[ToolCallingEngine] iteration {} processing {} tool call(s)",
                iteration + 1,
                response.tool_calls.len()
            );
            if let Some(observer) = self.observer.as_ref() {
                observer.on_assistant_tool_request(
                    iteration + 1,
                    &tool_call_content,
                    &response.tool_calls,
                );
            }
            conversation.push(response.clone());
            output.push(response.clone());

            let _tool_progress_scope = ToolProgressScopeGuard::enter(&tool_call_content);
            if self.is_cancelled() {
                return (
                    output,
                    ToolCallingStopReason::TransportError(
                        "tool-calling execution cancelled".to_string(),
                    ),
                );
            }
            let prepared_calls = self.prepare_tool_calls(&response.tool_calls);
            for call in &prepared_calls {
                info!(
                    "[ToolCallingEngine] tool call id={} name={} arguments={}",
                    call.call_id,
                    call.name,
                    truncate_for_log(&call.arguments.to_string(), LOG_PREVIEW_CHARS)
                );
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_tool_start(&call.name, &call.call_id, &call.arguments);
                }
            }

            let mut results: Vec<PreparedToolResult> =
                if Self::can_execute_in_parallel(&prepared_calls) {
                    let long_task_context = self.long_task_context.clone();
                    let observer_handle = self.observer.clone();
                    let confirmation_gate = Arc::clone(&self.confirmation_gate);
                    let cancellation = self.cancellation.clone();
                    let mut tasks = JoinSet::new();
                    for call in prepared_calls {
                        // Stop dispatching further parallel tool calls as soon as a
                        // cancellation is requested. The worker path below bypasses the
                        // serial-path `execute_prepared_call` guard, so without this check
                        // every tool in a parallel wave runs to completion after a stop.
                        if self.is_cancelled() {
                            return (
                                output,
                                ToolCallingStopReason::TransportError(
                                    "tool-calling execution cancelled".to_string(),
                                ),
                            );
                        }
                        let call_content = tool_call_content.clone();
                        let long_task_context = long_task_context.clone();
                        let task_observer = observer_handle.clone();
                        let gate = Arc::clone(&confirmation_gate);
                        let cancellation_guard = cancellation.clone();
                        tasks.spawn_blocking(move || {
                            // Re-check inside the worker: a call that has not yet started
                            // its real work short-circuits instead of running.
                            if cancellation_guard
                                .as_ref()
                                .is_some_and(|cancellation| cancellation.is_cancelled())
                            {
                                return PreparedToolResult {
                                    index: call.index,
                                    call_id: call.call_id,
                                    name: call.name,
                                    result: ToolExecutionOutput::text(
                                        "tool execution cancelled".to_string(),
                                    ),
                                };
                            }
                            Self::execute_prepared_call_with_context(
                                &call_content,
                                call,
                                long_task_context.as_ref(),
                                task_observer.as_ref(),
                                Some(&*gate),
                            )
                        });
                    }
                    let mut results = Vec::new();
                    while let Some(result) = tasks.join_next().await {
                        let result = result.expect("parallel streaming tool execution panicked");
                        self.notify_tool_finish(&result);
                        results.push(result);
                        // Abort the wave promptly once a stop is requested instead of
                        // draining every parallel tool call. Dropping the JoinSet cancels
                        // queued workers; a worker already running on the blocking pool is
                        // left to settle on its own.
                        if self.is_cancelled() {
                            return (
                                output,
                                ToolCallingStopReason::TransportError(
                                    "tool-calling execution cancelled".to_string(),
                                ),
                            );
                        }
                    }
                    results
                } else {
                    prepared_calls
                        .into_iter()
                        .map(|call| {
                            let result = std::thread::scope(|scope| {
                                scope
                                    .spawn(|| self.execute_prepared_call(&tool_call_content, call))
                                    .join()
                                    .expect("serial tool execution panicked")
                            });
                            self.notify_tool_finish(&result);
                            result
                        })
                        .collect::<Vec<PreparedToolResult>>()
                };

            if let Some((call_id, request)) =
                self.append_tool_results(&mut results, &mut conversation, &mut output)
            {
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_ask_user(&call_id, &request);
                }
                return (output, ToolCallingStopReason::AwaitUserInput(request));
            }
            if iteration + 1 == MAX_TOOL_ITERATIONS {
                let request = AskUserRequest {
                    question: "工具调用已达到本段上限，是否继续执行？".to_string(),
                    details: Some(format!("本段已执行 {MAX_TOOL_ITERATIONS} 次工具调用。")),
                    placeholder: None,
                    command_confirmation: None,
                    tool_call_limit: Some(ToolCallLimitRequest { used_calls: MAX_TOOL_ITERATIONS }),
                };
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_ask_user("tool_call_limit", &request);
                }
                return (output, ToolCallingStopReason::ToolCallLimitReached(request));
            }
        }

        warn!("[ToolCallingEngine] Tool loop exceeded max iterations ({MAX_TOOL_ITERATIONS})");
        (output, ToolCallingStopReason::MaxIterationsReached)
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
            "[ToolCallingEngine] iteration {} appended {} external message(s) before inference",
            iteration,
            appended.len()
        );
        conversation.append(&mut appended);
    }
}

#[async_trait]
impl AgentExecutor for ToolCallingEngine {
    async fn execute(
        &self,
        context: AgentContext,
        request: ToolCallingRequest,
    ) -> crate::error::Result<ToolCallingResult> {
        if context.is_cancelled() {
            return Err(crate::string_error!("tool-calling execution cancelled before inference"));
        }

        let (messages, stop_reason) = self.run(request.messages);
        Ok(ToolCallingResult { messages, stop_reason })
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[derive(Debug)]
    struct TestLlm {
        called: AtomicBool,
    }

    impl LLMBase for TestLlm {
        fn get_model_name(&self) -> &str {
            "test"
        }

        fn context_length(&self) -> usize {
            1
        }

        fn inference(&self, _param: &InferenceParam) -> LLMMessage {
            self.called.store(true, Ordering::Relaxed);
            LLMMessage::assistant_text("unexpected inference")
        }
    }

    struct TestCancellation;

    impl AgentCancellation for TestCancellation {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    #[test]
    fn cancellation_prevents_inference() {
        let llm = Arc::new(TestLlm { called: AtomicBool::new(false) });
        let mut engine = ToolCallingEngine::new(llm.clone());
        engine.set_cancellation(Arc::new(TestCancellation));

        let (messages, stop_reason) = engine.run(vec![LLMMessage::user("hello")]);

        assert!(messages.is_empty());
        assert!(
            matches!(stop_reason, ToolCallingStopReason::TransportError(message) if message == "tool-calling execution cancelled")
        );
        assert!(!llm.called.load(Ordering::Relaxed));
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
    let lines: Vec<String> =
        items.iter().map(|(name, count)| format!("  - {name}: {count} 次")).collect();
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
