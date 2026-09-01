use std::fs;
use std::path::PathBuf;

use chrono::{Duration, Utc};

use crate::storage::{AgentMemoryUpsert, LocalMemoryStore};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("zihuan-local-memory-test-{}", uuid::Uuid::new_v4()));
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn memory(key: &str, value: &str) -> AgentMemoryUpsert {
    AgentMemoryUpsert {
        key: key.to_string(),
        value: value.to_string(),
        expires_at: None,
        sender_id_list: Vec::new(),
        group_id_list: Vec::new(),
    }
}

/// Purpose: Verify local memory writes Markdown files and overwrites matching keys.
/// TestData: Two favorite-character values for the same `旅行者原神喜爱角色` key.
#[test]
fn test_local_memory_overwrites_matching_markdown_key() {
    let directory = TempDir::new();
    let store = LocalMemoryStore::new(directory.path.clone());

    store.create_or_update(&memory("旅行者原神喜爱角色", "刻晴")).unwrap();
    store.create_or_update(&memory("旅行者原神喜爱角色", "甘雨")).unwrap();

    assert_eq!(
        fs::read_to_string(directory.path.join("旅行者原神喜爱角色.md")).unwrap(),
        "甘雨"
    );
    assert_eq!(store.list(Some("甘雨"), 5).unwrap().len(), 1);
}

/// Purpose: Verify unsafe keys cannot write memory files outside the storage directory.
/// TestData: A path-traversal key, `../尘歌壶`.
#[test]
fn test_local_memory_rejects_path_traversal_key() {
    let directory = TempDir::new();
    let store = LocalMemoryStore::new(directory.path.clone());

    assert!(store.create_or_update(&memory("../尘歌壶", "璃月港")).is_err());
    assert!(!directory.path.join("../尘歌壶.md").exists());
}

/// Purpose: Verify local memory persists expiry metadata, filters expired records, and removes expired files.
/// TestData: One memory about纳西妲 expiring in one hour and one memory about雷电将军 expired one hour ago.
#[test]
fn test_local_memory_persists_expiry_metadata_and_filters_expired_records() {
    let directory = TempDir::new();
    let store = LocalMemoryStore::new(directory.path.clone());
    let expires_at = (Utc::now() + Duration::hours(1)).to_rfc3339();

    let mut future_memory = memory("纳西妲的元素爆发", "所闻遍计");
    future_memory.expires_at = Some(expires_at.clone());
    store.create_or_update(&future_memory).unwrap();

    let mut expired_memory = memory("雷电将军的元素爆发", "梦想一心");
    expired_memory.expires_at = Some((Utc::now() - Duration::hours(1)).to_rfc3339());
    store.create_or_update(&expired_memory).unwrap();

    let future_record = &store.list(None, 5).unwrap()[0].record;
    assert_eq!(future_record.key, "纳西妲的元素爆发");
    assert_eq!(future_record.expires_at.as_deref(), Some(expires_at.as_str()));
    assert_eq!(store.list(Some("梦想一心"), 5).unwrap().len(), 0);
    assert!(!directory.path.join("雷电将军的元素爆发.md").exists());
    assert!(!directory.path.join("雷电将军的元素爆发.meta.json").exists());
}

/// Purpose: Verify missing or corrupt metadata keeps local memory readable as a permanent memory.
/// TestData: One关于蒙德的 Markdown file without metadata and one关于璃月 with invalid JSON metadata.
#[test]
fn test_local_memory_treats_missing_or_corrupt_metadata_as_permanent() {
    let directory = TempDir::new();
    fs::create_dir_all(&directory.path).unwrap();
    fs::write(directory.path.join("蒙德风神.md"), "巴巴托斯的摸鱼日常").unwrap();
    fs::write(directory.path.join("璃月岩王帝君.md"), "摩拉克斯的退休生活").unwrap();
    fs::write(directory.path.join("璃月岩王帝君.meta.json"), "派蒙不是JSON").unwrap();

    let store = LocalMemoryStore::new(directory.path.clone());
    let records = store.list(None, 5).unwrap();

    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|hit| hit.record.expires_at.is_none()));
}

/// Purpose: Verify invalid expiry values are rejected before any local memory files are written.
/// TestData: A memory about枫原万叶 with `expires_at` set to the invalid value `稻妻明天`.
#[test]
fn test_local_memory_rejects_invalid_expiry_without_writing_files() {
    let directory = TempDir::new();
    let store = LocalMemoryStore::new(directory.path.clone());
    let mut invalid_memory = memory("枫原万叶的诗", "风带来的故事");
    invalid_memory.expires_at = Some("稻妻明天".to_string());

    assert!(store.create_or_update(&invalid_memory).is_err());
    assert!(!directory.path.exists());
}
