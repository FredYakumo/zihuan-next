use crate::error::Result;
use crate::storage::MemoryRecord;

/// Content identity of one memory record.
///
/// The recall phase shows the model each memory once: a record whose fingerprint the agent has
/// already received (or that appears in the previous conversation) is filtered out, so repeated
/// searches in one recall phase do not spend tokens on the same records.
pub(crate) fn memory_fingerprint(record: &MemoryRecord) -> Result<String> {
    use sha2::{Digest, Sha256};
    let content = serde_json::to_vec(record)?;
    Ok(format!(
        "memory:{}:{}:{}",
        record.category().schema().as_str(),
        record.object_id(),
        hex::encode(Sha256::digest(content))
    ))
}
