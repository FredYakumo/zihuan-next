//! Runtime view of one QQ Chat role agent's live conversation context.
//!
//! The agent's real context window lives in an [`LLMMessageSessionCacheRef`]. These helpers
//! let the API layer reach that cache by `agent_id` (so the WebUI can read the *actual*
//! context the model currently holds) and carry per-turn live events from the (blocking) QQ
//! turn thread out to the web socket broadcast.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ims_bot_adapter::models::message::Message;
use crate::model_inference::llm::TokenUsage;
use crate::storage::LLMMessageSessionCacheRef;

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

// ┌────────────────────────────────────────────────────────────────────────────────────────┐
// │ Stage name constants                                                                   │
// │                                                                                        │
// │ Every live-progress update of a turn is tagged with one of these names;                │
// │ the Data Explorer groups its trace entries by them.                                    │
// ├────────────────────────────────────────────────────────────────────────────────────────┤
/// Stage name under which a turn reports its system prompt.
pub const QQ_CONTEXT_STAGE_SYSTEM_PROMPT: &str = "System Prompt";

/// Stage name under which a local embedding inference reports its live progress.
pub const QQ_CONTEXT_STAGE_EMBEDDING: &str = "embedding model inference";

/// Stage name under which every LLM model inference of a turn reports its live progress.
pub const QQ_CONTEXT_STAGE_LLM: &str = "llm model inference";
// └────────────────────────────────────────────────────────────────────────────────────────┘

/// The message a turn replies to, captured for the Data Explorer's quote block.
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
        /// The tool schemas the sub-agent's loop hands to the LLM. Absent in entries recorded
        /// before the field existed.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tools: Vec<Value>,
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
        // u64, not u128: serde's internally-tagged enums buffer fields through a content
        // deserializer that rejects u128, which would make archived entries fail to load.
        duration_ms: u64,
    },
}

/// One turn-scoped context event, emitted while a QQ turn runs and pushed to the WebUI over
/// the existing global web socket broadcast.
///
/// A turn is one user message's full processing; its tool-calling loop runs several LLM calls
/// (`iteration`s) but stays one turn. Terminology: `documents/glossary.md`.
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
        /// The message the user quoted, shown inside the user entry in the Data Explorer flow.
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
        // u64, not u128: serde's internally-tagged enums buffer fields through a content
        // deserializer that rejects u128, which would make archived entries fail to load.
        duration_ms: u64,
    },
    /// A named execution stage progressed (intent classification, reply review, …).
    Stage {
        agent_id: String,
        sender_id: String,
        turn_id: String,
        at: String,
        stage: String,
        /// Stable id of a stage that reports progress more than once; a later event with the
        /// same id replaces the earlier one instead of appending a second entry.
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

impl QqChatContextEvent {
    /// Stable identity for updates of one step; distinct lifecycle events remain distinct.
    pub fn event_id(&self) -> String {
        let (agent, sender, turn) = crate::storage::context_store::event_identity(self);
        let key = match self {
            Self::TurnStarted { .. } => "started".to_string(),
            Self::TurnFinished { .. } => "finished".to_string(),
            Self::Stage { id: Some(id), .. } => format!("stage:{id}"),
            Self::ToolStarted { call_id, .. } => format!("tool-start:{call_id}"),
            Self::ToolFinished { call_id, .. } => format!("tool-finish:{call_id}"),
            _ => serde_json::to_string(self).unwrap_or_default(),
        };
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_OID,
            format!("{agent}:{sender}:{turn}:{key}").as_bytes(),
        )
        .to_string()
    }
}

static QQ_CHAT_HISTORY_CACHES: Lazy<RwLock<HashMap<String, Arc<LLMMessageSessionCacheRef>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Turn errors that make [`check_turn_storage`] fail the turn. Only direct, synchronous writes
/// record here: queued telemetry (see [`ContextWriter`]) logs instead, so background storage
/// trouble can never cut a reply short.
static QQ_CONTEXT_STORAGE_ERRORS: Lazy<RwLock<HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Budget for the storage probe below. The probe is a read on the turn's critical path, so it
/// gets a deadline instead of the unbounded bridge `block_async` would use.
const STORAGE_PROBE_TIMEOUT: Duration = Duration::from_secs(3);

pub fn check_turn_storage() -> crate::error::Result<()> {
    if let Some(trace) = current_turn_trace() {
        if let Some(error) = QQ_CONTEXT_STORAGE_ERRORS.read().unwrap().get(&trace.turn_id) {
            return Err(crate::string_error!("context persistence failed: {error}"));
        }
        if let Some(cache) = qq_chat_history_cache(&trace.agent_id) {
            if let Some((_, connection)) = cache.context_database.read().unwrap().clone() {
                let agent_id = trace.agent_id.clone();
                let sender_id = trace.sender_id.clone();
                let probe = crate::runtime::block_async_timeout(
                    async move {
                        crate::storage::context_store::running(&connection, &agent_id, &sender_id)
                            .await
                    },
                    STORAGE_PROBE_TIMEOUT,
                );
                match probe {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => return Err(error),
                    // A database that cannot answer in time must not abort the reply: the turn
                    // proceeds and the next probe retries.
                    Err(error) => {
                        log::warn!("[context_store] turn {}: {error}; continuing", trace.turn_id)
                    }
                }
            }
        }
    }
    Ok(())
}

static QQ_CONTEXT_EMITTER: Lazy<RwLock<Option<Arc<dyn Fn(QqChatContextEvent) + Send + Sync>>>> =
    Lazy::new(|| RwLock::new(None));

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
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

/// Capacity of the context-write queue. The queue exists so a stalled context database can never
/// block a reply; on overflow telemetry is dropped (and logged) instead of stalling the turn.
const WRITE_QUEUE_CAPACITY: usize = 1_024;

/// Budget for one queued write. The store is already deadline-bounded per query; this only bounds
/// a writer stuck past every query deadline. It must stay far above a healthy write: a cold pool
/// opening its first connection can take seconds, and the queue must never turn that into a
/// failed turn.
const WRITE_TIMEOUT: Duration = Duration::from_secs(15);

/// One unit of work for the context writer thread.
enum ContextWrite {
    Event {
        connection: crate::data_refs::RelationalDbConnection,
        event: QqChatContextEvent,
    },
    /// `Drop`-path finalization: mark the turn and its steps failed unless `TurnFinished` already
    /// committed. Queued after the turn's own events, so the order of writes is preserved.
    FailUnfinished {
        connection: crate::data_refs::RelationalDbConnection,
        agent: String,
        sender: String,
        turn: String,
    },
}

impl ContextWrite {
    fn turn(&self) -> &str {
        match self {
            Self::Event { event, .. } => crate::storage::context_store::event_identity(event).2,
            Self::FailUnfinished { turn, .. } => turn,
        }
    }
}

async fn write_event(write: ContextWrite) -> crate::error::Result<()> {
    match write {
        ContextWrite::Event { connection, event } => {
            crate::storage::context_store::record_event(&connection, &event).await
        }
        ContextWrite::FailUnfinished { connection, agent, sender, turn } => {
            crate::storage::context_store::fail_unfinished(&connection, &agent, &sender, &turn)
                .await
        }
    }
}

/// Records context events on a dedicated thread.
///
/// Events used to be written synchronously from the turn thread through `runtime::block_async`,
/// which hands the calling thread to `Handle::block_on`/`block_in_place`. One stalled
/// `record_event` therefore parked the whole turn thread — the reply hung forever and its session
/// claim leaked. Now the turn only pays for a channel send; a slow or broken database costs
/// telemetry, never the conversation.
struct ContextWriter {
    queue: SyncSender<ContextWrite>,
    dropped: AtomicU64,
}

/// Writes that failed or timed out. Kept outside [`ContextWriter`] because the writer thread starts
/// before the struct exists; only the log line reads it.
static FAILED_CONTEXT_WRITES: AtomicU64 = AtomicU64::new(0);

static CONTEXT_WRITER: Lazy<ContextWriter> = Lazy::new(ContextWriter::start);

impl ContextWriter {
    fn start() -> Self {
        let (queue, receiver) = sync_channel::<ContextWrite>(WRITE_QUEUE_CAPACITY);
        std::thread::Builder::new()
            .name("qq-context-writer".into())
            .spawn(move || Self::run(receiver))
            .expect("spawn qq context writer");
        Self { queue, dropped: AtomicU64::new(0) }
    }

    fn run(receiver: Receiver<ContextWrite>) {
        while let Ok(write) = receiver.recv() {
            let turn = write.turn().to_string();
            // A panic here would kill the only writer and silently drop all later telemetry.
            let written = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::runtime::block_async_timeout(write_event(write), WRITE_TIMEOUT)
            }));
            let error = match written {
                Ok(Ok(Ok(()))) => continue,
                Ok(Ok(Err(error))) => error.to_string(),
                Ok(Err(error)) => error.to_string(),
                Err(_) => "context write panicked".to_string(),
            };
            // Telemetry only: the turn has already moved on, so a failed write is logged and
            // counted — never recorded as a turn error, which would abort a healthy reply.
            let failed = FAILED_CONTEXT_WRITES.fetch_add(1, Ordering::Relaxed) + 1;
            log::error!("[context_store] turn {turn}: {error} ({failed} failed write(s))");
        }
    }

    fn submit(&self, write: ContextWrite) {
        match self.queue.try_send(write) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => self.drop_one("write queue full"),
            Err(TrySendError::Disconnected(_)) => self.drop_one("writer stopped"),
        }
    }

    /// Log every dropped event while the problem is rare, then only powers of two.
    fn drop_one(&self, reason: &str) {
        let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
        if dropped.is_power_of_two() {
            log::warn!("[context_store] dropped {dropped} context event(s): {reason}");
        }
    }
}

/// Forward one context event to the installed sink and queue its persistence. No-op for
/// persistence when no sink is installed (e.g. a standalone agent runtime), so callers never need
/// to branch on availability.
pub fn emit_qq_context_event(event: QqChatContextEvent) {
    let agent = crate::storage::context_store::event_identity(&event).0;
    if let Some(cache) = qq_chat_history_cache(agent) {
        let database = cache.context_database.read().unwrap().clone();
        if let Some((_, connection)) = database {
            CONTEXT_WRITER.submit(ContextWrite::Event { connection, event: event.clone() });
        }
    }
    let emitter = QQ_CONTEXT_EMITTER.read().ok().and_then(|guard| guard.clone());
    if let Some(emitter) = emitter {
        emitter(event);
    }
}

/// Mark a turn's unfinished steps failed without blocking, for `Drop` paths that cannot await.
/// No-op when the role agent has no context database registered.
pub fn finalize_turn_async(agent: &str, sender: &str, turn: &str) {
    if let Some(cache) = qq_chat_history_cache(agent) {
        let database = cache.context_database.read().unwrap().clone();
        if let Some((_, connection)) = database {
            CONTEXT_WRITER.submit(ContextWrite::FailUnfinished {
                connection,
                agent: agent.to_string(),
                sender: sender.to_string(),
                turn: turn.to_string(),
            });
        }
    }
}

/// The turn that model inference running on the current thread belongs to.
///
/// Embedding calls are synchronous and run on the blocking turn thread, and LLM calls report
/// from the usage-tracking wrapper around every model, so a thread-local scope lets those
/// layers report their progress without plumbing the turn identity through every retrieval,
/// tool and inference helper. Thread boundaries that run turn work elsewhere (tool execution,
/// nested sub-agent requests) re-enter the guard with the captured context.
#[derive(Debug, Clone)]
pub struct TurnTraceContext {
    pub agent_id: String,
    pub sender_id: String,
    pub turn_id: String,
    pub path: Vec<String>,
    /// Procedure the work on this thread belongs to, when it is more specific than the turn.
    pub procedure: Option<String>,
}

impl TurnTraceContext {
    /// Attributes the stages emitted under this context to one procedure.
    pub fn with_procedure(mut self, procedure: impl Into<String>) -> Self {
        self.procedure = Some(procedure.into());
        self
    }
}

thread_local! {
    static TURN_TRACE_CONTEXT: RefCell<Option<TurnTraceContext>> =
        const { RefCell::new(None) };
}

/// Binds model inference on this thread to the turn that is starting.
///
/// The guard restores the previous binding on drop, so nested or sequential turns never leak
/// their identity into the next one.
pub struct TurnTraceGuard {
    previous: Option<TurnTraceContext>,
    owns_turn: bool,
}

impl TurnTraceGuard {
    pub fn enter(agent_id: &str, sender_id: &str, turn_id: &str) -> Self {
        let mut guard = Self::from_context(TurnTraceContext {
            agent_id: agent_id.to_string(),
            sender_id: sender_id.to_string(),
            turn_id: turn_id.to_string(),
            path: Vec::new(),
            procedure: None,
        });
        guard.owns_turn = true;
        guard
    }

    /// Bind an already captured context, for threads that received the turn identity by value.
    pub fn from_context(context: TurnTraceContext) -> Self {
        let previous = TURN_TRACE_CONTEXT.with(|slot| slot.replace(Some(context)));
        Self { previous, owns_turn: false }
    }
}

impl Drop for TurnTraceGuard {
    fn drop(&mut self) {
        if let Some(trace) = current_turn_trace().filter(|_| self.owns_turn) {
            if self.previous.as_ref().is_none_or(|previous| previous.turn_id != trace.turn_id) {
                QQ_CONTEXT_STORAGE_ERRORS.write().unwrap().remove(&trace.turn_id);
            }
        }
        TURN_TRACE_CONTEXT.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

/// The turn context bound to the current thread, if any.
pub fn current_turn_trace() -> Option<TurnTraceContext> {
    TURN_TRACE_CONTEXT.with(|slot| slot.borrow().clone())
}

/// Run `f` with the turn bound to the current thread, if any.
pub fn with_turn_trace_context<R>(f: impl FnOnce(&TurnTraceContext) -> R) -> Option<R> {
    let context = TURN_TRACE_CONTEXT.with(|slot| slot.borrow().clone());
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
        id: Some(uuid::Uuid::new_v4().to_string()),
        detail,
    });
}

/// Emit a stage that reports progress under a stable `stage_id`; a later event with the same id
/// replaces the earlier entry in the context view instead of adding a second one.
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
