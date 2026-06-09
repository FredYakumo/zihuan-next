use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use ims_bot_adapter::{parse_ims_bot_adapter_connection, qq_avatar_url};
use model_inference::system_config::{AgentConfig, AgentType};
use salvo::http::body::BodySender;
use salvo::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use salvo::http::HeaderValue;
use salvo::http::ResBody;
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use storage_handler::{ConnectionConfig, ConnectionKind};
use tokio::sync::mpsc;
use uuid::Uuid;
use zihuan_agent::brain::BrainObserver;
use zihuan_core::agent_config::QqChatAgentConfig;
use zihuan_core::command::{CommandChannel, CommandContext, NewConversationRequest, SideEffectContext};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::ToolCalls;
use zihuan_core::llm::{LLMMessage, MessageRole, StreamToken};
use zihuan_core::message_part::MessagePart;

const CHAT_HISTORY_DIR_NAME: &str = "chat_history";
const APP_DIR_NAME: &str = "zihuan-next_aibot";

/// Bridges BrainObserver callbacks into the SSE event stream.
///
/// **Purpose:** The Brain tool-call loop emits structured events (tool start/finish) that the
/// dashboard needs to display in real time. This observer translates those callbacks into JSON
/// payloads and pushes them onto the same unbounded channel that the token stream uses, so the
/// relay loop can multiplex both onto a single SSE connection.
///
/// **Design:** Uses an unbounded sender intentionally — the relay loop drains both the token and
/// event channels via `tokio::select!`, so backpressure is managed by the SSE sender, not the
/// observer. Errors from `send` are silently ignored because a closed channel means the client
/// has disconnected and the entire streaming task will tear down.
///
/// **Architecture:** Created per-request inside `execute_chat_streaming`, passed as
/// `Arc<dyn BrainObserver>` into `infer_agent_response_streaming`.
struct SseBrainObserver {
    event_tx: mpsc::UnboundedSender<Value>,
    message_id: String,
}

impl BrainObserver for SseBrainObserver {
    fn on_tool_start(&self, name: &str, call_id: &str, arguments: &Value) {
        let event = json!({
            "type": "tool_call_start",
            "message_id": self.message_id,
            "call_id": call_id,
            "name": name,
            "arguments": arguments,
        });
        let _ = self.event_tx.send(event);
    }

    fn on_tool_finish(&self, name: &str, call_id: &str, result: &str) {
        let event = json!({
            "type": "tool_call_result",
            "message_id": self.message_id,
            "call_id": call_id,
            "name": name,
            "result": result,
        });
        let _ = self.event_tx.send(event);
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
    pub thinking_type: Option<model_inference::system_config::ThinkingType>,
    #[serde(default)]
    pub reasoning_effort: Option<model_inference::system_config::ReasoningEffort>,
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
    pub updated_at: String,
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    pub agent_type: Option<String>,
    pub agent_avatar_url: Option<String>,
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
    pub agent_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_avatar_url: Option<String>,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    pub timestamp: String,
    #[serde(default)]
    pub stream_index: Option<usize>,
    pub trace_id: String,
    pub message_id: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCalls>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
}

/// Lightweight display metadata extracted from an `AgentConfig`.
///
/// **Purpose:** Avoids passing the full `AgentConfig` (which contains LLM credentials and
/// connections) into the persistence and SSE layers that only need name/type/avatar.
#[derive(Debug, Clone)]
struct AgentSnapshot {
    name: String,
    agent_type: String,
    avatar_url: Option<String>,
}

/// Shared mutable state for command side-effects that run on the dashboard channel.
///
/// **Purpose:** Commands like "new conversation" need to issue a fresh session ID that the
/// streaming task must pick up. Rather than threading return values through the trait-based
/// `SideEffectContext`, we store the ID here and read it after `execute` returns.
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

/// `SideEffectContext` implementation for the dashboard chat channel.
///
/// **Purpose:** Adapts the generic `SideEffectContext` trait so that dashboard-originated commands
/// can trigger side-effects (e.g. starting a new conversation) while the streaming task is in
/// progress. The `state` is shared with the caller so the emitted session ID can be retrieved
/// after all side-effects have executed.
struct DashboardCommandSideEffectContext {
    command_context: CommandContext,
    state: DashboardCommandSideEffectState,
}

impl SideEffectContext for DashboardCommandSideEffectContext {
    fn command_context(&self) -> &CommandContext {
        &self.command_context
    }

    fn start_new_conversation(&self, _request: &NewConversationRequest) -> Result<()> {
        self.state.issue_new_session_id();
        Ok(())
    }
}

fn extract_agent_snapshot(agent: &AgentConfig, connections: &[ConnectionConfig]) -> AgentSnapshot {
    let agent_type = match &agent.agent_type {
        AgentType::QqChat(_) => "qq_chat",
        AgentType::HttpStream(_) => "http_stream",
    };

    let avatar_url = match &agent.agent_type {
        AgentType::QqChat(config) => resolve_qq_avatar_url(connections, config),
        AgentType::HttpStream(_) => None,
    };

    AgentSnapshot {
        name: agent.name.clone(),
        agent_type: agent_type.to_string(),
        avatar_url,
    }
}

fn resolve_qq_avatar_url(connections: &[ConnectionConfig], config: &QqChatAgentConfig) -> Option<String> {
    let connection = connections
        .iter()
        .find(|item| item.id == config.ims_bot_adapter_connection_id)?;
    let ConnectionKind::BotAdapter(raw) = &connection.kind else {
        return None;
    };
    let bot_connection = parse_ims_bot_adapter_connection(raw).ok()?;
    let qq_id = bot_connection.qq_id.as_ref()?.trim();
    if qq_id.is_empty() {
        return None;
    }
    qq_avatar_url(qq_id)
}

///
/// **Purpose:** Bundles the fully resolved `AgentConfig` and its display snapshot (name, type,
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
    agent: AgentConfig,
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
    agent_manager: &zihuan_service::agent::AgentManager,
    agent_id: &str,
) -> std::result::Result<ChatAgentInfo, Value> {
    let running_agent = agent_manager
        .running_agent(agent_id)
        .ok_or_else(|| json!({ "type": "error", "error": format!("agent '{}' is not running", agent_id) }))?;
    let agent = running_agent.agent_config().clone();

    let connections =
        crate::system_config::load_connections().map_err(|err| json!({ "type": "error", "error": err.to_string() }))?;
    let agent_snapshot = extract_agent_snapshot(&agent, &connections);

    Ok(ChatAgentInfo { agent, agent_snapshot })
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
/// the default outcome). When a command matches, it executes side-effects through
/// `DashboardCommandSideEffectContext`, then constructs the appropriate outcome based on whether
/// the command produced a passthrough text, an immediate reply, or triggered a new-conversation
/// side-effect. The three exit paths are documented on `CommandDispatchOutcome`.
///
/// **Architecture:** Called after `resolve_chat_agent` in `execute_chat_streaming`. Depends on
/// the global `CommandRegistry` from `zihuan_service`. Does **not** touch the SSE sender —
/// errors are returned as `Err(Value)` for the orchestrator to forward.
fn try_dispatch_dashboard_command(
    agent: &AgentConfig,
    agent_snapshot: &AgentSnapshot,
    requested_session_id: &Option<String>,
    messages: Vec<LLMMessage>,
    latest_user_message: &Option<LLMMessage>,
) -> std::result::Result<CommandDispatchOutcome, Value> {
    let requested_session_id = requested_session_id.as_deref().filter(|value| !value.trim().is_empty());
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
        agent_type: agent_snapshot.agent_type.clone(),
        agent_id: agent.id.clone(),
        caller_id: requested_session_id
            .map(|s| s.to_string())
            .unwrap_or_else(|| "dashboard".to_string()),
        channel: CommandChannel::DashboardChat {
            session_id: requested_session_id.map(|s| s.to_string()),
        },
    };

    let Some(dispatch_result) = command_registry.dispatch(&command_context, &raw_user_text) else {
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

    let side_effect_state = DashboardCommandSideEffectState::default();
    let side_effect_context = DashboardCommandSideEffectContext {
        command_context: command_context.clone(),
        state: side_effect_state.clone(),
    };
    for effect in &dispatch_result.result.side_effects {
        if let Err(err) = effect.execute(&side_effect_context) {
            return Err(json!({ "type": "error", "error": err.to_string() }));
        }
    }

    let issued_new_session_id = side_effect_state.current_new_session_id();
    if let Some(next_session_id) = issued_new_session_id.clone() {
        session_id = next_session_id;
    }

    if let Some(passthrough_text) = dispatch_result.passthrough_text {
        let passthrough_message = LLMMessage::user(passthrough_text.clone());
        latest_user_message = Some(passthrough_message.clone());

        if issued_new_session_id.is_some() {
            messages = vec![passthrough_message];
        } else if dispatch_result.result.inject_to_llm {
            messages.push(LLMMessage::assistant_text(dispatch_result.result.reply));
            messages.push(passthrough_message);
        } else {
            replace_last_user_message(&mut messages, passthrough_message);
        }
    } else if issued_new_session_id.is_some() {
        should_run_inference = false;
        should_persist = false;
        requires_assistant_message = false;
        latest_user_message = None;
    } else {
        should_run_inference = false;
        immediate_output_messages = Some(vec![LLMMessage::assistant_text(dispatch_result.result.reply)]);
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
    agent: &AgentConfig,
    agent_snapshot: &AgentSnapshot,
    trace_id: &str,
    latest_user_message: Option<&LLMMessage>,
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
) {
    loop {
        tokio::select! {
            biased;
            Some(brain_event) = event_rx.recv() => {
                if sender.send_data(format!("data: {brain_event}\n\n")).await.is_err() {
                    break;
                }
            }
            token_opt = token_rx.recv() => {
                match token_opt {
                    Some(token) => {
                        let event_type = match &token {
                            StreamToken::Thinking(_) => "thinking_delta",
                            StreamToken::Content(_) => "delta",
                        };
                        let delta_event = json!({
                            "type": event_type,
                            "message_id": assistant_message_id,
                            "token": token.as_str(),
                        });
                        if sender.send_data(format!("data: {delta_event}\n\n")).await.is_err() {
                            break;
                        }
                    }
                    None => {
                        while let Ok(brain_event) = event_rx.try_recv() {
                            if sender.send_data(format!("data: {brain_event}\n\n")).await.is_err() {
                                break;
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
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
    let body: ChatStreamRequest = match req.parse_json().await {
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
    if body.messages.is_empty() {
        render_bad_request(res, "messages must not be empty".to_string());
        return;
    }

    let state = depot.obtain::<std::sync::Arc<crate::api::state::AppState>>().unwrap().clone();

    let (sender, receiver) = ResBody::channel();
    res.headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream; charset=utf-8"));
    res.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    res.body = receiver;

    tokio::spawn(execute_chat_streaming(state, body, sender));
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
pub async fn get_chat_session_messages(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let session_id = req.param::<String>("session_id").unwrap_or_default();
    if session_id.trim().is_empty() {
        render_bad_request(res, "session_id must not be empty".to_string());
        return;
    }

    match load_chat_session_messages(&session_id) {
        Ok(messages) => res.render(Json(json!({ "messages": messages }))),
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

    match delete_chat_session_file(&session_id) {
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
/// 3. **Inference + relay** — if inference is required, spawns `infer_agent_response_streaming`
///    in a background task and either `relay_inference_stream` (token-by-token) or
///    `relay_collected_text` (batch) to forward results to the client.
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
    body: ChatStreamRequest,
    mut sender: BodySender,
) {
    let ChatStreamRequest {
        agent_id,
        session_id: requested_session_id,
        messages: raw_messages,
        stream,
        model_config_id,
        thinking_type,
        reasoning_effort,
    } = body;
    let mut messages: Vec<LLMMessage> = raw_messages.into_iter().map(Into::into).collect();

    let ChatAgentInfo { agent, agent_snapshot } = match resolve_chat_agent(&state.agent_manager, &agent_id) {
        Ok(info) => info,
        Err(event) => {
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
    };

    messages = sanitize_messages(messages);
    if messages.is_empty() {
        let event = json!({ "type": "error", "error": "messages must not be empty after sanitization" });
        let _ = sender.send_data(format!("data: {event}\n\n")).await;
        return;
    }
    let latest_user_message = messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::User))
        .cloned();

    let trace_id = Uuid::new_v4().to_string();

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

    let assistant_message_id = requires_assistant_message.then(|| format!("msg_{}", Uuid::new_v4().simple()));

    if !send_sse(
        &mut sender,
        &build_chat_stream_event("start", &session_id, assistant_message_id.as_deref()),
    )
    .await
    {
        return;
    }

    if !should_run_inference {
        if let (Some(ref output_messages), Some(ref msg_id)) = (&immediate_output_messages, &assistant_message_id) {
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
            )
            .await
            {
                return;
            }
        }

        let _ = send_sse(
            &mut sender,
            &build_chat_stream_event("done", &session_id, assistant_message_id.as_deref()),
        )
        .await;
        let _ = sender.send_data("data: [DONE]\n\n").await;
        return;
    }

    let assistant_message_id =
        assistant_message_id.expect("assistant_message_id must exist when inference is required");

    let (token_tx, mut token_rx) = mpsc::unbounded_channel::<StreamToken>();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<Value>();
    let observer: Arc<dyn BrainObserver> = Arc::new(SseBrainObserver {
        event_tx,
        message_id: assistant_message_id.clone(),
    });

    let inference_handle = tokio::spawn({
        let state = state.clone();
        let agent_id = agent_id.clone();
        let model_config_id = model_config_id.clone();
        async move {
            state
                .agent_manager
                .infer_agent_response_streaming_with_model(
                    &agent_id,
                    messages,
                    token_tx,
                    Some(observer),
                    model_config_id.as_deref(),
                    thinking_type,
                    reasoning_effort,
                )
                .await
        }
    });

    if stream.unwrap_or(true) {
        relay_inference_stream(&mut sender, &assistant_message_id, &mut token_rx, &mut event_rx).await;
    } else {
        relay_collected_text(&mut sender, &assistant_message_id, &mut token_rx, &mut event_rx).await;
    }

    let output_messages = match inference_handle.await {
        Ok(Ok(msgs)) => msgs,
        Ok(Err(err)) => {
            let event = json!({ "type": "error", "error": err.to_string() });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
        Err(err) => {
            let event = json!({ "type": "error", "error": format!("failed to join chat task: {err}") });
            let _ = sender.send_data(format!("data: {event}\n\n")).await;
            return;
        }
    };

    if let Err(err) = persist_chat_records(
        &session_id,
        &agent,
        &agent_snapshot,
        &trace_id,
        &assistant_message_id,
        latest_user_message.as_ref(),
        &output_messages,
    ) {
        let event = json!({ "type": "error", "error": err.to_string() });
        let _ = sender.send_data(format!("data: {event}\n\n")).await;
        return;
    }

    let _ = send_sse(
        &mut sender,
        &build_chat_stream_event("done", &session_id, Some(&assistant_message_id)),
    )
    .await;
    let _ = sender.send_data("data: [DONE]\n\n").await;
}

/// Build a top-level SSE event (`start` / `done`) with optional `message_id`.
fn build_chat_stream_event(kind: &str, session_id: &str, message_id: Option<&str>) -> Value {
    match message_id {
        Some(message_id) => json!({
            "type": kind,
            "session_id": session_id,
            "message_id": message_id,
        }),
        None => json!({
            "type": kind,
            "session_id": session_id,
        }),
    }
}

/// Strip messages whose text content is empty/whitespace-only and has no tool calls.
///
/// **Purpose:** Prevents degenerate inputs (e.g. trailing empty user messages) from reaching
/// the LLM, which would cause API errors.
fn sanitize_messages(messages: Vec<LLMMessage>) -> Vec<LLMMessage> {
    messages
        .into_iter()
        .filter(|message| {
            let has_content = message.content_text_owned().is_some_and(|text| !text.trim().is_empty());
            let has_reasoning = message.reasoning_content.as_deref().is_some_and(|text| !text.trim().is_empty());
            has_content || has_reasoning || !message.tool_calls.is_empty()
        })
        .collect()
}

/// Replace the last user-role message in the list, or append if none exists.
///
/// **Purpose:** Used by command dispatch when a passthrough command rewrites the user message
/// in-place rather than appending.
fn replace_last_user_message(messages: &mut Vec<LLMMessage>, replacement: LLMMessage) {
    if let Some(index) = messages.iter().rposition(|message| matches!(message.role, MessageRole::User)) {
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
    agent: &AgentConfig,
    agent_snapshot: &AgentSnapshot,
    trace_id: &str,
    assistant_message_id: &str,
    latest_user_message: Option<&LLMMessage>,
    output_messages: &[LLMMessage],
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    if let Some(user_message) = latest_user_message {
        let user_record = ChatHistoryRecord {
            session_id: session_id.to_string(),
            agent_id: agent.id.clone(),
            agent_name: agent_snapshot.name.clone(),
            agent_type: agent_snapshot.agent_type.clone(),
            agent_avatar_url: agent_snapshot.avatar_url.clone(),
            role: "user".to_string(),
            content: user_message.content_text_owned().unwrap_or_default(),
            reasoning_content: None,
            timestamp: now.clone(),
            stream_index: None,
            trace_id: trace_id.to_string(),
            message_id: format!("msg_{}", Uuid::new_v4().simple()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        };
        append_history_record(&user_record)?;
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
            agent_type: agent_snapshot.agent_type.clone(),
            agent_avatar_url: agent_snapshot.avatar_url.clone(),
            role: role.to_string(),
            content: message.content_text_owned().unwrap_or_default(),
            reasoning_content: message.reasoning_content.clone(),
            timestamp: now.clone(),
            stream_index: None,
            trace_id: trace_id.to_string(),
            message_id: if matches!(message.role, MessageRole::Assistant) && message.tool_calls.is_empty() {
                assistant_message_id.to_string()
            } else {
                format!("msg_{}", Uuid::new_v4().simple())
            },
            tool_calls: message.tool_calls.clone(),
            tool_call_id: message.tool_call_id.clone(),
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
        .map_err(|err| Error::StringError(format!("failed to serialize chat record: {err}")))?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
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

        if let Some(filter) = filter_agent_id {
            if first_record.as_ref().map(|r| r.agent_id.as_str()) != Some(filter) {
                continue;
            }
        }

        sessions.push(ChatSessionSummary {
            session_id: stem.to_string(),
            updated_at,
            agent_id: first_record.as_ref().map(|r| r.agent_id.clone()),
            agent_name: first_record.as_ref().map(|r| r.agent_name.clone()),
            agent_type: first_record.as_ref().map(|r| r.agent_type.clone()),
            agent_avatar_url: first_record.as_ref().and_then(|r| r.agent_avatar_url.clone()),
        });
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
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
            Err(err) => return Err(Error::StringError(format!("failed to parse chat record: {err}"))),
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

    let record: ChatHistoryRecord = serde_json::from_str(line.trim())
        .map_err(|err| Error::StringError(format!("failed to parse first chat history record: {err}")))?;
    Ok(Some(record))
}

fn chat_history_dir() -> Result<PathBuf> {
    let root = zihuan_core::system_config::app_data_dir()
        .join(APP_DIR_NAME)
        .join(CHAT_HISTORY_DIR_NAME);
    Ok(root)
}

fn chat_session_file_path(session_id: &str) -> Result<PathBuf> {
    if session_id.trim().is_empty() {
        return Err(Error::ValidationError("session_id must not be empty".to_string()));
    }
    Ok(chat_history_dir()?.join(format!("{session_id}.jsonl")))
}

fn delete_chat_session_file(session_id: &str) -> Result<()> {
    let path = chat_session_file_path(session_id)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
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
