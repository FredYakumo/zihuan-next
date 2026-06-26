use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use chrono::Local;

use crate::ims_bot_adapter::models::message::PersistedMediaSource;

pub fn build_persisted_media_id(source: &PersistedMediaSource, original_source: &str, rustfs_path: &str) -> String {
    let seed = format!("{source}|{original_source}|{rustfs_path}");
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let now = Local::now().format("%Y-%m-%d-%H-%M-%S");
    format!("{}-{:016x}", now, hasher.finish())
}
