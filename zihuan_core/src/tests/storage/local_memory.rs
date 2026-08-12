use std::fs;
use std::path::PathBuf;

use crate::storage::{AgentMemoryUpsert, LocalMemoryStore};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("zihuan-local-memory-test-{}", uuid::Uuid::new_v4()));
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
/// TestData: Two values for the same `user preference` key.
#[test]
fn test_local_memory_overwrites_matching_markdown_key() {
    let directory = TempDir::new();
    let store = LocalMemoryStore::new(directory.path.clone());

    store.create_or_update(&memory("user preference", "tea")).unwrap();
    store.create_or_update(&memory("user preference", "coffee")).unwrap();

    assert_eq!(fs::read_to_string(directory.path.join("user preference.md")).unwrap(), "coffee");
    assert_eq!(store.list(Some("coffee"), 5).unwrap().len(), 1);
}

/// Purpose: Verify unsafe keys cannot write memory files outside the storage directory.
/// TestData: A path-traversal key, `../outside`.
#[test]
fn test_local_memory_rejects_path_traversal_key() {
    let directory = TempDir::new();
    let store = LocalMemoryStore::new(directory.path.clone());

    assert!(store.create_or_update(&memory("../outside", "value")).is_err());
    assert!(!directory.path.join("..\\outside.md").exists());
}
