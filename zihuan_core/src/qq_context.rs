//! Runtime view of one QQ Chat role agent's live conversation context.
//!
//! The agent's real context window lives in an [`LLMMessageSessionCacheRef`]. These helpers
//! let the API layer reach that cache by `agent_id` (so the WebUI can read the *actual*
//! context the model currently holds) and carry per-turn live events from the (blocking) QQ
//! turn thread out to the web socket broadcast.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::storage::LLMMessageSessionCacheRef;
use crate::ims_bot_adapter::models::message::Message;
use crate::model_inference::llm::TokenUsage;

/// Which backend an agent's conversation context cache is actually using.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QqChatContextBackend {
    Memory,
    /// Redis-backed — survives a restart while Redis holds the values.
    Redis,
}

/// A reference to an image the model was given, resolved enough for the WebUI to render a
/// preview and label it. `size_bytes` is filled best-effort from object storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRef {
    pub media_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rustfs_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

/// Collect the images the current user message carries, for the live context view.
///
/// This reads the original messages rather than the resolved multimodal parts: resolving an
/// image yields a data-URL part with a freshly generated media id, while the rendered user text
/// still carries the original `[Image: media_id=...]` placeholder. Taking the ids from the
/// messages keeps both sides pointing at the same image.
pub fn collect_turn_media_refs(messages: &[Message]) -> Vec<MediaRef> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::Image(image) => {
                let media = &image.media;
                Some(MediaRef {
                    media_id: media.media_id.clone(),
                    rustfs_path: Some(media.rustfs_path.clone())
                        .filter(|value| !value.trim().is_empty()),
                    original_source: Some(media.original_source.clone())
                        .filter(|value| !value.trim().is_empty()),
                    mime_type: media.mime_type.clone(),
                    name: media.name.clone(),
                    size_bytes: None,
                })
            }
            _ => None,
        })
        .collect()
}

/// Stage name under which a turn reports the assembled state/system prefix it sent to the
/// model. Each turn builds its own prefix, so the Data Explorer shows it per turn.
pub const QQ_CONTEXT_STAGE_SYSTEM_PROMPT: &str = "System Prompt";

/// Stage name under which a local embedding inference reports its live progress.
pub const QQ_CONTEXT_STAGE_EMBEDDING: &str = "embedding model inference";

/// The message a turn replies to, captured for the Data Explorer's quote block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatQuotedMessage {
    pub message_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,
    /// Rendered text of the quoted message.
    pub text: String,
}

/// One step of a sub-agent's own tool-calling loop. The step is recorded under the tool call
/// that started the sub-agent; the enclosing entry carries the call path, so nested sub-agents
/// are rendered one level deeper.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QqChatSubAgentEntry {
    /// The prompt the sub-agent was given.
    Input {
        #[serde(default)]
        system_prompt: String,
        user_prompt: String,
    },
    /// One assistant message of the sub-agent's loop. `final_response` marks the message that
    /// ended the loop.
    Assistant {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        iteration: Option<usize>,
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
        #[serde(default)]
        final_response: bool,
    },
    /// A live delta of the sub-agent's streaming response. Only broadcast to the WebUI; the
    /// completed [`Self::Assistant`] message is what gets archived.
    AssistantChunk { content: String, reasoning: String },
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

/// One turn-scoped context event, emitted while a QQ turn runs and pushed to the WebUI over
/// the existing global web socket broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QqChatContextEvent {
    /// The user's message arrived and the turn began.
    TurnStarted {
        agent_id: String,
        sender_id: String,
        sender_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group_name: Option<String>,
        is_group: bool,
        turn_id: String,
        created_at: String,
        user_text: String,
        /// The message the user quoted, shown inside the user bubble in the Data Explorer flow.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        quoted: Option<QqChatQuotedMessage>,
        #[serde(default)]
        media: Vec<MediaRef>,
        /// This turn is a steer follow-up of the previous turn, so the UI does not separate
        /// the two with a divider.
        #[serde(default)]
        continues_previous: bool,
    },
    /// A tool call started inside the brain loop.
    ToolStarted {
        agent_id: String,
        sender_id: String,
        turn_id: String,
        call_id: String,
        tool_name: String,
        arguments: Value,
    },
    /// A tool call finished with its result.
    ToolFinished {
        agent_id: String,
        sender_id: String,
        turn_id: String,
        call_id: String,
        tool_name: String,
        result: String,
        duration_ms: u128,
    },
    /// A named execution stage progressed (intent classification, reply review, …).
    Stage {
        agent_id: String,
        sender_id: String,
        turn_id: String,
        at: String,
        stage: String,
        /// Stable id of a stage that reports progress more than once; a later event with the
        /// same id replaces the earlier one instead of appending a second bubble.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        detail: Value,
    },
    /// One step of a sub-agent started by a tool call of this turn. `path` lists the enclosing
    /// tool call ids, outermost first; the last one is the sub-agent call that owns the step.
    SubAgent {
        agent_id: String,
        sender_id: String,
        turn_id: String,
        at: String,
        path: Vec<String>,
        event: QqChatSubAgentEntry,
    },
    /// The turn finished and produced its final assistant output plus token usage.
    TurnFinished {
        agent_id: String,
        sender_id: String,
        turn_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assistant_text: Option<String>,
        /// The model's thinking text for the final call, shown as a collapsed block.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        generation_ms: Option<u128>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_tokens_per_second: Option<f64>,
    },
}

static QQ_CHAT_HISTORY_CACHES: Lazy<RwLock<HashMap<String, Arc<LLMMessageSessionCacheRef>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static QQ_CONTEXT_EMITTER: Lazy<RwLock<Option<Arc<dyn Fn(QqChatContextEvent) + Send + Sync>>>> =
    Lazy::new(|| RwLock::new(None));

/// One step inside a turn, in the order it happened. The conversation cache only keeps the
/// user/assistant text, so tool calls and named stages are accumulated here instead.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QqChatTurnEntry {
    Stage {
        at: String,
        stage: String,
        /// Stable id of a stage updated in place (for example an embedding's start/finish).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        detail: Value,
    },
    ToolCall {
        at: String,
        call_id: String,
        name: String,
        arguments: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u128>,
    },
    /// One step of a sub-agent's own flow, nested under the tool call named by `path`.
    SubAgent {
        at: String,
        path: Vec<String>,
        event: QqChatSubAgentEntry,
    },
}

/// The recorded flow of one QQ turn, including everything the model was given and produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatTurnRecord {
    pub turn_id: String,
    pub agent_id: String,
    pub sender_id: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
    pub is_group: bool,
    pub user_text: String,
    /// The quoted message this turn replies to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quoted: Option<QqChatQuotedMessage>,
    #[serde(default)]
    pub media: Vec<MediaRef>,
    #[serde(default)]
    pub entries: Vec<QqChatTurnEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_per_second: Option<f64>,
    /// This turn continues the previous one with queued steer messages; the view keeps both in
    /// one visual block without a divider.
    #[serde(default)]
    pub continues_previous: bool,
    pub finished: bool,
}

/// Turns kept per sender. Old entries are dropped so the log cannot grow without bound.
const MAX_TURNS_PER_SENDER: usize = 50;

static QQ_CHAT_TURN_LOGS: Lazy<RwLock<HashMap<String, Vec<QqChatTurnRecord>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

fn turn_log_key(agent_id: &str, sender_id: &str) -> String {
    format!("{agent_id}:{sender_id}")
}

/// Fold one live event into the per-sender turn log.
///
/// The log is what the Data Explorer context view reads, so it carries the tool calls and
/// stages that the conversation cache never stores. Updates stay in memory; only a finished
/// turn is written to Redis, keeping the per-turn write count at one.
pub fn record_qq_chat_turn_event(event: &QqChatContextEvent) {
    let (agent_id, sender_id) = match event {
        QqChatContextEvent::TurnStarted { agent_id, sender_id, .. }
        | QqChatContextEvent::ToolStarted { agent_id, sender_id, .. }
        | QqChatContextEvent::ToolFinished { agent_id, sender_id, .. }
        | QqChatContextEvent::Stage { agent_id, sender_id, .. }
        | QqChatContextEvent::SubAgent { agent_id, sender_id, .. }
        | QqChatContextEvent::TurnFinished { agent_id, sender_id, .. } => {
            (agent_id.as_str(), sender_id.as_str())
        }
    };
    let key = turn_log_key(agent_id, sender_id);
    let finished = {
        let Ok(mut guard) = QQ_CHAT_TURN_LOGS.write() else {
            return;
        };
        let turns = guard.entry(key).or_default();
        match event {
            QqChatContextEvent::TurnStarted {
                turn_id,
                created_at,
                sender_name,
                group_name,
                is_group,
                user_text,
                quoted,
                media,
                continues_previous,
                ..
            } => {
                if turns.iter().any(|turn| turn.turn_id == *turn_id) {
                    false
                } else {
                    turns.push(QqChatTurnRecord {
                        turn_id: turn_id.clone(),
                        agent_id: agent_id.to_string(),
                        sender_id: sender_id.to_string(),
                        created_at: created_at.clone(),
                        sender_name: Some(sender_name.clone()),
                        group_name: group_name.clone(),
                        is_group: *is_group,
                        user_text: user_text.clone(),
                        quoted: quoted.clone(),
                        media: media.clone(),
                        entries: Vec::new(),
                        assistant_text: None,
                        reasoning_text: None,
                        usage: None,
                        generation_ms: None,
                        output_tokens_per_second: None,
                        continues_previous: *continues_previous,
                        finished: false,
                    });
                    if turns.len() > MAX_TURNS_PER_SENDER {
                        let overflow = turns.len() - MAX_TURNS_PER_SENDER;
                        turns.drain(0..overflow);
                    }
                    false
                }
            }
            QqChatContextEvent::ToolStarted {
                turn_id, call_id, tool_name, arguments, ..
            } => {
                if let Some(turn) = turns.iter_mut().find(|turn| turn.turn_id == *turn_id) {
                    turn.entries.push(QqChatTurnEntry::ToolCall {
                        at: now_rfc3339(),
                        call_id: call_id.clone(),
                        name: tool_name.clone(),
                        arguments: arguments.clone(),
                        result: None,
                        duration_ms: None,
                    });
                }
                false
            }
            QqChatContextEvent::ToolFinished {
                turn_id, call_id, result, duration_ms, ..
            } => {
                if let Some(turn) = turns.iter_mut().find(|turn| turn.turn_id == *turn_id) {
                    if let Some(QqChatTurnEntry::ToolCall { result: slot, duration_ms: slot_ms, .. }) =
                        turn.entries.iter_mut().rev().find(|entry| {
                            matches!(entry, QqChatTurnEntry::ToolCall { call_id: id, .. } if id == call_id)
                        })
                    {
                        *slot = Some(result.clone());
                        *slot_ms = Some(*duration_ms);
                    }
                }
                false
            }
            QqChatContextEvent::Stage { turn_id, at, stage, detail, id, .. } => {
                if let Some(turn) = turns.iter_mut().find(|turn| turn.turn_id == *turn_id) {
                    // A progress stage (embedding start → finish) keeps one entry: the later
                    // event replaces the earlier detail instead of appending a second bubble.
                    let updated = id.as_ref().is_some_and(|stage_id| {
                        turn.entries.iter_mut().rev().any(|entry| match entry {
                            QqChatTurnEntry::Stage { id: existing, detail: slot, .. }
                                if existing.as_deref() == Some(stage_id.as_str()) =>
                            {
                                *slot = detail.clone();
                                true
                            }
                            _ => false,
                        })
                    });
                    if !updated {
                        turn.entries.push(QqChatTurnEntry::Stage {
                            at: at.clone(),
                            stage: stage.clone(),
                            id: id.clone(),
                            detail: detail.clone(),
                        });
                    }
                }
                false
            }
            QqChatContextEvent::SubAgent { turn_id, at, path, event, .. } => {
                if let Some(turn) = turns.iter_mut().find(|turn| turn.turn_id == *turn_id) {
                    // Streaming deltas are a live signal only: the settled assistant message is
                    // recorded right after them, so archiving every token would just bloat the
                    // turn log.
                    if !matches!(event, QqChatSubAgentEntry::AssistantChunk { .. }) {
                        turn.entries.push(QqChatTurnEntry::SubAgent {
                            at: at.clone(),
                            path: path.clone(),
                            event: event.clone(),
                        });
                    }
                }
                false
            }
            QqChatContextEvent::TurnFinished {
                turn_id,
                assistant_text,
                reasoning_text,
                usage,
                generation_ms,
                output_tokens_per_second,
                ..
            } => {
                let mut finished = false;
                if let Some(turn) = turns.iter_mut().find(|turn| turn.turn_id == *turn_id) {
                    turn.assistant_text = assistant_text.clone();
                    turn.reasoning_text = reasoning_text.clone();
                    turn.usage = usage.clone();
                    turn.generation_ms = *generation_ms;
                    turn.output_tokens_per_second = *output_tokens_per_second;
                    turn.finished = true;
                    finished = true;
                }
                finished
            }
        }
    };

    let _ = finished;
}

fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339()
}

/// The turns recorded by the current process for one sender, oldest first.
///
/// This is deliberately process-scoped: durable history lives in the agent's relational
/// database now, so an old copy must not be resurrected here after a restart — it would show
/// up as "current runtime context" instead of archived history.
pub async fn qq_chat_turn_records(agent_id: &str, sender_id: &str) -> Vec<QqChatTurnRecord> {
    let key = turn_log_key(agent_id, sender_id);
    QQ_CHAT_TURN_LOGS
        .read()
        .ok()
        .and_then(|guard| guard.get(&key).cloned())
        .unwrap_or_default()
}

/// One recorded turn by id, for archiving it after it finishes.
pub async fn qq_chat_turn_record(
    agent_id: &str,
    sender_id: &str,
    turn_id: &str,
) -> Option<QqChatTurnRecord> {
    let key = turn_log_key(agent_id, sender_id);
    QQ_CHAT_TURN_LOGS
        .read()
        .ok()
        .and_then(|guard| guard.get(&key).cloned())
        .and_then(|turns| turns.into_iter().find(|turn| turn.turn_id == turn_id))
}

/// Forget the turns recorded by this process for one sender.
///
/// Called when the live conversation cache is cleared: finished turns already live in the
/// relational archive, so the context view must stop presenting them as current.
pub fn clear_qq_chat_turn_records(agent_id: &str, sender_id: &str) {
    if let Ok(mut guard) = QQ_CHAT_TURN_LOGS.write() {
        guard.remove(&turn_log_key(agent_id, sender_id));
    }
}

/// Turns written to Redis by a previous build, before durable archiving existed.
///
/// Reading the value removes it, so the migration runs once: those turns then live in the
/// relational archive and are never resurrected as "current runtime context".
pub async fn take_legacy_turn_records(agent_id: &str, sender_id: &str) -> Vec<QqChatTurnRecord> {
    let Some(cache) = qq_chat_history_cache(agent_id) else {
        return Vec::new();
    };
    let key = format!("qq_chat_turn_log:{agent_id}:{sender_id}");
    let Some(raw) = cache.redis_take_string(&key).await else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

/// Publish the conversation-context cache of one running QQ role agent so the API layer can
/// enumerate its senders and read their context by `agent_id`.
pub fn register_qq_chat_history_cache(agent_id: &str, cache: Arc<LLMMessageSessionCacheRef>) {
    if let Ok(mut guard) = QQ_CHAT_HISTORY_CACHES.write() {
        guard.insert(agent_id.to_string(), cache);
    }
}

/// Drop a previously registered conversation-context cache (agent stopped).
pub fn unregister_qq_chat_history_cache(agent_id: &str) {
    if let Ok(mut guard) = QQ_CHAT_HISTORY_CACHES.write() {
        guard.remove(agent_id);
    }
}

/// The conversation-context cache of a running QQ role agent, if it is registered.
pub fn qq_chat_history_cache(agent_id: &str) -> Option<Arc<LLMMessageSessionCacheRef>> {
    QQ_CHAT_HISTORY_CACHES
        .read()
        .ok()
        .and_then(|guard| guard.get(agent_id).cloned())
}

/// Install the process-wide sink that forwards context events to the web socket broadcast.
/// Set once at service startup; later calls replace the previous sink.
pub fn set_qq_context_emitter(emitter: Arc<dyn Fn(QqChatContextEvent) + Send + Sync>) {
    if let Ok(mut guard) = QQ_CONTEXT_EMITTER.write() {
        *guard = Some(emitter);
    }
}

/// Forward one context event to the installed sink. No-op when no sink is installed (e.g. a
/// standalone agent runtime), so callers never need to branch on availability.
pub fn emit_qq_context_event(event: QqChatContextEvent) {
    let emitter = QQ_CONTEXT_EMITTER.read().ok().and_then(|guard| guard.clone());
    if let Some(emitter) = emitter {
        emitter(event);
    }
}

/// The turn that embedding inference running on the current thread belongs to.
///
/// Embedding calls are synchronous and run on the blocking turn thread, so a thread-local scope
/// lets the core embedding layer report its progress without plumbing the turn identity through
/// every retrieval and tool helper.
#[derive(Debug, Clone)]
pub struct EmbeddingTraceContext {
    pub agent_id: String,
    pub sender_id: String,
    pub turn_id: String,
}

thread_local! {
    static EMBEDDING_TRACE_CONTEXT: RefCell<Option<EmbeddingTraceContext>> =
        const { RefCell::new(None) };
}

/// Binds embedding inference on this thread to the turn that is starting.
///
/// The guard restores the previous binding on drop, so nested or sequential turns never leak
/// their identity into the next one.
pub struct EmbeddingTraceGuard {
    previous: Option<EmbeddingTraceContext>,
}

impl EmbeddingTraceGuard {
    pub fn enter(agent_id: &str, sender_id: &str, turn_id: &str) -> Self {
        let context = EmbeddingTraceContext {
            agent_id: agent_id.to_string(),
            sender_id: sender_id.to_string(),
            turn_id: turn_id.to_string(),
        };
        let previous = EMBEDDING_TRACE_CONTEXT.with(|slot| slot.replace(Some(context)));
        Self { previous }
    }
}

impl Drop for EmbeddingTraceGuard {
    fn drop(&mut self) {
        EMBEDDING_TRACE_CONTEXT.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

/// Run `f` with the turn bound to the current thread, if any.
pub fn with_embedding_trace_context<R>(f: impl FnOnce(&EmbeddingTraceContext) -> R) -> Option<R> {
    let context = EMBEDDING_TRACE_CONTEXT.with(|slot| slot.borrow().clone());
    context.as_ref().map(f)
}

/// Emit one named stage for a running turn. Call sites pass the stage's real output so the
/// Data Explorer flow mirrors what actually ran instead of a fixed description.
pub fn emit_qq_context_stage(
    agent_id: &str,
    sender_id: &str,
    turn_id: &str,
    stage: &str,
    detail: Value,
) {
    emit_qq_context_event(QqChatContextEvent::Stage {
        agent_id: agent_id.to_string(),
        sender_id: sender_id.to_string(),
        turn_id: turn_id.to_string(),
        at: now_rfc3339(),
        stage: stage.to_string(),
        id: None,
        detail,
    });
}

/// Emit a stage that reports progress under a stable `stage_id`; a later event with the same id
/// replaces the earlier bubble in the context view instead of adding a second one.
pub fn emit_qq_context_stage_with_id(
    agent_id: &str,
    sender_id: &str,
    turn_id: &str,
    stage: &str,
    stage_id: &str,
    detail: Value,
) {
    emit_qq_context_event(QqChatContextEvent::Stage {
        agent_id: agent_id.to_string(),
        sender_id: sender_id.to_string(),
        turn_id: turn_id.to_string(),
        at: now_rfc3339(),
        stage: stage.to_string(),
        id: Some(stage_id.to_string()),
        detail,
    });
}

/// Emit one step of a sub-agent flow for a running turn, placed under the tool call chain
/// named by `path`.
pub fn emit_qq_context_sub_agent(
    agent_id: &str,
    sender_id: &str,
    turn_id: &str,
    path: &[String],
    event: QqChatSubAgentEntry,
) {
    emit_qq_context_event(QqChatContextEvent::SubAgent {
        agent_id: agent_id.to_string(),
        sender_id: sender_id.to_string(),
        turn_id: turn_id.to_string(),
        at: now_rfc3339(),
        path: path.to_vec(),
        event,
    });
}
