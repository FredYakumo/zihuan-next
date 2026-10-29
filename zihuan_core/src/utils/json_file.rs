use crate::error::Result;
use serde_json::Value;
use std::path::Path;

/// Publish complete JSON documents so another process never reads a partial write.
pub fn write_json_atomic(path: &Path, value: &Value) -> Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}
