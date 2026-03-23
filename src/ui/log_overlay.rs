use std::collections::VecDeque;
use std::sync::Mutex;

use lazy_static::lazy_static;
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use log_util::log_util::LogUtil;

const MAX_ENTRIES: usize = 5;
const MAX_HISTORY: usize = 1000;

#[derive(Clone)]
pub struct LogEntry {
    pub level: Level,
    pub message: String,
}

lazy_static! {
    static ref LOG_RING_BUFFER: Mutex<VecDeque<LogEntry>> = Mutex::new(VecDeque::new());
    /// Full history (up to MAX_HISTORY entries), never drained automatically.
    static ref LOG_HISTORY: Mutex<VecDeque<LogEntry>> = Mutex::new(VecDeque::new());
}

pub struct CompositeLogger {
    base: &'static LogUtil,
}

impl Log for CompositeLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.base.enabled(metadata)
    }

    fn log(&self, record: &Record) {
        // Delegate to the underlying LogUtil (console + file output)
        self.base.log(record);

        if self.enabled(record.metadata()) {
            let entry = LogEntry {
                level: record.level(),
                message: record.args().to_string(),
            };
            if let Ok(mut buf) = LOG_RING_BUFFER.lock() {
                buf.push_back(entry.clone());
                while buf.len() > MAX_ENTRIES {
                    buf.pop_front();
                }
            }
            if let Ok(mut hist) = LOG_HISTORY.lock() {
                hist.push_back(entry);
                while hist.len() > MAX_HISTORY {
                    hist.pop_front();
                }
            }
        }
    }

    fn flush(&self) {
        self.base.flush();
    }
}

impl CompositeLogger {
    /// Initialize the global logger, replacing the default LogUtil logger.
    /// Mirrors LogUtil::init_with_logger but wraps it so logs also flow into the ring buffer.
    pub fn init(base: &'static LogUtil) -> Result<(), SetLoggerError> {
        let max_level = fetch_max_level_from_env();
        let logger = Box::new(CompositeLogger { base });
        log::set_boxed_logger(logger)?;
        log::set_max_level(max_level);
        Ok(())
    }
}

/// Drain all pending log entries from the ring buffer.
/// Called from the UI poll timer on the main thread.
pub fn drain_new_entries() -> Vec<LogEntry> {
    if let Ok(mut buf) = LOG_RING_BUFFER.lock() {
        buf.drain(..).collect()
    } else {
        Vec::new()
    }
}

/// Return a snapshot of the full log history (up to MAX_HISTORY entries).
pub fn get_history() -> Vec<LogEntry> {
    LOG_HISTORY
        .lock()
        .map(|h| h.iter().cloned().collect())
        .unwrap_or_default()
}

fn fetch_max_level_from_env() -> LevelFilter {
    match std::env::var("RUST_LOG").unwrap_or_default().as_str() {
        "info" => LevelFilter::Info,
        "debug" => LevelFilter::Debug,
        "error" => LevelFilter::Error,
        "warn" => LevelFilter::Warn,
        "off" => LevelFilter::Off,
        "trace" => LevelFilter::Trace,
        _ => LevelFilter::Info,
    }
}
