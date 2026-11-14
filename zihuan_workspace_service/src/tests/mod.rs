mod api;
mod tools;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Returns a unique temporary directory for one test, creating it if needed.
///
/// Nanosecond timestamps alone can collide when parallel tests start within the same clock tick
/// (macOS clock granularity is microsecond-level), so a process-local counter is mixed in to
/// guarantee uniqueness; otherwise parallel tests share one directory and delete it under each
/// other's feet.
pub(crate) fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{counter}", std::process::id()));
    fs::create_dir_all(&path).expect("create temporary directory");
    path
}
