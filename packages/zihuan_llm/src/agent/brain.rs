use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use log::{info, warn};
use serde_json::Value;

use zihuan_llm_types::llm_base::LLMBase;
use zihuan_llm_types::tooling::FunctionTool;
use zihuan_llm_types::{ContentPart, InferenceParam, MessageContent, MessageRole, OpenAIMessage};

pub const MAX_TOOL_ITERATIONS: usize = 25;

/// Remove dangling / unresolved tool-call sequences from a message history so
/// that the sequence passed to the LLM is always well-formed.
///
/// Mirrors the logic that was duplicated in `BrainNode::sanitize_messages_for_inference`
/// and in `qq_message_agent_node::sanitize_messages`.
pub fn sanitize_messages_for_inference(messages: Vec<OpenAIMessage>) -> Vec<OpenAIMessage> {
    let mut sanitized: Vec<OpenAIMessage> = Vec::with_capacity(messages.len());
    let mut pending: Option<(usize, HashSet<String>)> = None;

    for message in messages {
        if !message.tool_calls.is_empty() {
            if let Some((start, ids)) = pending.take() {
                warn!(
                    "[brain] Dropping incomplete tool-call segment before new assistant tool-call: unresolved_ids={:?}",
                    ids
                );
                sanitized.truncate(start);
            }
            let ids: HashSet<String> = message.tool_calls.iter().map(|tc| tc.id.clone()).collect();
            let start = sanitized.len();
            sanitized.push(message);
            if !ids.is_empty() {
                pending = Some((start, ids));
            }
            continue;
        }

        if matches!(message.role, MessageRole::Tool) {
            let mut keep = false;
            if let Some((_, unresolved)) = pending.as_mut() {
                if let Some(id) = &message.tool_call_id {
                    if unresolved.remove(id) {
                        keep = true;
                    }
                }
            }
            if keep {
                sanitized.push(message);
                if pending.as_ref().is_some_and(|(_, ids)| ids.is_empty()) {
                    pending = None;
                }
            } else {
                warn!("[brain] Dropping orphan tool message");
            }
            continue;
        }

        if let Some((start, ids)) = pending.take() {
            warn!(
                "[brain] Dropping dangling tool-call segment before non-tool message: unresolved_ids={:?}",
                ids
            );
            sanitized.truncate(start);
        }
        sanitized.push(message);
    }

    if let Some((start, ids)) = pending {
        warn!(
            "[brain] Dropping dangling segment at end of history: unresolved_ids={:?}",
            ids
        );
        sanitized.truncate(start);
    }

    sanitized
}

// ─────────────────────────────────────────────────────────────────────────────
// BrainTool trait
// ─────────────────────────────────────────────────────────────────────────────

/// A tool that [`Brain`] can invoke during an inference loop.
pub trait BrainTool: Send + Sync + 'static {
    /// Returns the LLM-facing function specification (name, description, parameters).
    fn spec(&self) -> Arc<dyn FunctionTool>;
    /// Execute the tool call. `call_content` is the assistant's text for this turn
    /// (used e.g. to send a progress notification before doing the actual work).
    fn execute(&self, call_content: &str, arguments: &Value) -> String;
}

// ─────────────────────────────────────────────────────────────────────────────
// BrainStopReason
// ─────────────────────────────────────────────────────────────────────────────

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

// ─────────────────────────────────────────────────────────────────────────────
// Brain
// ─────────────────────────────────────────────────────────────────────────────

/// Orchestrates a multi-turn LLM ↔ tool call loop.
///
/// Create a `Brain`, register tools with [`Brain::with_tool`] or [`Brain::add_tool`],
/// then call [`Brain::run`] with the initial conversation messages.
pub struct Brain {
    llm: Arc<dyn LLMBase>,
    tools: Vec<Box<dyn BrainTool>>,
}

impl Brain {
    pub fn new(llm: Arc<dyn LLMBase>) -> Self {
        Self {
            llm,
            tools: Vec::new(),
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

            // Transport errors → abort immediately.
            if let Some(content) = response.content_text() {
                if is_transport_error(content) {
                    warn!("[Brain] Transport error on iteration {iteration}: {content}");
                    let msg = content.to_string();
                    output.push(response);
                    return (output, BrainStopReason::TransportError(msg));
                }
            }

            if response.tool_calls.is_empty() {
                output.push(response);
                return (output, BrainStopReason::Done);
            }

            // On the last iteration, refuse to execute further tool calls.
            if is_last_iteration {
                output.push(response);
                return (output, BrainStopReason::MaxIterationsReached);
            }

            let tool_call_content = response.content_text_owned().unwrap_or_default();
            if !tool_call_content.is_empty() {
                info!(
                    "[Brain] iteration {} assistant content: {tool_call_content}",
                    iteration + 1
                );
            }
            info!(
                "[Brain] iteration {} processing {} tool call(s)",
                iteration + 1,
                response.tool_calls.len()
            );
            conversation.push(response.clone());
            output.push(response.clone());

            for tc in &response.tool_calls {
                info!(
                    "[Brain] tool call id={} name={} arguments={}",
                    tc.id, tc.function.name, tc.function.arguments
                );
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
                    "[Brain] tool call id={} name={} result: {result}",
                    tc.id, tc.function.name
                );
                let msg = OpenAIMessage::tool_result(tc.id.clone(), result);
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

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` if `content` looks like a transport-level LLM error string.
pub fn is_transport_error(content: &str) -> bool {
    content.starts_with("Error: API request failed")
        || content.starts_with("Error: Failed to send request")
        || content.starts_with("Error: Failed to parse response")
        || content.starts_with("Error: Invalid response structure")
}
