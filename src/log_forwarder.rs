/// Wraps the LogUtil logger and forwards every log record to the WebSocket
/// broadcast channel as a `ServerMessage::LogMessage`.
///
/// Usage in main():
///   1. `log_forwarder::init(&BASE_LOG);`      ← replaces LogUtil::init_with_logger
///   2. (after broadcast is created)
///      `log_forwarder::set_broadcast(broadcast.clone());`
use chrono::Local;
use log::{Log, Metadata, Record};
use log_util::log_util::LogUtil;
use once_cell::sync::OnceCell;
use std::cell::RefCell;
use std::sync::Arc;

use crate::api::state::{AppState, TaskLogEntry};
use crate::api::ws::{ServerMessage, WsBroadcast};

// ── Statics ───────────────────────────────────────────────────────────────────

static BROADCAST: OnceCell<WsBroadcast> = OnceCell::new();
static FORWARDER: OnceCell<LogForwarder> = OnceCell::new();
static APP_STATE: OnceCell<Arc<AppState>> = OnceCell::new();

thread_local! {
    static CURRENT_TASK_ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

// ── LogForwarder ──────────────────────────────────────────────────────────────

pub struct LogForwarder {
    inner: &'static LogUtil,
}

// SAFETY: LogUtil itself must be Send + Sync for log::set_logger to accept it.
// LogUtil is registered as the global logger by the log crate in the original code,
// which already requires it to be Send + Sync.
unsafe impl Send for LogForwarder {}
unsafe impl Sync for LogForwarder {}

impl Log for LogForwarder {
    #[inline]
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        // Always delegate to the original logger first (file + console output).
        self.inner.log(record);

        // Then forward to WebSocket clients if the channel is ready.
        if let Some(tx) = BROADCAST.get() {
            let level = record.level().to_string();
            let message = format!("{}", record.args());
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
            if let Some(task_id) = current_task_id() {
                if let Some(state) = APP_STATE.get() {
                    let _ = state.tasks.lock().unwrap().append_task_log(
                        &task_id,
                        &TaskLogEntry {
                            timestamp: timestamp.clone(),
                            level: level.clone(),
                            message: message.clone(),
                        },
                    );
                }
            }
            // Ignore errors: no receivers yet, channel full, etc.
            let _ = tx.send(ServerMessage::LogMessage {
                level,
                message,
                timestamp,
            });
        }
    }

    #[inline]
    fn flush(&self) {
        self.inner.flush();
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Replace the global logger with `LogForwarder` wrapping `inner`.
/// Must be called exactly once, before any `log::*` macro is invoked.
pub fn init(inner: &'static LogUtil) {
    FORWARDER.get_or_init(|| LogForwarder { inner });
    let forwarder: &'static LogForwarder = FORWARDER.get().unwrap();
    log::set_logger(forwarder).expect("Failed to set log_forwarder as global logger");

    // Mirror the level-filter logic used by LogUtil: honour $RUST_LOG, else Info.
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(log::LevelFilter::Info);
    log::set_max_level(level);
}

/// Provide the broadcast sender so that subsequent log records are forwarded.
/// Call this after `api::ws::create_broadcast()` returns a sender.
/// Safe to call multiple times; only the first call takes effect.
pub fn set_broadcast(tx: WsBroadcast) {
    let _ = BROADCAST.set(tx);
}

pub fn set_app_state(state: Arc<AppState>) {
    let _ = APP_STATE.set(state);
}

pub fn scope_task<T>(task_id: impl Into<String>, f: impl FnOnce() -> T) -> T {
    let task_id = task_id.into();
    CURRENT_TASK_ID.with(|cell| {
        let previous = cell.replace(Some(task_id));
        let result = f();
        cell.replace(previous);
        result
    })
}

fn current_task_id() -> Option<String> {
    CURRENT_TASK_ID
        .with(|cell| cell.borrow().clone())
        .or_else(|| {
            zihuan_graph_engine::data_value::EXECUTION_TASK_ID
                .try_with(Clone::clone)
                .ok()
        })
}
