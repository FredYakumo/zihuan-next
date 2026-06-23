use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::ims_bot_adapter::models::message::PersistedMediaSource;

pub fn build_persisted_media_id(source: &PersistedMediaSource, original_source: &str, rustfs_path: &str) -> String {
    let seed = format!("{source}|{original_source}|{rustfs_path}");
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    format!("media-{:016x}", hasher.finish())
}
