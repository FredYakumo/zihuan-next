use std::collections::HashMap;
use std::sync::Arc;

use log::{info, warn};
use model_inference::message_content_utils::{is_transport_error, sanitize_messages_for_inference};
use serde_json::Value;
use tokio::sync::mpsc;

use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::tooling::ToolCalls;
use zihuan_core::llm::{ContentPart, InferenceParam, MessageContent, MessageRole, OpenAIMessage};

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

/// A tool that [`Brain`] can invoke during an inference loop.
pub trait BrainTool: Send + Sync + 'static {
    /// Returns the LLM-facing function specification (name, description, parameters).
    fn spec(&self) -> Arc<dyn FunctionTool>;
    /// Execute the tool call. `call_content` is the assistant's text for this turn
    /// (used e.g. to send a progress notification before doing the actual work).
    fn execute(&self, call_content: &str, arguments: &Value) -> String;
}

pub trait BrainObserver: Send + Sync + 'static {
    fn on_assistant_tool_request(
        &self,
        _iteration: usize,
        _content: &str,
        _tool_calls: &[ToolCalls],
    ) {
    }

    fn on_tool_start(&self, _name: &str, _call_id: &str, _arguments: &Value) {}

    fn on_tool_finish(&self, _name: &str, _call_id: &str, _result: &str) {}

    fn on_final_assistant(&self, _response: &OpenAIMessage, _stop_reason: &BrainStopReason) {}
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
}

/// Orchestrates a multi-turn LLM ↔ tool call loop.
///
/// Create a `Brain`, register tools with [`Brain::with_tool`] or [`Brain::add_tool`],
/// then call [`Brain::run`] with the initial conversation messages.
pub struct Brain {
    llm: Arc<dyn LLMBase>,
    tools: Vec<Box<dyn BrainTool>>,
    observer: Option<Arc<dyn BrainObserver>>,
}

impl Brain {
    pub fn new(llm: Arc<dyn LLMBase>) -> Self {
        Self {
            llm,
            tools: Vec::new(),
            observer: None,
        }
    }

    /// Register a tool, consuming and returning `self` for builder-style chaining.
    pub fn with_tool(mut self, tool: impl BrainTool) -> Self {
        self.tools.push(Box::new(tool));
        self
    }

    /// Register a tool in-place.
    pub fn add_tool(&mut self, tool: impl BrainTool) {
        self.tools.push(Box::new(tool));
    }

    pub fn with_observer(mut self, observer: Arc<dyn BrainObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn set_observer(&mut self, observer: Arc<dyn BrainObserver>) {
        self.observer = Some(observer);
    }

    /// Run the inference loop and return `(new_messages, stop_reason)`.
    ///
    /// `new_messages` contains all assistant and tool-result messages produced
    /// during this run. The caller's original `messages` are not included.
    pub fn run(&self, messages: Vec<OpenAIMessage>) -> (Vec<OpenAIMessage>, BrainStopReason) {
        let tool_specs: Vec<Arc<dyn FunctionTool>> = self.tools.iter().map(|t| t.spec()).collect();
        let mut conversation = sanitize_messages_for_inference(messages);
        let mut output: Vec<OpenAIMessage> = Vec::new();
        for iteration in 0..MAX_TOOL_ITERATIONS {
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
                observer.on_assistant_tool_request(
                    iteration + 1,
                    &tool_call_content,
                    &response.tool_calls,
                );
            }
            conversation.push(response.clone());
            output.push(response.clone());

            for tc in &response.tool_calls {
                info!(
                    "[Brain] tool call id={} name={} arguments={}",
                    tc.id,
                    tc.function.name,
                    truncate_for_log(&tc.function.arguments.to_string(), LOG_PREVIEW_CHARS)
                );
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_tool_start(&tc.function.name, &tc.id, &tc.function.arguments);
                }
                let result = self
                    .tools
                    .iter()
                    .find(|t| t.spec().name() == tc.function.name)
                    .map(|t| t.execute(&tool_call_content, &tc.function.arguments))
                    .unwrap_or_else(|| {
                        warn!(
                            "[Brain] Tool '{}' not found for call id={} arguments={}",
                            tc.function.name, tc.id, tc.function.arguments
                        );
                        serde_json::json!({"error": format!("Tool '{}' not found", tc.function.name)})
                            .to_string()
                    });

                info!(
                    "[Brain] tool call id={} name={} result: {}",
                    tc.id,
                    tc.function.name,
                    truncate_for_log(&result, LOG_PREVIEW_CHARS)
                );
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_tool_finish(&tc.function.name, &tc.id, &result);
                }
                let mut msg = OpenAIMessage::tool_result(tc.id.clone(), result);
                if let Some(api_style) = self.llm.api_style() {
                    msg.api_style = Some(api_style.to_string());
                }
                conversation.push(msg.clone());
                output.push(msg);
            }
        }

        warn!("[Brain] Tool loop exceeded max iterations ({MAX_TOOL_ITERATIONS})");
        (output, BrainStopReason::MaxIterationsReached)
    }

    pub async fn run_streaming(
        &self,
        messages: Vec<OpenAIMessage>,
        token_tx: mpsc::UnboundedSender<String>,
    ) -> (Vec<OpenAIMessage>, BrainStopReason) {
        let tool_specs: Vec<Arc<dyn FunctionTool>> = self.tools.iter().map(|t| t.spec()).collect();
        let mut conversation = sanitize_messages_for_inference(messages);
        let mut output: Vec<OpenAIMessage> = Vec::new();

        let streaming_llm = self.llm.as_streaming();

        for iteration in 0..MAX_TOOL_ITERATIONS {
            let is_last_iteration = iteration == MAX_TOOL_ITERATIONS - 1;

            if is_last_iteration {
                let counts = count_tool_calls(&conversation);
                append_tool_summary_to_system(&mut conversation, &counts);
            }

            let tools_param: Option<&Vec<Arc<dyn FunctionTool>>> =
                if is_last_iteration || tool_specs.is_empty() {
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
                observer.on_assistant_tool_request(
                    iteration + 1,
                    &tool_call_content,
                    &response.tool_calls,
                );
            }
            conversation.push(response.clone());
            output.push(response.clone());

            for tc in &response.tool_calls {
                info!(
                    "[Brain] tool call id={} name={} arguments={}",
                    tc.id,
                    tc.function.name,
                    truncate_for_log(&tc.function.arguments.to_string(), LOG_PREVIEW_CHARS)
                );
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_tool_start(&tc.function.name, &tc.id, &tc.function.arguments);
                }
                let result = self
                    .tools
                    .iter()
                    .find(|t| t.spec().name() == tc.function.name)
                    .map(|t| t.execute(&tool_call_content, &tc.function.arguments))
                    .unwrap_or_else(|| {
                        warn!(
                            "[Brain] Tool '{}' not found for call id={} arguments={}",
                            tc.function.name, tc.id, tc.function.arguments
                        );
                        serde_json::json!({"error": format!("Tool '{}' not found", tc.function.name)})
                            .to_string()
                    });

                info!(
                    "[Brain] tool call id={} name={} result: {}",
                    tc.id,
                    tc.function.name,
                    truncate_for_log(&result, LOG_PREVIEW_CHARS)
                );
                if let Some(observer) = self.observer.as_ref() {
                    observer.on_tool_finish(&tc.function.name, &tc.id, &result);
                }
                let mut msg = OpenAIMessage::tool_result(tc.id.clone(), result);
                if let Some(api_style) = self.llm.api_style() {
                    msg.api_style = Some(api_style.to_string());
                }
                conversation.push(msg.clone());
                output.push(msg);
            }
        }

        warn!("[Brain] Tool loop exceeded max iterations ({MAX_TOOL_ITERATIONS})");
        (output, BrainStopReason::MaxIterationsReached)
    }
}

/// Count tool calls already present in `messages` by tool name.
fn count_tool_calls(messages: &[OpenAIMessage]) -> HashMap<String, usize> {
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
fn append_tool_summary_to_system(
    messages: &mut Vec<OpenAIMessage>,
    counts: &HashMap<String, usize>,
) {
    if counts.is_empty() {
        return;
    }

    let mut items: Vec<_> = counts.iter().collect();
    items.sort_by(|a, b| a.0.cmp(b.0));
    let lines: Vec<String> = items
        .iter()
        .map(|(name, count)| format!("  - {name}: {count} 次"))
        .collect();
    let summary = format!(
        "工具调用次数已达上限。目前已调用的工具及次数如下：\n{}\n\n请基于已获取的信息直接作答，不再调用任何工具。",
        lines.join("\n")
    );

    for msg in messages.iter_mut() {
        if matches!(msg.role, MessageRole::System) {
            if let Some(ref mut content) = msg.content {
                match content {
                    MessageContent::Text(text) => {
                        text.push('\n');
                        text.push('\n');
                        text.push_str(&summary);
                        return;
                    }
                    MessageContent::Parts(parts) => {
                        parts.push(ContentPart::text(summary));
                        return;
                    }
                }
            }
        }
    }

    messages.push(OpenAIMessage::system(summary));
}

