use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use salvo::http::body::BodySender;
use salvo::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use salvo::http::HeaderValue;
use salvo::http::ResBody;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use uuid::Uuid;
use zihuan_core::agent::resource_resolver::{build_llm_model, resolve_llm_service_config};
use zihuan_core::agent::tools::{ToolCallingObserver, ToolCallingStopReason};
use zihuan_core::agent::{
    Agent, AgentCancellation, ContextCompactionEvent, ContextCompactionObserver,
};
use zihuan_core::chat_history::{
    chat_history_dir, delete_session_title, load_session_title, write_session_title,
};
use zihuan_core::command::{CommandChannel, CommandContext};
use zihuan_core::config::llm_refs::load_llm_refs;
use zihuan_core::error::{Error, Result};
use zihuan_core::ims_bot_adapter::resolve_fallback_bot_profile;
use zihuan_core::message_part::MessagePart;
use zihuan_core::model_inference::llm::tooling::ToolCalls;
use zihuan_core::model_inference::llm::{LLMMessage, MessageRole, StreamToken, TokenUsage};
use zihuan_core::role::procedure::ProcedureContext;
use zihuan_core::role::service_config::RoleServiceConfig;
use zihuan_core::role::TransportSink;
use zihuan_core::storage::ConnectionConfig;
use zihuan_core::workspace::{normalized_workspace_path, AskUserRequest};

use zihuan_workspace_service::api::workspace_changes;
use zihuan_workspace_service::procedure::WorkspaceBrain;
use zihuan_workspace_service::task_tracking::{
    delete_workspace_tasks, interrupt_workspace_tasks, load_workspace_tasks,
};

use crate::api::state::{RunningChatMessage, RunningChatToolCall, TaskStatus};
use crate::api::ws::{ServerMessage, WsBroadcast};
use zihuan_service::role::RoleAgent;

const CHAT_STREAM_MAX_BODY_BYTES: usize = 64 * 1024 * 1024;
const CHAT_FORK_METADATA_SUFFIX: &str = ".fork.json";

struct WorkspaceChatCancellation(Arc<AtomicBool>);

impl AgentCancellation for WorkspaceChatCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// SSE adapter's transport out boundary (documents/transport.md).
///
/// **Design:** Holds the sending halves of the token and brain-event channels; the relay loops
/// in `execute_chat_streaming` own the receiving halves. Procedures emit through the sink via
/// the `TransportSink` impl, and all sender clones dropped when the turn task ends let the
/// relay drain and exit.
struct SseTransportSink {
    token_tx: mpsc::UnboundedSender<StreamToken>,
    event_tx: mpsc::UnboundedSender<Value>,
}

impl TransportSink for SseTransportSink {
    fn token_sender(&self) -> mpsc::UnboundedSender<StreamToken> {
        self.token_tx.clone()
    }

    fn send_event(&self, event: Value) {
        let _ = self.event_tx.send(event);
    }
}

/// Bridges ToolCallingObserver callbacks into the SSE event stream.
///
/// **Purpose:** The ToolCallingEngine tool-call loop emits structured events (tool start/finish)
/// that the dashboard needs to display in real time. This observer translates those callbacks
/// into JSON payloads and pushes them onto the same unbounded channel that the token stream
/// uses, so the relay loop can multiplex both onto a single SSE connection.
///
/// **Design:** Uses an unbounded sender intentionally — the relay loop drains both the token and
/// event channels via `tokio::select!`, so backpressure is managed by the SSE sender, not the
/// observer. Errors from `send` are silently ignored because a closed channel means the client
/// has disconnected and the entire streaming task will tear down.
///
/// **Architecture:** Created per-request inside the spawned turn task of
/// `execute_chat_streaming`, passed as the turn's `ToolCallingObserver` into the brain
/// procedure.
struct SseToolCallingObserver {
    event_tx: mpsc::UnboundedSender<Value>,
    session_id: String,
    message_id: String,
    change_recorder: Arc<workspace_changes::WorkspaceChangeRecorder>,
    running_chat_message: Option<Arc<Mutex<RunningChatMessage>>>,
}

impl ToolCallingObserver for SseToolCallingObserver {
    fn on_tool_start(&self, name: &str, call_id: &str, arguments: &Value) {
        if let Some(snapshot) = &self.running_chat_message {
            snapshot.lock().unwrap().live_tool_calls.push(RunningChatToolCall {
                call_id: call_id.to_string(),
                name: name.to_string(),
                arguments: arguments.clone(),
                result: serde_json::to_string(&json!({ "stdout": "", "stderr": "" })).unwrap(),
                done: false,
            });
        }
        let event = json!({
            "type": "tool_call_start",
            "message_id": self.message_id,
            "call_id": call_id,
            "name": name,
            "arguments": arguments,
        });
        let _ = self.event_tx.send(event);
        if let Some(operation) = workspace_changes::operation_for_tool(name) {
            self.change_recorder.start(call_id, operation, arguments);
        }
    }

    fn on_tool_output(&self, name: &str, call_id: &str, stream: &str, chunk: &str) {
        if stream == "command_confirmation" {
            let payload = serde_json::from_str::<Value>(chunk).unwrap_or_else(|_| json!({}));
            let event = json!({
                "type": "command_confirmation",
                "session_id": self.session_id,
                "message_id": self.message_id,
                "call_id": call_id,
                "name": name,
                "command": payload.get("command"),
                "shell": payload.get("shell"),
            });
            let _ = self.event_tx.send(event);
            return;
        }
        if let Some(snapshot) = &self.running_chat_message {
            let mut snapshot = snapshot.lock().unwrap();
            if let Some(tool_call) =
                snapshot.live_tool_calls.iter_mut().find(|item| item.call_id == call_id)
            {
                let mut output =
                    serde_json::from_str::<Value>(&tool_call.result).unwrap_or_else(|_| json!({}));
                let key = if stream == "stderr" {
                    "stderr"
                } else {
                    "stdout"
                };
                let content = output[key].as_str().unwrap_or_default();
                output[key] = Value::String(format!("{content}{chunk}"));
                tool_call.result = serde_json::to_string(&output).unwrap();
            }
        }
        let event = json!({
            "type": "tool_call_output",
            "message_id": self.message_id,
            "call_id": call_id,
            "name": name,
            "stream": stream,
            "chunk": chunk,
        });
        let _ = self.event_tx.send(event);
    }

    fn on_tool_finish(&self, name: &str, call_id: &str, result: &str) {
        if let Some(snapshot) = &self.running_chat_message {
            let mut snapshot = snapshot.lock().unwrap();
            if let Some(tool_call) =
                snapshot.live_tool_calls.iter_mut().find(|item| item.call_id == call_id)
            {
                tool_call.result = result.to_string();
                tool_call.done = true;
            }
        }
        let event = json!({
            "type": "tool_call_result",
            "message_id": self.message_id,
            "call_id": call_id,
            "name": name,
            "result": result,
        });
        let _ = self.event_tx.send(event);
        if matches!(name, "TaskCreate" | "TaskUpdate" | "TaskGet" | "TaskList") {
            if let Ok(payload) = serde_json::from_str::<Value>(result) {
                if let Some(tasks) = payload.get("tasks") {
                    let _ = self.event_tx.send(json!({
                        "type": "workspace_tasks",
                        "message_id": self.message_id,
                        "tasks": tasks,
                    }));
                }
            }
        }
        if let Some(change) = self.change_recorder.finish(call_id, result) {
            let _ = self.event_tx.send(json!({
                "type": "workspace_change",
                "message_id": self.message_id,
                "change": change,
            }));
        }
    }
}

/// Incoming request body for the `/chat/stream` endpoint.
///
/// **Purpose:** Carries the agent to talk to, an optional session ID for continuing an existing
/// Incoming message shape sent by the dashboard frontend.
///
/// The frontend uses the flat OpenAI-style `content: String` field.
/// This type accepts that wire format and converts to the internal
/// `LLMMessage` (which uses `parts`) at the API boundary.
#[derive(Debug, Deserialize)]
struct DashboardChatMessage {
    pub role: MessageRole,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub parts: Vec<MessagePart>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCalls>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

impl From<DashboardChatMessage> for LLMMessage {
    fn from(msg: DashboardChatMessage) -> Self {
        let parts = if !msg.parts.is_empty() {
            msg.parts
        } else if !msg.content.is_empty() {
            vec![MessagePart::text(msg.content)]
        } else {
            Vec::new()
        };
        LLMMessage {
            role: msg.role,
            parts,
            reasoning_content: None,
            tool_calls: msg.tool_calls,
            tool_call_id: msg.tool_call_id,
            usage: None,
        }
    }
}

fn history_record_to_message(record: ChatHistoryRecord) -> LLMMessage {
    LLMMessage {
        role: match record.role.as_str() {
            "user" => MessageRole::User,
            "assistant" => MessageRole::Assistant,
            "tool" => MessageRole::Tool,
            _ => MessageRole::System,
        },
        parts: if record.parts.is_empty() && !record.content.is_empty() {
            vec![MessagePart::text(record.content)]
        } else {
            record.parts
        },
        reasoning_content: record.reasoning_content,
        tool_calls: record.tool_calls,
        tool_call_id: record.tool_call_id,
        usage: None,
    }
}

/// conversation, the full message history, and a stream toggle.
///
/// **Design:** Mirrors the OpenAI chat-completion request shape but adds `agent_id` and
/// `session_id` fields specific to zihuan's multi-agent routing.
#[derive(Debug, Deserialize)]
pub struct ChatStreamRequest {
    pub agent_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    messages: Vec<DashboardChatMessage>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub model_config_id: Option<String>,
    #[serde(default)]
    pub image_understand_model_config_id: Option<String>,
    #[serde(default)]
    pub thinking_type: Option<zihuan_core::model_inference::model_config::ThinkingType>,
    #[serde(default)]
    pub reasoning_effort: Option<zihuan_core::model_inference::model_config::ReasoningEffort>,
    #[serde(default)]
    pub workspace_path: Option<String>,
    #[serde(default)]
    pub continuation: Option<ChatContinuationDecision>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatContinuationDecision {
    Continue,
    Stop,
}

#[derive(Debug, Deserialize)]
struct StopChatRequest {
    #[serde(default)]
    task_id: Option<String>,
}

/// Summary row returned by the session-list endpoint.
///
/// **Purpose:** Gives the frontend enough information to render the sidebar session list —
/// display name, timestamps, agent metadata — without loading full message history.
///
/// **Design:** `agent_id/name/type/avatar_url` are all optional because legacy session files may
/// lack these fields; the frontend degrades gracefully when they are absent.
#[derive(Debug, Serialize)]
pub struct ChatSessionSummary {
    pub session_id: String,
    pub root_session_id: String,
    pub updated_at: String,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub role_service_type: Option<String>,
    pub agent_avatar_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_ask_user: Option<AskUserRequest>,
    pub title: String,
}

/// Single line in a `.jsonl` chat-history file.
///
/// **Purpose:** The canonical on-disk representation of every message in a session — user,
/// assistant, tool-call, tool-result. One JSON object per line, appended sequentially.
///
/// **Design:** Uses newline-delimited JSON (JSONL) rather than a single JSON array so that
/// appending a new record is O(1) — just seek to end and write. The trade-off is that reading
/// requires line-by-line parsing, but sessions are typically small enough that this dominates
/// nothing. `stream_index` is reserved for future token-level replay; currently always `None`.
///
/// **Architecture:** Written by `append_history_record`, read by `load_chat_session_messages`.
/// The schema must remain backward-compatible because old files are never migrated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHistoryRecord {
    pub session_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub role_service_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_avatar_url: Option<String>,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<MessagePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    pub timestamp: String,
    #[serde(default)]
    pub stream_index: Option<usize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub streaming: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub live_tool_calls: Vec<RunningChatToolCall>,
    pub trace_id: String,
    pub message_id: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCalls>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    /// Model config used for this message's request, so the frontend can restore the
    /// previously used model when editing or re-sending a conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_config_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_understand_model_config_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_ask_user: Option<AskUserRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ChatResponseMetrics>,
}

/// Aggregated inference telemetry for one streamed dashboard chat reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponseMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_to_first_token_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_per_second: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_prompt_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache_miss_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hit_rate: Option<f64>,
}

#[derive(Debug, Default)]
struct RelayTiming {
    first_token_after_start: Option<Duration>,
}

#[derive(Debug, Deserialize)]
pub struct ChatForkRequest {
    pub message_id: String,
}

#[derive(Debug, Serialize)]
pub struct ChatForkResponse {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatForkMetadata {
    source_session_id: String,
    source_message_id: String,
    fork_group_id: String,
    prefix_record_count: usize,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct ChatBranchVersion {
    session_id: String,
    message_id: String,
}

#[derive(Debug, Serialize)]
struct ChatMessageBranch {
    message_id: String,
    current_index: usize,
    total: usize,
    versions: Vec<ChatBranchVersion>,
}

/// Lightweight display metadata extracted from an `RoleServiceConfig`.
///
/// **Purpose:** Avoids passing the full `RoleServiceConfig` (which contains LLM credentials and
/// connections) into the persistence and SSE layers that only need name/type/avatar.
#[derive(Debug, Clone)]
struct AgentSnapshot {
    name: String,
    role_service_type: String,
    avatar_url: Option<String>,
}

/// Shared mutable state for command effects that run on the dashboard channel.
///
/// **Purpose:** Commands like "new conversation" need to issue a fresh session ID that the
/// streaming task must pick up. The dashboard command runtime records the new-session signal
/// here and the caller reads it after the engine run completes.
///
/// **Design:** Uses `Arc<Mutex<Option<String>>>` — minimal overhead for a rarely-contended
/// single-write-then-read pattern. The mutex guard is held only briefly during `issue_new_session_id`
/// and `current_new_session_id`.
#[derive(Clone, Default)]
struct DashboardCommandSideEffectState {
    next_session_id: Arc<Mutex<Option<String>>>,
}

impl DashboardCommandSideEffectState {
    fn issue_new_session_id(&self) -> String {
        let mut guard = self.next_session_id.lock().unwrap();
        guard.get_or_insert_with(|| Uuid::new_v4().to_string()).clone()
    }

    fn current_new_session_id(&self) -> Option<String> {
        self.next_session_id.lock().unwrap().clone()
    }
}

/// Dashboard (SSE) channel command runtime.
///
/// Executes `builtin://*` steps shared with the QQ channel and renders effects
/// into dashboard outputs: assistant texts plus an optional new-session signal.
/// QQ-only privileged specs (`ims://*`) fall back to a channel-unsupported
/// message, preserving the pre-engine stub behavior for QQ-kind agents served
/// over the dashboard transport.
struct DashboardCommandRuntime {
    state: DashboardCommandSideEffectState,
    output_texts: Vec<String>,
}

impl DashboardCommandRuntime {
    fn new(state: DashboardCommandSideEffectState) -> Self {
        Self { state, output_texts: Vec::new() }
    }
}

impl zihuan_core::command::CommandRuntime for DashboardCommandRuntime {
    fn run_step(
        &mut self,
        step: &zihuan_core::command::Step,
        inv: &zihuan_core::command::Invocation,
        _state: &mut zihuan_core::command::CmdState,
    ) -> Result<zihuan_core::command::StepRun> {
        if let Some(op) = step.op.strip_prefix("builtin://") {
            return match zihuan_core::command::execute_builtin(op, &inv.args, &inv.ctx.caller_id) {
                Some(Ok(effects)) => {
                    Ok(zihuan_core::command::StepRun::stop_with(effects))
                }
                Some(Err(err)) => Err(err),
                None => Err(zihuan_core::string_error!(
                    "builtin 操作不存在: {}",
                    step.op
                )),
            };
        }
        // QQ-only privileged command reached a non-QQ channel.
        Ok(zihuan_core::command::StepRun::stop_with(vec![
            zihuan_core::command::Effect::Text(
                "该命令仅能在 QQ Chat Agent 运行时中使用。".to_string(),
            ),
        ]))
    }

    fn apply_effect(&mut self, effect: &zihuan_core::command::Effect) -> Result<()> {
        match effect {
            zihuan_core::command::Effect::Text(text)
            | zihuan_core::command::Effect::Notice(text)
            | zihuan_core::command::Effect::Forward(text) => self.output_texts.push(text.clone()),
            zihuan_core::command::Effect::StartNewConversation => {
                self.state.issue_new_session_id();
            }
            // The engine folds SetContext into the shared CmdState before
            // delivery; dashboard never receives one.
            zihuan_core::command::Effect::SetContext { .. } => {}
        }
        Ok(())
    }
}

fn extract_agent_snapshot(
    agent: &RoleServiceConfig,
    connections: &[ConnectionConfig],
) -> AgentSnapshot {
    let avatar_url =
        if let Some(config) = zihuan_service::role::optional_qq_chat(&agent.role_service_type) {
            resolve_fallback_bot_profile(connections, &config.ims_bot_adapter_connection_id)
                .ok()
                .flatten()
                .and_then(|profile| profile.avatar_url)
        } else {
            agent.avatar_url.clone()
        };

    AgentSnapshot {
        name: agent.name.clone(),
        role_service_type: agent.role_service_type.kind_tag().into(),
        avatar_url,
    }
}

///
/// **Purpose:** Bundles the fully resolved `RoleServiceConfig` and its display snapshot (name, type,
/// avatar) so that downstream stages don't need to look up connections again.
///
/// **Design:** Produced by `resolve_chat_agent` and consumed exclusively by the orchestrator
/// `execute_chat_streaming`. Keeping these together avoids redundant connection lookups in
/// both the command-dispatch and persistence stages.
///
/// **Architecture:** Sits between the agent-manager lookup (infrastructure) and the domain
/// logic (command dispatch, inference, persistence). Not persisted — only lives for the
/// duration of a single streaming request.
struct ChatAgentInfo {
    agent: RoleServiceConfig,
    agent_snapshot: AgentSnapshot,
}

/// Outcome of dashboard command dispatch — determines how the pipeline proceeds
/// after a slash-command is recognized (or not).
///
/// **Purpose:** When a slash-command matches, the pipeline may short-circuit (skip LLM
/// inference), mutate the message list, or switch the session.  This struct makes every such
/// decision explicit so the orchestrator can remain branch-free and declarative.
struct CommandDispatchOutcome {
    session_id: String,
    messages: Vec<LLMMessage>,
    latest_user_message: Option<LLMMessage>,
    should_run_inference: bool,
    should_persist: bool,
    requires_assistant_message: bool,
    immediate_output_messages: Option<Vec<LLMMessage>>,
}

/// Look up a running agent by ID and build its display snapshot.
///
/// **Purpose:** Validates that the requested agent is currently active and collects the
/// connection data needed to render its avatar — the first gate in the streaming pipeline.
///
/// **Design:** Returns `Err(SSE-error-JSON)` so the caller can forward it directly to the
/// client when the agent is missing or connections fail to load. This avoids scattering
/// error-serialization logic throughout the orchestrator.
///
/// **Architecture:** Called at the top of `execute_chat_streaming`. The returned
/// `ChatAgentInfo` is then threaded into command dispatch, persistence, and SSE event
/// construction.
fn resolve_chat_agent(
    role_service_manager: &zihuan_service::RoleServiceManager,
    agent_id: &str,
) -> std::result::Result<ChatAgentInfo, Value> {
    let running_role_service = role_service_manager.running_role_service(agent_id).ok_or_else(
        || json!({ "type": "error", "error": format!("agent '{}' is not running", agent_id) }),
    )?;
    let role_service = running_role_service.agent().clone();

    let connections = crate::system_config::load_connections()
        .map_err(|err| json!({ "type": "error", "error": err.to_string() }))?;
    let agent_snapshot = extract_agent_snapshot(&role_service, &connections);

    Ok(ChatAgentInfo { agent: role_service, agent_snapshot })
}

/// Attempt to match and execute a dashboard slash-command against the user's latest message.
///
/// **Purpose:** Slash-commands (e.g. `/new`, `/reset`) are dispatched before LLM inference
/// begins. Depending on the command, the pipeline may skip inference entirely, rewrite the
/// message list, or switch to a new session. This function centralises all that branching
/// logic and returns a single `CommandDispatchOutcome` that tells the orchestrator exactly
/// what to do next.
///
/// **Design:** The function follows an early-return pattern for the "no command" case (returns
/// the default outcome). When a command matches, it runs the command state machine through the
/// `DashboardCommandRuntime`, then constructs the appropriate outcome based on whether the
/// command produced a passthrough text, an immediate reply, or triggered a new-conversation
/// effect. The three exit paths are documented on `CommandDispatchOutcome`.
///
/// **Architecture:** Called after `resolve_chat_agent` in `execute_chat_streaming`. Depends on
/// the global `CommandRegistry` from `zihuan_service`. Does **not** touch the SSE sender —
/// errors are returned as `Err(Value)` for the orchestrator to forward.
fn try_dispatch_dashboard_command(
    agent: &RoleServiceConfig,
    agent_snapshot: &AgentSnapshot,
    requested_session_id: &Option<String>,
    messages: Vec<LLMMessage>,
    latest_user_message: &Option<LLMMessage>,
) -> std::result::Result<CommandDispatchOutcome, Value> {
    let requested_session_id =
        requested_session_id.as_deref().filter(|value| !value.trim().is_empty());
    let mut session_id = requested_session_id
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let mut messages = messages;
    let mut latest_user_message = latest_user_message.clone();
    let mut should_run_inference = true;
    let mut should_persist = true;
    let mut requires_assistant_message = true;
    let mut immediate_output_messages: Option<Vec<LLMMessage>> = None;

    let Some(command_registry) = zihuan_service::command::global_command_registry() else {
        return Ok(CommandDispatchOutcome {
            session_id,
            messages,
            latest_user_message,
            should_run_inference,
            should_persist,
            requires_assistant_message,
            immediate_output_messages,
        });
    };

    let raw_user_text = latest_user_message.as_ref().and_then(LLMMessage::content_text_owned);

    let Some(raw_user_text) = raw_user_text else {
        return Ok(CommandDispatchOutcome {
            session_id,
            messages,
            latest_user_message,
            should_run_inference,
            should_persist,
            requires_assistant_message,
            immediate_output_messages,
        });
    };

    let command_context = CommandContext {
        agent_type: agent_snapshot.role_service_type.clone(),
        agent_id: agent.id.clone(),
        caller_id: requested_session_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| "dashboard".to_string()),
        channel: CommandChannel::DashboardChat {
            session_id: requested_session_id.map(|s| s.to_string()),
        },
    };

    let Some((spec, parsed)) = command_registry.spec_for(&command_context, &raw_user_text) else {
        return Ok(CommandDispatchOutcome {
            session_id,
            messages,
            latest_user_message,
            should_run_inference,
            should_persist,
            requires_assistant_message,
            immediate_output_messages,
        });
    };

    // Permission gate.
    let permission = command_registry.check_permission(&command_context, &raw_user_text);
    if permission.matched && !permission.allowed {
        immediate_output_messages = Some(vec![LLMMessage::assistant_text(
            "你没有权限使用此命令。".to_string(),
        )]);
        should_run_inference = false;
        return Ok(CommandDispatchOutcome {
            session_id,
            messages,
            latest_user_message,
            should_run_inference,
            should_persist,
            requires_assistant_message,
            immediate_output_messages,
        });
    }

    let side_effect_state = DashboardCommandSideEffectState::default();
    let mut runtime = DashboardCommandRuntime::new(side_effect_state.clone());
    let invocation = zihuan_core::command::Invocation {
        ctx: command_context,
        args: parsed.args,
        passthrough: parsed.passthrough_text,
        resumed: false,
    };
    let execution = match zihuan_core::command::execute_command(&mut runtime, spec, invocation) {
        Ok(execution) => execution,
        Err(err) => return Err(json!({ "type": "error", "error": err.to_string() })),
    };

    let issued_new_session_id = side_effect_state.current_new_session_id();
    if let Some(next_session_id) = issued_new_session_id.clone() {
        session_id = next_session_id;
    }

    let passthrough_text = execution.passthrough;
    let output_text = runtime.output_texts.join("\n");

    if let Some(passthrough_text) = passthrough_text {
        let passthrough_message = LLMMessage::user(passthrough_text.clone());
        latest_user_message = Some(passthrough_message.clone());

        if issued_new_session_id.is_some() {
            messages = vec![passthrough_message];
        } else {
            replace_last_user_message(&mut messages, passthrough_message);
        }
    } else if issued_new_session_id.is_some() {
        // Starting a new conversation consumes the turn without emitting an
        // assistant message (mirrors the pre-engine `/new` behavior).
        should_run_inference = false;
        should_persist = false;
        requires_assistant_message = false;
        latest_user_message = None;
    } else {
        should_run_inference = false;
        if !output_text.is_empty() {
            immediate_output_messages = Some(vec![LLMMessage::assistant_text(output_text)]);
        } else {
            // A matched command that produced no visible output still consumes
            // the turn rather than falling through to inference.
            immediate_output_messages = Some(vec![LLMMessage::assistant_text("命令已执行。".to_string())]);
        }
    }

    Ok(CommandDispatchOutcome {
        session_id,
        messages,
        latest_user_message,
        should_run_inference,
        should_persist,
        requires_assistant_message,
        immediate_output_messages,
    })
}

/// Emit a command's immediate reply as a single SSE delta event and optionally persist it.
///
/// **Purpose:** When a slash-command short-circuits the pipeline (no LLM inference needed),
/// the reply must still be sent to the client and optionally recorded to the session file.
/// This function handles both concerns in one place.
///
/// **Design:** Returns `false` if the SSE write or persistence fails, allowing the caller to
/// abort the stream cleanly. The function is small enough that error propagation would add
/// more noise than the boolean return.
///
/// **Architecture:** Called from the `!should_run_inference` branch of
/// `execute_chat_streaming`, after the `start` event has been sent.
async fn emit_immediate_output(
    sender: &mut BodySender,
    session_id: &str,
    assistant_message_id: &str,
    output_messages: &[LLMMessage],
    should_persist: bool,
    agent: &RoleServiceConfig,
    agent_snapshot: &AgentSnapshot,
    trace_id: &str,
    latest_user_message: Option<&LLMMessage>,
    workspace_path: Option<String>,
    model_config_id: Option<&str>,
) -> bool {
    if let Some(content) = output_messages
        .iter()
        .find(|message| matches!(message.role, MessageRole::Assistant))
        .and_then(LLMMessage::content_text_owned)
    {
        let delta_event = json!({
            "type": "delta",
            "message_id": assistant_message_id,
            "index": 0,
            "token": content,
        });
        if sender.send_data(format!("data: {delta_event}\n\n")).await.is_err() {
            return false;
        }
    }

    if should_persist {
        if let Err(err) = persist_chat_records(
            session_id,
            agent,
            agent_snapshot,
            trace_id,
            assistant_message_id,
            latest_user_message,
            output_messages,
            workspace_path,
            None,
            None,
            true,
            model_config_id,
            None,
        ) {
            let event = json!({ "type": "error", "error": err.to_string() });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return false;
        }
    }

    true
}

/// Relay inference tokens and brain events to the SSE client in real time.
///
/// **Purpose:** In streaming mode, each token and tool-call event must arrive at the browser
/// as soon as it is produced. This function runs a `tokio::select!` loop that multiplexes
/// both channels onto the SSE connection.
///
/// **Design:** Uses `biased` select to prioritise brain events over token deltas — tool-call
/// UI state should update before the next token chunk arrives. When the token channel closes
/// (inference finished), remaining brain events are drained with `try_recv` before returning.
/// A failed `send_data` (client disconnected) breaks the loop immediately.
///
/// **Architecture:** Spawned inline by `execute_chat_streaming` when `stream` is true.
/// The inference task and this relay run concurrently; the orchestrator joins the inference
/// handle after this function returns.
async fn relay_inference_stream(
    sender: &mut BodySender,
    assistant_message_id: &str,
    token_rx: &mut mpsc::UnboundedReceiver<StreamToken>,
    event_rx: &mut mpsc::UnboundedReceiver<Value>,
    inference_started_at: Instant,
    running_chat_message: Option<&Arc<Mutex<RunningChatMessage>>>,
) -> (bool, RelayTiming) {
    let mut client_connected = true;
    let mut timing = RelayTiming::default();
    loop {
        tokio::select! {
            biased;
            Some(brain_event) = event_rx.recv() => {
                if sender.send_data(format!("data: {brain_event}\n\n")).await.is_err() {
                    client_connected = false;
                    break;
                }
            }
            token_opt = token_rx.recv() => {
                match token_opt {
                    Some(token) => {
                        if timing.first_token_after_start.is_none() {
                            timing.first_token_after_start = Some(inference_started_at.elapsed());
                        }
                        let event_type = match &token {
                            StreamToken::Thinking(_) => "thinking_delta",
                            StreamToken::Content(_) => "delta",
                        };
                        if let Some(snapshot) = running_chat_message {
                            let mut snapshot = snapshot.lock().unwrap();
                            match &token {
                                StreamToken::Thinking(value) => snapshot.reasoning_content.push_str(value),
                                StreamToken::Content(value) => snapshot.content.push_str(value),
                            }
                        }
                        let delta_event = json!({
                            "type": event_type,
                            "message_id": assistant_message_id,
                            "token": token.as_str(),
                        });
                        if sender.send_data(format!("data: {delta_event}\n\n")).await.is_err() {
                            client_connected = false;
                            break;
                        }
                    }
                    None => {
                        while let Ok(brain_event) = event_rx.try_recv() {
                            if sender.send_data(format!("data: {brain_event}\n\n")).await.is_err() {
                                client_connected = false;
                                break;
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
    (client_connected, timing)
}

/// Collect all inference tokens into a single payload, then emit one delta event.
///
/// **Purpose:** In non-streaming mode, the client expects the full response in one shot rather
/// than a token-by-token stream. This function still relays brain events (tool-call progress)
/// in real time, but batches all text tokens and sends them as a single delta after inference
/// completes.
///
async fn relay_collected_text(
    sender: &mut BodySender,
    assistant_message_id: &str,
    token_rx: &mut mpsc::UnboundedReceiver<StreamToken>,
    event_rx: &mut mpsc::UnboundedReceiver<Value>,
) {
    let mut full_content = String::new();
    loop {
        tokio::select! {
            biased;
            Some(brain_event) = event_rx.recv() => {
                let _ = sender.send_data(format!("data: {brain_event}\n\n")).await;
            }
            token_opt = token_rx.recv() => {
                match token_opt {
                    Some(token) => full_content.push_str(token.as_str()),
                    None => {
                        while let Ok(brain_event) = event_rx.try_recv() {
                            let _ = sender.send_data(format!("data: {brain_event}\n\n")).await;
                        }
                        break;
                    }
                }
            }
        }
    }
    if !full_content.is_empty() {
        let delta_event = json!({
            "type": "delta",
            "message_id": assistant_message_id,
            "index": 0,
            "token": full_content,
        });
        let _ = sender.send_data(format!("data: {delta_event}\n\n")).await;
    }
}

async fn send_sse(sender: &mut BodySender, event: &Value) -> bool {
    sender.send_data(format!("data: {event}\n\n")).await.is_ok()
}

fn is_false(value: &bool) -> bool {
    !value
}

/// HTTP handler for `POST /chat/stream`.
///
/// **Purpose:** Validates the request body, sets up the SSE response channel, and spawns the
/// streaming task. Returns immediately so the Salvo handler does not block.
///
/// **Design:** Request validation happens synchronously before spawning; errors are rendered
/// as HTTP 400 responses. The SSE body channel is created with `ResBody::channel()` and the
/// receiver is attached to the response, while the sender goes into the spawned task.
#[handler]
pub async fn stream_chat(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let body: ChatStreamRequest =
        match req.parse_json_with_max_size(CHAT_STREAM_MAX_BODY_BYTES).await {
            Ok(body) => body,
            Err(err) => {
                render_bad_request(res, format!("invalid request body: {err}"));
                return;
            }
        };

    if body.agent_id.trim().is_empty() {
        render_bad_request(res, "agent_id must not be empty".to_string());
        return;
    }
    if body.messages.is_empty() && body.continuation.is_none() {
        render_bad_request(res, "messages must not be empty".to_string());
        return;
    }

    let state = depot.obtain::<std::sync::Arc<crate::api::state::AppState>>().unwrap().clone();
    let broadcast_tx = depot.obtain::<WsBroadcast>().unwrap().clone();

    let (sender, receiver) = ResBody::channel();
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream; charset=utf-8"));
    res.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    res.body = receiver;

    tokio::spawn(execute_chat_streaming(state, broadcast_tx, body, sender));
}

#[handler]
pub async fn stop_chat(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    if session_id.trim().is_empty() {
        render_bad_request(res, "session_id must not be empty".to_string());
        return;
    }
    let body = req
        .parse_json::<StopChatRequest>()
        .await
        .unwrap_or(StopChatRequest { task_id: None });
    let stopped = depot
        .obtain::<Arc<crate::api::state::AppState>>()
        .unwrap()
        .tasks
        .lock()
        .unwrap()
        .stop_workspace_chat_task(&session_id, body.task_id.as_deref());
    // Stopping is intentionally idempotent. A second click may arrive after the turn has
    // already observed the cancellation flag and removed itself from the running-task list.
    res.render(Json(json!({ "ok": true, "stopped": stopped })));
}

#[handler]
pub async fn list_chat_sessions(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let filter_agent_id = req.query::<String>("agent_id");
    match load_chat_sessions(filter_agent_id.as_deref()) {
        Ok(sessions) => res.render(Json(json!({ "sessions": sessions }))),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn get_chat_session_messages(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    if session_id.trim().is_empty() {
        render_bad_request(res, "session_id must not be empty".to_string());
        return;
    }

    let state = depot.obtain::<Arc<crate::api::state::AppState>>().unwrap();
    match load_chat_session_messages(&session_id) {
        Ok(mut messages) => {
            append_running_chat_message(&mut messages, &session_id, state);
            match load_message_branches(&session_id, &messages) {
            Ok(branches) => match load_workspace_tasks(&session_id) {
                Ok(snapshot) => res.render(Json(json!({ "messages": messages, "branches": branches, "tasks": snapshot.tasks }))),
                Err(err) => render_internal_error(res, zihuan_core::string_error!("failed to load workspace tasks: {err}")),
            },
            Err(err) => render_internal_error(res, err),
            }
        }
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn fork_chat_session(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    let body: ChatForkRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => {
            render_bad_request(res, format!("invalid fork request: {err}"));
            return;
        }
    };
    if session_id.trim().is_empty() || body.message_id.trim().is_empty() {
        render_bad_request(res, "session_id and message_id must not be empty".to_string());
        return;
    }

    match fork_chat_session_history(&session_id, &body.message_id) {
        Ok(forked_session_id) => {
            res.render(Json(ChatForkResponse { session_id: forked_session_id }))
        }
        Err(Error::ValidationError(message)) => render_bad_request(res, message),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn delete_chat_session(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    if session_id.trim().is_empty() {
        render_bad_request(res, "session_id must not be empty".to_string());
        return;
    }

    match delete_chat_session_file(&session_id)
        .and_then(|_| delete_workspace_tasks(&session_id).map_err(Error::ValidationError))
    {
        Ok(()) => res.render(Json(json!({ "ok": true }))),
        Err(err) => render_internal_error(res, err),
    }
}

/// Orchestrates a single chat-streaming request from end to end.
///
/// **Purpose:** This is the main entry point for the `/chat/stream` SSE pipeline. It
/// coordinates agent resolution, command dispatch, LLM inference (streaming or collected),
/// SSE event emission, and chat-history persistence — in that order.
///
/// **Design:** The pipeline proceeds through four stages, each delegated to a dedicated
/// helper:
///
/// 1. **Agent resolution** (`resolve_chat_agent`) — validates the agent is running and builds
///    a display snapshot.
/// 2. **Command dispatch** (`try_dispatch_dashboard_command`) — intercepts slash-commands
///    before inference; may short-circuit the pipeline with an immediate reply or a session
///    switch.
/// 3. **Procedure chain + relay** — if inference is required, spawns the RoleService procedure
///    chain (`run_procedure_chain`, whose Brain procedure drives the inference) in a background
///    task and either `relay_inference_stream` (token-by-token) or `relay_collected_text`
///    (batch) to forward results to the client.
/// 4. **Persistence** (`persist_chat_records`) — writes the user message and all output
///    messages to the session's `.jsonl` file.
///
/// SSE protocol: every stream emits `start → (delta | tool_call_*)* → done → [DONE]`.
/// Errors at any stage are sent as `{"type":"error",…}` events.
///
/// **Architecture:** Spawned as a Tokio task from the `stream_chat` handler so that the
/// Salvo request handler can return immediately with the SSE channel receiver. The sender
/// half is passed into this function and never shared.
async fn execute_chat_streaming(
    state: Arc<crate::api::state::AppState>,
    broadcast_tx: WsBroadcast,
    body: ChatStreamRequest,
    mut sender: BodySender,
) {
    let ChatStreamRequest {
        agent_id,
        session_id: requested_session_id,
        messages: raw_messages,
        stream,
        model_config_id,
        image_understand_model_config_id,
        thinking_type,
        reasoning_effort,
        workspace_path,
        continuation,
    } = body;
    let is_continuation = continuation.is_some();
    let mut messages: Vec<LLMMessage> = raw_messages.into_iter().map(Into::into).collect();
    if let Some(decision) = continuation {
        let Some(session_id) =
            requested_session_id.as_deref().filter(|value| !value.trim().is_empty())
        else {
            let event =
                json!({ "type": "error", "error": "continuation requires an existing session" });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        };
        let latest = match load_chat_session_messages(session_id)
            .and_then(|records| Ok(records.last().cloned()))
        {
            Ok(Some(record)) => record,
            _ => {
                let event = json!({ "type": "error", "error": "no resumable tool-call limit prompt exists for this session" });
                let _ = sender.send_data(format!("data: {event}\n\n")).await;
                return;
            }
        };
        if latest
            .pending_ask_user
            .as_ref()
            .and_then(|request| request.tool_call_limit.as_ref())
            .is_none()
        {
            let event = json!({ "type": "error", "error": "the session is not waiting for a tool-call limit decision" });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
        if let Err(error) = append_tool_call_limit_decision_record(session_id, &latest, &decision) {
            let event = json!({ "type": "error", "error": error.to_string() });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
        match decision {
            ChatContinuationDecision::Stop => {
                match interrupt_workspace_tasks(session_id, "用户在工具调用上限处停止")
                {
                    Ok(snapshot) => {
                        let event = json!({ "type": "tool_call_limit_stopped", "session_id": session_id, "tasks": snapshot.tasks });
                        let _ = sender.send_data(format!("data: {event}\n\n")).await;
                        let _ = sender.send_data("data: [DONE]\n\n").await;
                    }
                    Err(error) => {
                        let event = json!({ "type": "error", "error": error });
                        let _ = sender.send_data(format!("data: {event}\n\n")).await;
                    }
                }
                return;
            }
            ChatContinuationDecision::Continue => {
                let records = match load_chat_session_messages(session_id) {
                    Ok(records) => records,
                    Err(error) => {
                        let event = json!({ "type": "error", "error": error.to_string() });
                        let _ = sender.send_data(format!("data: {event}\n\n")).await;
                        return;
                    }
                };
                messages = records
                    .into_iter()
                    .filter(|record| matches!(record.role.as_str(), "user" | "assistant" | "tool"))
                    .map(history_record_to_message)
                    .collect();
                messages.push(LLMMessage::system("用户已同意继续执行。请从现有状态继续完成任务。"));
            }
        }
    }
    let ChatAgentInfo { agent, agent_snapshot } =
        match resolve_chat_agent(&state.role_service_manager, &agent_id) {
            Ok(info) => info,
            Err(event) => {
                let _ = sender.send_data(format!("data: {event}\n\n")).await;
                return;
            }
        };

    messages = sanitize_messages(messages);
    if messages.is_empty() {
        let event =
            json!({ "type": "error", "error": "messages must not be empty after sanitization" });
        let _ = sender.send_data(format!("data: {event}\n\n")).await;
        return;
    }
    let latest_user_message = messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::User))
        .cloned();

    let trace_id = Uuid::new_v4().to_string();
    let effective_workspace_path = match resolve_effective_workspace_path(
        &agent,
        requested_session_id.as_deref(),
        workspace_path.as_deref(),
    ) {
        Ok(path) => path,
        Err(err) => {
            let event = json!({ "type": "error", "error": err.to_string() });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
    };

    let CommandDispatchOutcome {
        session_id,
        messages,
        latest_user_message,
        should_run_inference,
        should_persist,
        requires_assistant_message,
        immediate_output_messages,
    } = match try_dispatch_dashboard_command(
        &agent,
        &agent_snapshot,
        &requested_session_id,
        messages,
        &latest_user_message,
    ) {
        Ok(outcome) => outcome,
        Err(event) => {
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
    };

    // Turn context shared by the whole procedure chain (documents/procedure.md). The
    // new-conversation check must happen before any persistence: Workspace turns create the
    // session file with the user message right below.
    let is_new_conversation =
        !chat_session_file_path(&session_id).map(|path| path.exists()).unwrap_or(false);
    let (token_tx, mut token_rx) = mpsc::unbounded_channel::<StreamToken>();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Value>();
    let transport_sink: Arc<dyn TransportSink> =
        Arc::new(SseTransportSink { token_tx, event_tx: event_tx.clone() });
    let procedure_context = ProcedureContext {
        session_id: session_id.clone(),
        is_new_conversation,
        latest_user_text: latest_user_message.as_ref().and_then(LLMMessage::content_text_owned),
        workspace_path: effective_workspace_path.clone(),
        transport_out: Some(transport_sink),
        procedure_outputs: Vec::new(),
        role_context: None,
    };
    if !should_run_inference {
        // Command-only turns skip the brain, so the role's side-effect procedures (e.g. /new
        // session naming) run here; inference turns run the whole chain inside the spawned
        // turn task below.
        if let Err(err) = state
            .role_service_manager
            .run_procedure_chain(&agent, procedure_context.clone(), Vec::new())
            .await
        {
            log::error!("role procedures failed for agent '{}': {err}", agent.name);
        }
    }

    let assistant_message_id =
        requires_assistant_message.then(|| format!("msg_{}", Uuid::new_v4().simple()));

    let workspace_task = if should_run_inference && zihuan_service::role::is_workspace_agent(&agent)
    {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let task_id = state.tasks.lock().unwrap().add_workspace_chat_task(
            agent.id.clone(),
            session_id.clone(),
            agent_snapshot.name.clone(),
            effective_workspace_path.clone(),
            Arc::clone(&stop_flag),
        );
        let Some(task_id) = task_id else {
            let event = json!({ "type": "error", "error": "当前会话已有正在运行的 Workspace 任务，请切换到新对话后再发送。" });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        };
        let _ = broadcast_tx.send(ServerMessage::TaskStarted {
            task_id: task_id.clone(),
            graph_name: agent_snapshot.name.clone(),
            graph_session_id: session_id.clone(),
        });
        append_workspace_task_log(&state, &task_id, "INFO", "Workspace 聊天任务已开始");
        Some((task_id, stop_flag))
    } else {
        None
    };

    if workspace_task.is_some() {
        if let Err(err) = persist_chat_records(
            &session_id,
            &agent,
            &agent_snapshot,
            &trace_id,
            assistant_message_id.as_deref().unwrap_or(""),
            latest_user_message.as_ref(),
            &[],
            effective_workspace_path.clone(),
            None,
            None,
            !is_continuation,
            model_config_id.as_deref(),
            image_understand_model_config_id.as_deref(),
        ) {
            if let Some((task_id, _)) = &workspace_task {
                finish_workspace_task(
                    &state,
                    &broadcast_tx,
                    task_id,
                    TaskStatus::Failed,
                    Some(err.to_string()),
                    None,
                );
            }
            let event = json!({ "type": "error", "error": err.to_string() });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
    }

    if !send_sse(
        &mut sender,
        &build_chat_stream_event(
            "start",
            &session_id,
            assistant_message_id.as_deref(),
            workspace_task.as_ref().map(|(task_id, _)| task_id.as_str()),
        ),
    )
    .await
        && workspace_task.is_none()
    {
        return;
    }

    if !should_run_inference {
        if let (Some(ref output_messages), Some(ref msg_id)) =
            (&immediate_output_messages, &assistant_message_id)
        {
            if !emit_immediate_output(
                &mut sender,
                &session_id,
                msg_id,
                output_messages,
                should_persist,
                &agent,
                &agent_snapshot,
                &trace_id,
                latest_user_message.as_ref(),
                effective_workspace_path.clone(),
                model_config_id.as_deref(),
            )
            .await
            {
                return;
            }
        }

        let _ = send_sse(
            &mut sender,
            &build_chat_stream_event("done", &session_id, assistant_message_id.as_deref(), None),
        )
        .await;
        let _ = sender.send_data("data: [DONE]\n\n").await;
        return;
    }

    let assistant_message_id =
        assistant_message_id.expect("assistant_message_id must exist when inference is required");
    let running_chat_message = workspace_task.as_ref().map(|_| {
        let snapshot = Arc::new(Mutex::new(RunningChatMessage {
            message_id: assistant_message_id.clone(),
            agent_id: agent.id.clone(),
            agent_name: agent_snapshot.name.clone(),
            agent_type: agent_snapshot.role_service_type.clone(),
            agent_avatar_url: agent_snapshot.avatar_url.clone(),
            trace_id: trace_id.clone(),
            workspace_path: effective_workspace_path.clone(),
            model_config_id: model_config_id.clone(),
            image_understand_model_config_id: image_understand_model_config_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            content: String::new(),
            reasoning_content: String::new(),
            live_tool_calls: Vec::new(),
        }));
        state
            .running_chat_messages
            .lock()
            .unwrap()
            .insert(session_id.clone(), Arc::clone(&snapshot));
        snapshot
    });

    let inference_started_at = Instant::now();
    let cancellation: Option<Arc<dyn AgentCancellation>> =
        workspace_task.as_ref().map(|(_, stop_flag)| {
            Arc::new(WorkspaceChatCancellation(Arc::clone(stop_flag))) as Arc<dyn AgentCancellation>
        });
    let turn_handle = {
        let manager = state.role_service_manager.clone();
        let agent_id = agent_id.clone();
        let agent_config = agent.clone();
        let procedure_context = procedure_context;
        let turn_event_tx = event_tx;
        let turn_session_id = session_id.clone();
        let turn_workspace_path = effective_workspace_path.clone();
        let turn_model_config_id = model_config_id.clone();
        let turn_image_understand_model_config_id = image_understand_model_config_id.clone();
        let turn_running_chat_message = running_chat_message.clone();
        let turn_assistant_message_id = assistant_message_id.clone();
        tokio::spawn(async move {
            // Resolve the running agent at turn time like the old inference task did; a
            // missing service flows through the standard turn failure handling below.
            let Some(role_service) = manager.running_role_service(&agent_id) else {
                return Err(zihuan_core::string_error!("agent '{agent_id}' is not running"));
            };
            // Bind this turn's model overrides into a turn-scoped agent at inference time, so
            // the agent handed to the brain procedure already carries its models. With no
            // override the role's own configured binding is used as-is.
            let brain_agent: Arc<RoleAgent> = if turn_model_config_id.is_some()
                || turn_image_understand_model_config_id.is_some()
            {
                let llm_refs = load_llm_refs()?;
                let (llm, model_name) = match turn_model_config_id.as_deref() {
                    Some(model_id) => {
                        let mut llm_config = resolve_llm_service_config(
                            Some(model_id),
                            &llm_refs,
                            role_service.agent().name.as_str(),
                        )?;
                        if let Some(override_value) = thinking_type {
                            llm_config.thinking_type = Some(override_value);
                        }
                        if let Some(override_value) = reasoning_effort {
                            llm_config.reasoning_effort = Some(override_value);
                        }
                        let model_name = llm_config.model_name.clone();
                        (build_llm_model(&llm_config)?, model_name)
                    }
                    None => (role_service.llm(), role_service.model_name().to_string()),
                };
                let image_understand_llm = match turn_image_understand_model_config_id.as_deref() {
                    Some(image_model_id) => {
                        let llm_config = resolve_llm_service_config(
                            Some(image_model_id),
                            &llm_refs,
                            role_service.agent().name.as_str(),
                        )?;
                        Some(build_llm_model(&llm_config)?)
                    }
                    None => None,
                };
                Arc::new(role_service.with_llm_override(llm, model_name, image_understand_llm))
            } else {
                role_service
            };
            let observer: Arc<dyn ToolCallingObserver> = Arc::new(SseToolCallingObserver {
                event_tx: turn_event_tx.clone(),
                session_id: turn_session_id.clone(),
                message_id: turn_assistant_message_id.clone(),
                change_recorder: workspace_changes::WorkspaceChangeRecorder::new(
                    turn_session_id.clone(),
                    turn_workspace_path.clone(),
                ),
                running_chat_message: turn_running_chat_message.clone(),
            });
            let compaction_observer: ContextCompactionObserver = {
                let event_tx = turn_event_tx;
                let message_id = turn_assistant_message_id;
                Arc::new(move |event| {
                    let payload = match event {
                        ContextCompactionEvent::Started => json!({
                            "type": "context_compaction_start",
                            "message_id": message_id,
                        }),
                        ContextCompactionEvent::Completed {
                            estimated_tokens_before,
                            estimated_tokens_after,
                            duration,
                        } => json!({
                            "type": "context_compaction_complete",
                            "message_id": message_id,
                            "estimated_tokens_before": estimated_tokens_before,
                            "estimated_tokens_after": estimated_tokens_after,
                            "duration_ms": duration.as_millis() as u64,
                        }),
                        ContextCompactionEvent::Failed => json!({
                            "type": "context_compaction_failed",
                            "message_id": message_id,
                        }),
                    };
                    let _ = event_tx.send(payload);
                })
            };
            let brain = Arc::new(WorkspaceBrain::<RoleAgent>::new(
                brain_agent,
                messages,
                Some(turn_session_id),
                turn_workspace_path,
                cancellation,
                Some(observer),
                Some(compaction_observer),
            ));
            manager.run_procedure_chain(&agent_config, procedure_context, vec![brain]).await
        })
    };

    let stop_watch = workspace_task.as_ref().map(|(_, flag)| {
        let flag = Arc::clone(flag);
        let abort_handle = turn_handle.abort_handle();
        tokio::spawn(async move {
            while !flag.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            abort_handle.abort();
        })
    });

    let stream_enabled = stream.unwrap_or(true);
    let (client_connected, relay_timing) = if stream_enabled {
        relay_inference_stream(
            &mut sender,
            &assistant_message_id,
            &mut token_rx,
            &mut event_rx,
            inference_started_at,
            running_chat_message.as_ref(),
        )
        .await
    } else {
        relay_collected_text(&mut sender, &assistant_message_id, &mut token_rx, &mut event_rx)
            .await;
        (true, RelayTiming::default())
    };

    // A disconnected SSE client must not cancel a Workspace task. The relay has
    // already stopped consuming events, but the inference result is still awaited
    // and persisted below.

    let (output_messages, stop_reason) = {
        let turn_result = match turn_handle.await {
            Ok(result) => result,
            Err(err) => Err(zihuan_core::string_error!("failed to join chat task: {err}")),
        };
        match turn_result.and_then(|outputs| {
            outputs
                .iter()
                .rev()
                .find_map(|output| output.cloned::<(Vec<LLMMessage>, ToolCallingStopReason)>())
                .ok_or_else(|| {
                    zihuan_core::string_error!("brain procedure produced no turn output")
                })
        }) {
            Ok(result) => result,
            Err(err) => {
                if let Some(snapshot) = running_chat_message.as_ref() {
                    if let Err(persist_err) =
                        persist_running_chat_message(&session_id, &agent, &agent_snapshot, snapshot)
                    {
                        log::warn!("failed to persist running chat message after inference error: {persist_err}");
                    }
                }
                clear_running_chat_message(&state, &session_id, running_chat_message.as_ref());
                let stopped =
                    workspace_task.as_ref().is_some_and(|(_, flag)| flag.load(Ordering::Relaxed));
                if let Some((task_id, _)) = &workspace_task {
                    finish_workspace_task(
                        &state,
                        &broadcast_tx,
                        task_id,
                        if stopped {
                            TaskStatus::Stopped
                        } else {
                            TaskStatus::Failed
                        },
                        (!stopped).then(|| err.to_string()),
                        None,
                    );
                }
                if stopped {
                    if let Err(error) = interrupt_workspace_tasks(&session_id, "用户手动停止推理")
                    {
                        log::warn!("failed to interrupt workspace tasks: {error}");
                    }
                    return;
                }
                let event = json!({ "type": "error", "error": err.to_string() });
                if client_connected {
                    let _ = sender.send_data(format!("data: {event}\n\n")).await;
                }
                return;
            }
        }
    };

    if let Some(watch) = stop_watch {
        watch.abort();
    }
    if workspace_task.as_ref().is_some_and(|(_, flag)| flag.load(Ordering::Relaxed)) {
        if let Some(snapshot) = running_chat_message.as_ref() {
            if let Err(err) =
                persist_running_chat_message(&session_id, &agent, &agent_snapshot, snapshot)
            {
                if client_connected {
                    let event = json!({ "type": "error", "error": err.to_string() });
                    let _ = sender.send_data(format!("data: {event}\n\n")).await;
                }
            }
        }
        clear_running_chat_message(&state, &session_id, running_chat_message.as_ref());
        if let Some((task_id, _)) = &workspace_task {
            finish_workspace_task(&state, &broadcast_tx, task_id, TaskStatus::Stopped, None, None);
        }
        if zihuan_service::role::is_workspace_agent(&agent) {
            if let Err(error) = interrupt_workspace_tasks(&session_id, "用户手动停止推理") {
                log::warn!("failed to interrupt workspace tasks: {error}");
            }
        }
        return;
    }

    let metrics = build_chat_response_metrics(
        &output_messages,
        stream_enabled.then_some(relay_timing.first_token_after_start).flatten(),
        stream_enabled.then_some(inference_started_at.elapsed()),
    );

    if let Err(err) = persist_chat_records(
        &session_id,
        &agent,
        &agent_snapshot,
        &trace_id,
        &assistant_message_id,
        latest_user_message.as_ref(),
        &output_messages,
        effective_workspace_path.clone(),
        match &stop_reason {
            ToolCallingStopReason::AwaitUserInput(request)
            | ToolCallingStopReason::ToolCallLimitReached(request) => Some(request.clone()),
            _ => None,
        },
        metrics.as_ref(),
        workspace_task.is_none(),
        model_config_id.as_deref(),
        image_understand_model_config_id.as_deref(),
    ) {
        clear_running_chat_message(&state, &session_id, running_chat_message.as_ref());
        if let Some((task_id, _)) = &workspace_task {
            finish_workspace_task(
                &state,
                &broadcast_tx,
                task_id,
                TaskStatus::Failed,
                Some(err.to_string()),
                None,
            );
        }
        let event = json!({ "type": "error", "error": err.to_string() });
        if client_connected {
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
        }
        return;
    }

    if let Some((task_id, _)) = &workspace_task {
        let summary = output_messages.iter().rev().find_map(|message| message.content_text_owned());
        finish_workspace_task(&state, &broadcast_tx, task_id, TaskStatus::Success, None, summary);
    }
    clear_running_chat_message(&state, &session_id, running_chat_message.as_ref());

    if let Some(metrics) = &metrics {
        let event = json!({
            "type": "metrics",
            "message_id": assistant_message_id,
            "metrics": metrics,
        });
        if client_connected && !send_sse(&mut sender, &event).await {
            return;
        }
    }

    if let ToolCallingStopReason::AwaitUserInput(request)
    | ToolCallingStopReason::ToolCallLimitReached(request) = stop_reason
    {
        let event = json!({
            "type": "ask_user",
            "session_id": session_id,
            "message_id": assistant_message_id,
            "question": request.question,
            "details": request.details,
            "placeholder": request.placeholder,
            "command_confirmation": request.command_confirmation,
            "tool_call_limit": request.tool_call_limit,
        });
        if client_connected && !send_sse(&mut sender, &event).await {
            return;
        }
    }

    if client_connected {
        let _ = send_sse(
            &mut sender,
            &build_chat_stream_event("done", &session_id, Some(&assistant_message_id), None),
        )
        .await;
        let _ = sender.send_data("data: [DONE]\n\n").await;
    }
}

/// Build a top-level SSE event (`start` / `done`) with optional `message_id`.
fn build_chat_stream_event(
    kind: &str,
    session_id: &str,
    message_id: Option<&str>,
    task_id: Option<&str>,
) -> Value {
    match message_id {
        Some(message_id) => json!({
            "type": kind,
            "session_id": session_id,
            "message_id": message_id,
            "task_id": task_id,
        }),
        None => json!({
            "type": kind,
            "session_id": session_id,
            "task_id": task_id,
        }),
    }
}

fn finish_workspace_task(
    state: &crate::api::state::AppState,
    broadcast_tx: &WsBroadcast,
    task_id: &str,
    status: TaskStatus,
    error: Option<String>,
    summary: Option<String>,
) {
    let level = match status {
        TaskStatus::Success => "INFO",
        TaskStatus::Stopped => "WARN",
        _ => "ERROR",
    };
    let message = error
        .as_deref()
        .unwrap_or_else(|| summary.as_deref().unwrap_or("Workspace 聊天任务已结束"));
    append_workspace_task_log(state, task_id, level, message);
    state
        .tasks
        .lock()
        .unwrap()
        .finish_task(task_id, status.clone(), error.clone(), summary);
    match status {
        TaskStatus::Success => {
            let _ = broadcast_tx.send(ServerMessage::TaskFinished {
                task_id: task_id.to_string(),
                success: true,
                error: None,
            });
        }
        TaskStatus::Failed => {
            let _ = broadcast_tx.send(ServerMessage::TaskFinished {
                task_id: task_id.to_string(),
                success: false,
                error,
            });
        }
        TaskStatus::Stopped => {
            let _ = broadcast_tx.send(ServerMessage::TaskStopped { task_id: task_id.to_string() });
        }
        _ => {}
    }
}

fn append_workspace_task_log(
    state: &crate::api::state::AppState,
    task_id: &str,
    level: &str,
    message: &str,
) {
    let entry = crate::api::state::TaskLogEntry {
        timestamp: Utc::now().to_rfc3339(),
        level: level.to_string(),
        message: message.to_string(),
    };
    if let Err(err) = state.tasks.lock().unwrap().append_task_log(task_id, &entry) {
        log::warn!("failed to append Workspace task log '{}': {}", task_id, err);
    }
}

fn build_chat_response_metrics(
    output_messages: &[LLMMessage],
    first_token_after_start: Option<Duration>,
    total_duration: Option<Duration>,
) -> Option<ChatResponseMetrics> {
    let usage = aggregate_assistant_usage(output_messages);
    let generation_duration = first_token_after_start
        .and_then(|first_token| total_duration.and_then(|total| total.checked_sub(first_token)));
    let output_tokens_per_second =
        match (usage.as_ref().and_then(|usage| usage.completion_tokens), generation_duration) {
            (Some(completion_tokens), Some(duration)) if !duration.is_zero() => {
                Some(completion_tokens as f64 / duration.as_secs_f64())
            }
            _ => None,
        };
    let cache_hit_rate = match (
        usage.as_ref().and_then(|usage| usage.cached_prompt_tokens),
        usage.as_ref().and_then(|usage| usage.prompt_tokens),
    ) {
        (Some(cached_prompt_tokens), Some(prompt_tokens)) if prompt_tokens > 0 => {
            Some(cached_prompt_tokens as f64 / prompt_tokens as f64)
        }
        _ => None,
    };

    let metrics = ChatResponseMetrics {
        time_to_first_token_ms: first_token_after_start.map(duration_to_millis),
        generation_duration_ms: generation_duration.map(duration_to_millis),
        output_tokens_per_second,
        prompt_tokens: usage.as_ref().and_then(|usage| usage.prompt_tokens),
        cached_prompt_tokens: usage.as_ref().and_then(|usage| usage.cached_prompt_tokens),
        prompt_cache_miss_tokens: usage.as_ref().and_then(|usage| usage.prompt_cache_miss_tokens),
        completion_tokens: usage.as_ref().and_then(|usage| usage.completion_tokens),
        total_tokens: usage.as_ref().and_then(|usage| usage.total_tokens),
        cache_hit_rate,
    };

    (metrics.time_to_first_token_ms.is_some()
        || metrics.generation_duration_ms.is_some()
        || metrics.prompt_tokens.is_some()
        || metrics.cached_prompt_tokens.is_some()
        || metrics.prompt_cache_miss_tokens.is_some()
        || metrics.completion_tokens.is_some()
        || metrics.total_tokens.is_some())
    .then_some(metrics)
}

// Original aggregation (disabled): summing usage across every assistant message
// in the tool loop double-counts the accumulated history on each iteration.
//
// fn aggregate_assistant_usage(output_messages: &[LLMMessage]) -> Option<TokenUsage> {
//     let mut usage = TokenUsage::default();
//     let mut has_usage = false;
//
//     for message in output_messages {
//         if !matches!(message.role, MessageRole::Assistant) {
//             continue;
//         }
//         let Some(message_usage) = &message.usage else {
//             continue;
//         };
//         has_usage = true;
//         add_optional_token_count(&mut usage.prompt_tokens, message_usage.prompt_tokens);
//         add_optional_token_count(&mut usage.cached_prompt_tokens, message_usage.cached_prompt_tokens);
//         add_optional_token_count(
//             &mut usage.prompt_cache_miss_tokens,
//             message_usage.prompt_cache_miss_tokens,
//         );
//         add_optional_token_count(&mut usage.completion_tokens, message_usage.completion_tokens);
//         add_optional_token_count(&mut usage.total_tokens, message_usage.total_tokens);
//     }
//
//     has_usage.then_some(usage)
// }
//
// fn add_optional_token_count(total: &mut Option<usize>, value: Option<usize>) {
//     if let Some(value) = value {
//         *total = Some(total.unwrap_or(0) + value);
//     }
// }

/// take the usage of the last LLM call only (the final assistant
/// message in the output that carries usage). Each iteration of the tool loop
/// sends the full context accumulated up to that point (history + tool
/// definitions + previous assistant/tool messages), so summing every message
/// would count the same history input multiple times and inflate total_tokens.
/// The final request's usage is the complete context actually sent for this
/// reply.
fn aggregate_assistant_usage(output_messages: &[LLMMessage]) -> Option<TokenUsage> {
    output_messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::Assistant) && message.usage.is_some())
        .and_then(|message| message.usage.clone())
}

fn duration_to_millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

fn append_running_chat_message(
    messages: &mut Vec<ChatHistoryRecord>,
    session_id: &str,
    state: &crate::api::state::AppState,
) {
    let snapshot = state
        .running_chat_messages
        .lock()
        .unwrap()
        .get(session_id)
        .map(|snapshot| snapshot.lock().unwrap().clone());
    let Some(snapshot) = snapshot else {
        return;
    };

    if messages.iter().any(|message| message.message_id == snapshot.message_id) {
        return;
    }

    messages.push(ChatHistoryRecord {
        session_id: session_id.to_string(),
        agent_id: snapshot.agent_id,
        agent_name: snapshot.agent_name,
        role_service_type: snapshot.agent_type,
        agent_avatar_url: snapshot.agent_avatar_url,
        role: "assistant".to_string(),
        content: snapshot.content,
        parts: Vec::new(),
        reasoning_content: (!snapshot.reasoning_content.is_empty())
            .then_some(snapshot.reasoning_content),
        timestamp: snapshot.timestamp,
        stream_index: None,
        streaming: true,
        live_tool_calls: snapshot.live_tool_calls,
        trace_id: snapshot.trace_id,
        message_id: snapshot.message_id,
        tool_calls: Vec::new(),
        tool_call_id: None,
        workspace_path: snapshot.workspace_path,
        model_config_id: snapshot.model_config_id,
        image_understand_model_config_id: None,
        pending_ask_user: None,
        metrics: None,
    });
}

fn clear_running_chat_message(
    state: &crate::api::state::AppState,
    session_id: &str,
    snapshot: Option<&Arc<Mutex<RunningChatMessage>>>,
) {
    let Some(snapshot) = snapshot else {
        return;
    };
    let mut running_messages = state.running_chat_messages.lock().unwrap();
    if running_messages
        .get(session_id)
        .is_some_and(|current| Arc::ptr_eq(current, snapshot))
    {
        running_messages.remove(session_id);
    }
}

/// Persist the assistant output observed before a Workspace task exits without a final result.
///
/// The user message is persisted when the task starts, while the assistant record normally
/// waits for inference to finish. On an interrupted or failed completion path, retain the live
/// snapshot instead of clearing it so a history reload does not discard streamed text or tool
/// activity.
fn persist_running_chat_message(
    session_id: &str,
    agent: &RoleServiceConfig,
    agent_snapshot: &AgentSnapshot,
    snapshot: &Arc<Mutex<RunningChatMessage>>,
) -> Result<()> {
    let snapshot = snapshot.lock().unwrap().clone();
    if snapshot.content.is_empty()
        && snapshot.reasoning_content.is_empty()
        && snapshot.live_tool_calls.is_empty()
    {
        return Ok(());
    }

    append_history_record(&ChatHistoryRecord {
        session_id: session_id.to_string(),
        agent_id: agent.id.clone(),
        agent_name: agent_snapshot.name.clone(),
        role_service_type: agent_snapshot.role_service_type.clone(),
        agent_avatar_url: agent_snapshot.avatar_url.clone(),
        role: "assistant".to_string(),
        content: snapshot.content,
        parts: Vec::new(),
        reasoning_content: (!snapshot.reasoning_content.is_empty())
            .then_some(snapshot.reasoning_content),
        timestamp: snapshot.timestamp,
        stream_index: None,
        streaming: false,
        live_tool_calls: snapshot.live_tool_calls,
        trace_id: snapshot.trace_id,
        message_id: snapshot.message_id,
        tool_calls: Vec::new(),
        tool_call_id: None,
        workspace_path: snapshot.workspace_path,
        model_config_id: snapshot.model_config_id,
        image_understand_model_config_id: None,
        pending_ask_user: None,
        metrics: None,
    })
}

/// Strip messages whose text content is empty/whitespace-only and has no tool calls.
///
/// **Purpose:** Prevents degenerate inputs (e.g. trailing empty user messages) from reaching
/// the LLM, which would cause API errors.
fn sanitize_messages(messages: Vec<LLMMessage>) -> Vec<LLMMessage> {
    messages
        .into_iter()
        .filter(|message| {
            let has_content =
                message.content_text_owned().is_some_and(|text| !text.trim().is_empty());
            let has_reasoning =
                message.reasoning_content.as_deref().is_some_and(|text| !text.trim().is_empty());
            has_content
                || has_reasoning
                || !message.parts.is_empty()
                || !message.tool_calls.is_empty()
        })
        .collect()
}

/// Replace the last user-role message in the list, or append if none exists.
///
/// **Purpose:** Used by command dispatch when a passthrough command rewrites the user message
/// in-place rather than appending.
fn replace_last_user_message(messages: &mut Vec<LLMMessage>, replacement: LLMMessage) {
    if let Some(index) =
        messages.iter().rposition(|message| matches!(message.role, MessageRole::User))
    {
        messages[index] = replacement;
    } else {
        messages.push(replacement);
    }
}

/// Write the user message (if any) and all output messages to the session's JSONL file.
///
/// **Purpose:** Provides durable chat history that survives server restarts. Each call
/// atomically appends one record per message — no transaction is needed because JSONL files
/// are append-only and tolerant of partial writes.
///
/// **Design:** The assistant message that corresponds to the streaming response reuses
/// `assistant_message_id` so the frontend can correlate deltas with the stored record.
/// Tool-call and tool-result messages get fresh random IDs since they are not streamed
/// individually.
fn persist_chat_records(
    session_id: &str,
    agent: &RoleServiceConfig,
    agent_snapshot: &AgentSnapshot,
    trace_id: &str,
    assistant_message_id: &str,
    latest_user_message: Option<&LLMMessage>,
    output_messages: &[LLMMessage],
    workspace_path: Option<String>,
    pending_ask_user: Option<AskUserRequest>,
    metrics: Option<&ChatResponseMetrics>,
    include_user_message: bool,
    model_config_id: Option<&str>,
    image_understand_model_config_id: Option<&str>,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    if include_user_message {
        if let Some(user_message) = latest_user_message {
            let user_record = ChatHistoryRecord {
                session_id: session_id.to_string(),
                agent_id: agent.id.clone(),
                agent_name: agent_snapshot.name.clone(),
                role_service_type: agent_snapshot.role_service_type.clone(),
                agent_avatar_url: agent_snapshot.avatar_url.clone(),
                role: "user".to_string(),
                content: user_message.content_text_owned().unwrap_or_default(),
                parts: user_message.parts.clone(),
                reasoning_content: None,
                timestamp: now.clone(),
                stream_index: None,
                streaming: false,
                live_tool_calls: Vec::new(),
                trace_id: trace_id.to_string(),
                message_id: format!("msg_{}", Uuid::new_v4().simple()),
                tool_calls: Vec::new(),
                tool_call_id: None,
                workspace_path: workspace_path.clone(),
                model_config_id: model_config_id.map(str::to_string),
                image_understand_model_config_id: image_understand_model_config_id
                    .map(str::to_string),
                pending_ask_user: None,
                metrics: None,
            };
            append_history_record(&user_record)?;
        }
    }

    for message in output_messages {
        let role = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::System => "system",
        };
        let record = ChatHistoryRecord {
            session_id: session_id.to_string(),
            agent_id: agent.id.clone(),
            agent_name: agent_snapshot.name.clone(),
            role_service_type: agent_snapshot.role_service_type.clone(),
            agent_avatar_url: agent_snapshot.avatar_url.clone(),
            role: role.to_string(),
            content: message.content_text_owned().unwrap_or_default(),
            parts: message.parts.clone(),
            reasoning_content: message.reasoning_content.clone(),
            timestamp: now.clone(),
            stream_index: None,
            streaming: false,
            live_tool_calls: Vec::new(),
            trace_id: trace_id.to_string(),
            message_id: if matches!(message.role, MessageRole::Assistant)
                && message.tool_calls.is_empty()
            {
                assistant_message_id.to_string()
            } else {
                format!("msg_{}", Uuid::new_v4().simple())
            },
            tool_calls: message.tool_calls.clone(),
            tool_call_id: message.tool_call_id.clone(),
            workspace_path: workspace_path.clone(),
            model_config_id: model_config_id.map(str::to_string),
            image_understand_model_config_id: image_understand_model_config_id.map(str::to_string),
            pending_ask_user: pending_ask_user.clone(),
            metrics: if matches!(message.role, MessageRole::Assistant)
                && message.tool_calls.is_empty()
            {
                metrics.cloned()
            } else {
                None
            },
        };
        append_history_record(&record)?;
    }

    Ok(())
}

/// Append a single `ChatHistoryRecord` as a JSON line to the session file.
///
/// **Purpose:** The lowest-level persistence primitive — every message in every session passes
/// through here. Creates the file and parent directories if they don't exist.
fn append_history_record(record: &ChatHistoryRecord) -> Result<()> {
    let path = chat_session_file_path(&record.session_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, record)
        .map_err(|err| zihuan_core::string_error!("failed to serialize chat record: {err}"))?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn append_tool_call_limit_decision_record(
    session_id: &str,
    latest: &ChatHistoryRecord,
    decision: &ChatContinuationDecision,
) -> Result<()> {
    let content = match decision {
        ChatContinuationDecision::Continue => "用户已同意继续执行。",
        ChatContinuationDecision::Stop => "用户已在工具调用上限处停止执行。",
    };
    append_history_record(&ChatHistoryRecord {
        session_id: session_id.to_string(),
        agent_id: latest.agent_id.clone(),
        agent_name: latest.agent_name.clone(),
        role_service_type: latest.role_service_type.clone(),
        agent_avatar_url: latest.agent_avatar_url.clone(),
        role: "system".to_string(),
        content: content.to_string(),
        parts: vec![MessagePart::text(content)],
        reasoning_content: None,
        timestamp: Utc::now().to_rfc3339(),
        stream_index: None,
        streaming: false,
        live_tool_calls: Vec::new(),
        trace_id: latest.trace_id.clone(),
        message_id: format!("msg_{}", Uuid::new_v4().simple()),
        tool_calls: Vec::new(),
        tool_call_id: None,
        workspace_path: latest.workspace_path.clone(),
        model_config_id: latest.model_config_id.clone(),
        image_understand_model_config_id: latest.image_understand_model_config_id.clone(),
        pending_ask_user: None,
        metrics: None,
    })
}

fn fork_chat_session_history(source_session_id: &str, source_message_id: &str) -> Result<String> {
    let records = load_chat_session_messages(source_session_id)?;
    let Some(message_index) = records
        .iter()
        .position(|record| record.message_id == source_message_id && record.role == "user")
    else {
        return Err(Error::ValidationError(
            "only an existing user message can be forked".to_string(),
        ));
    };

    let forked_session_id = Uuid::new_v4().to_string();
    let mut prefix = records[..message_index].to_vec();
    for record in &mut prefix {
        record.session_id = forked_session_id.clone();
    }
    write_chat_session_records(&forked_session_id, &prefix)?;

    let metadata = ChatForkMetadata {
        source_session_id: source_session_id.to_string(),
        source_message_id: source_message_id.to_string(),
        fork_group_id: resolve_fork_group_id(source_session_id, source_message_id)?,
        prefix_record_count: prefix.len(),
        created_at: Utc::now().to_rfc3339(),
    };
    write_fork_metadata(&forked_session_id, &metadata)?;
    if let Some(title) = load_session_title(source_session_id)? {
        write_session_title(&forked_session_id, &title)?;
    }
    Ok(forked_session_id)
}

fn write_chat_session_records(session_id: &str, records: &[ChatHistoryRecord]) -> Result<()> {
    let path = chat_session_file_path(session_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    for record in records {
        serde_json::to_writer(&mut file, record).map_err(|err| {
            zihuan_core::string_error!("failed to serialize fork history record: {err}")
        })?;
        file.write_all(b"\n")?;
    }
    file.flush()?;
    Ok(())
}

fn resolve_fork_group_id(session_id: &str, message_id: &str) -> Result<String> {
    let Some(metadata) = load_fork_metadata(session_id)? else {
        return Ok(message_id.to_string());
    };
    let records = load_chat_session_messages(session_id)?;
    if records
        .get(metadata.prefix_record_count)
        .is_some_and(|record| record.message_id == message_id)
    {
        return Ok(metadata.fork_group_id);
    }
    resolve_fork_group_id(&metadata.source_session_id, message_id)
}

fn load_message_branches(
    session_id: &str,
    records: &[ChatHistoryRecord],
) -> Result<Vec<ChatMessageBranch>> {
    let metadata_by_session = load_all_fork_metadata()?;
    let mut branches = Vec::new();
    for record in records.iter().filter(|record| record.role == "user") {
        let group_id = resolve_fork_group_id(session_id, &record.message_id)?;
        let mut versions = Vec::new();
        for (forked_session_id, metadata) in &metadata_by_session {
            if metadata.fork_group_id != group_id {
                continue;
            }
            versions.push(ChatBranchVersion {
                session_id: metadata.source_session_id.clone(),
                message_id: metadata.source_message_id.clone(),
            });
            let forked_records = load_chat_session_messages(forked_session_id)?;
            if let Some(forked_message) = forked_records.get(metadata.prefix_record_count) {
                versions.push(ChatBranchVersion {
                    session_id: forked_session_id.clone(),
                    message_id: forked_message.message_id.clone(),
                });
            }
        }
        let mut distinct_versions = Vec::new();
        for version in versions {
            if distinct_versions
                .iter()
                .any(|existing: &ChatBranchVersion| existing.session_id == version.session_id)
            {
                continue;
            }
            distinct_versions.push(version);
        }
        let versions = distinct_versions;
        if versions.len() < 2 {
            continue;
        }
        let current_index = versions
            .iter()
            .position(|version| version.session_id == session_id)
            .unwrap_or(0);
        branches.push(ChatMessageBranch {
            message_id: record.message_id.clone(),
            current_index,
            total: versions.len(),
            versions,
        });
    }
    Ok(branches)
}

fn load_all_fork_metadata() -> Result<Vec<(String, ChatForkMetadata)>> {
    let dir = chat_history_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut metadata = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(session_id) = file_name.strip_suffix(CHAT_FORK_METADATA_SUFFIX) else {
            continue;
        };
        if let Some(item) = load_fork_metadata(session_id)? {
            metadata.push((session_id.to_string(), item));
        }
    }
    metadata.sort_by(|left, right| left.1.created_at.cmp(&right.1.created_at));
    Ok(metadata)
}

fn load_fork_metadata(session_id: &str) -> Result<Option<ChatForkMetadata>> {
    let path = chat_fork_metadata_path(session_id)?;
    if !path.exists() {
        return Ok(None);
    }
    let file = OpenOptions::new().read(true).open(path)?;
    serde_json::from_reader(file)
        .map(Some)
        .map_err(|err| zihuan_core::string_error!("failed to parse chat fork metadata: {err}"))
}

fn write_fork_metadata(session_id: &str, metadata: &ChatForkMetadata) -> Result<()> {
    let path = chat_fork_metadata_path(session_id)?;
    let file = OpenOptions::new().create_new(true).write(true).open(path)?;
    serde_json::to_writer(file, metadata)
        .map_err(|err| zihuan_core::string_error!("failed to serialize chat fork metadata: {err}"))
}

fn load_chat_sessions(filter_agent_id: Option<&str>) -> Result<Vec<ChatSessionSummary>> {
    let dir = chat_history_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }

        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };

        let updated_at = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .map(DateTime::<Utc>::from)
            .map(|time| time.to_rfc3339())
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        let first_record = read_first_record(&path).ok().flatten();
        let first_user_message = read_first_user_message(&path).ok().flatten();
        let title = load_session_title(stem)
            .ok()
            .flatten()
            .unwrap_or_else(|| build_session_title(first_user_message.as_deref(), stem));

        if let Some(filter) = filter_agent_id {
            if first_record.as_ref().map(|r| r.agent_id.as_str()) != Some(filter) {
                continue;
            }
        }

        sessions.push(ChatSessionSummary {
            session_id: stem.to_string(),
            root_session_id: resolve_root_session_id(stem)?,
            updated_at,
            agent_id: first_record.as_ref().map(|r| r.agent_id.clone()),
            agent_name: first_record.as_ref().map(|r| r.agent_name.clone()),
            role_service_type: first_record.as_ref().map(|r| r.role_service_type.clone()),
            agent_avatar_url: first_record.as_ref().and_then(|r| r.agent_avatar_url.clone()),
            workspace_path: read_last_record(&path).ok().flatten().and_then(|r| r.workspace_path),
            pending_ask_user: read_last_record(&path)
                .ok()
                .flatten()
                .and_then(|r| r.pending_ask_user),
            title,
        });
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

/// Resolve the original session of a fork lineage so the client can present all edits and
/// resends as versions of one conversation.
fn resolve_root_session_id(session_id: &str) -> Result<String> {
    let mut current_session_id = session_id.to_string();
    let mut visited = HashSet::new();

    while let Some(metadata) = load_fork_metadata(&current_session_id)? {
        if !visited.insert(current_session_id.clone()) {
            return Err(zihuan_core::string_error!(
                "detected a cycle in chat fork metadata for session {session_id}"
            ));
        }
        current_session_id = metadata.source_session_id;
    }

    Ok(current_session_id)
}

fn load_chat_session_messages(session_id: &str) -> Result<Vec<ChatHistoryRecord>> {
    let path = chat_session_file_path(session_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = OpenOptions::new().read(true).open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ChatHistoryRecord>(&line) {
            Ok(record) => entries.push(record),
            Err(err) => {
                return Err(zihuan_core::string_error!("failed to parse chat record: {err}"))
            }
        }
    }
    Ok(entries)
}

fn read_first_record(path: &Path) -> Result<Option<ChatHistoryRecord>> {
    let file = OpenOptions::new().read(true).open(path)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 || line.trim().is_empty() {
        return Ok(None);
    }

    let record: ChatHistoryRecord = serde_json::from_str(line.trim()).map_err(|err| {
        zihuan_core::string_error!("failed to parse first chat history record: {err}")
    })?;
    Ok(Some(record))
}

fn read_last_record(path: &Path) -> Result<Option<ChatHistoryRecord>> {
    let records = load_chat_session_messages(
        path.file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| Error::ValidationError("invalid session file name".to_string()))?,
    )?;
    Ok(records.into_iter().last())
}

/// Return the content of the first user message in a session file.
fn read_first_user_message(path: &Path) -> Result<Option<String>> {
    let file = OpenOptions::new().read(true).open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<ChatHistoryRecord>(line.trim()) {
            Ok(record) if record.role == "user" => return Ok(Some(record.content)),
            Ok(_) => continue,
            Err(err) => {
                return Err(zihuan_core::string_error!(
                    "failed to parse chat history record: {err}"
                ));
            }
        }
    }
    Ok(None)
}

/// Build a display title for a session from the first user message.
fn build_session_title(raw: Option<&str>, session_id: &str) -> String {
    let message = raw.map(str::trim).unwrap_or_default();
    let message = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if message.is_empty() {
        return session_id.chars().take(8).collect();
    }
    const MAX_TITLE_LEN: usize = 30;
    if message.chars().count() <= MAX_TITLE_LEN {
        return message;
    }
    let truncated: String = message.chars().take(MAX_TITLE_LEN).collect();
    format!("{truncated}…")
}

fn resolve_effective_workspace_path(
    agent: &RoleServiceConfig,
    session_id: Option<&str>,
    requested_workspace_path: Option<&str>,
) -> Result<Option<String>> {
    if !zihuan_service::role::is_workspace_agent(&agent) {
        return Ok(None);
    }

    if let Some(path) = normalized_workspace_path(requested_workspace_path) {
        return Ok(Some(path));
    }

    if let Some(existing_session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) {
        let records = load_chat_session_messages(existing_session_id)?;
        if let Some(path) = records.iter().rev().find_map(|record| record.workspace_path.clone()) {
            return Ok(Some(path));
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        return Ok(Some(cwd.to_string_lossy().to_string()));
    }

    Err(Error::ValidationError(
        "Workspace Agent Service requires a workspace_path for new sessions and could not determine current directory"
            .to_string(),
    ))
}

fn chat_session_file_path(session_id: &str) -> Result<PathBuf> {
    if session_id.trim().is_empty() {
        return Err(Error::ValidationError("session_id must not be empty".to_string()));
    }
    Ok(chat_history_dir()?.join(format!("{session_id}.jsonl")))
}

fn chat_fork_metadata_path(session_id: &str) -> Result<PathBuf> {
    if session_id.trim().is_empty() {
        return Err(Error::ValidationError("session_id must not be empty".to_string()));
    }
    Ok(chat_history_dir()?.join(format!("{session_id}{CHAT_FORK_METADATA_SUFFIX}")))
}

fn delete_chat_session_file(session_id: &str) -> Result<()> {
    let path = chat_session_file_path(session_id)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    let metadata_path = chat_fork_metadata_path(session_id)?;
    if metadata_path.exists() {
        fs::remove_file(metadata_path)?;
    }
    delete_session_title(session_id)?;
    Ok(())
}

fn render_bad_request(res: &mut Response, message: String) {
    res.status_code(StatusCode::BAD_REQUEST);
    res.render(Json(json!({ "error": message })));
}

fn render_internal_error(res: &mut Response, err: impl ToString) {
    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(Json(json!({ "error": err.to_string() })));
}
