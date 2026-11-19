use std::fs;
use std::path::PathBuf;

use chrono::{Duration, Utc};

use crate::storage::memory::MemoryRecord;
use crate::storage::memory::{prepare_behavioral, prepare_concept};
use crate::storage::{BehavioralMemoryWrite, ConceptMemoryWrite, LocalMemoryStore};

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

fn concept(key: &str, content: &str) -> ConceptMemoryWrite {
    ConceptMemoryWrite {
        key: key.to_string(),
        content: content.to_string(),
        access_level: 2,
        starts_at: None,
        sender_id_list: Vec::new(),
        group_id_list: Vec::new(),
    }
}

fn behavioral(key: &str, content: &str, expires_at: Option<String>) -> BehavioralMemoryWrite {
    BehavioralMemoryWrite {
        key: key.to_string(),
        content: content.to_string(),
        access_level: 2,
        expires_at,
        sender_id_list: Vec::new(),
        group_id_list: Vec::new(),
    }
}

/// Purpose: Verify local concept memory writes one file per title and overwrites it on correction.
/// TestData: Two favorite-character contents for the same `旅行者原神喜爱角色` key.
#[test]
fn test_local_memory_overwrites_matching_markdown_key() {
    let directory = TempDir::new();
    let store = LocalMemoryStore::new(directory.path.clone());

    store
        .save_concept(&prepare_concept(&concept("旅行者原神喜爱角色", "刻晴")).unwrap())
        .unwrap();
    store
        .save_concept(&prepare_concept(&concept("旅行者原神喜爱角色", "甘雨")).unwrap())
        .unwrap();

    let category_dir = directory.path.join("concept_memory");
    let mut markdown = Vec::new();
    for entry in fs::read_dir(&category_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            markdown.push(path);
        }
    }
    assert_eq!(markdown.len(), 1);
    assert_eq!(fs::read_to_string(&markdown[0]).unwrap(), "甘雨");
    assert_eq!(store.list(Some("甘雨"), 5).unwrap().len(), 1);
}

/// Purpose: Verify behavioral local memory persists expiry metadata and filters expired records.
/// TestData: One behavioral memory about纳西妲 expiring in one hour and one about雷电将军 expired one hour ago.
#[test]
fn test_local_memory_persists_expiry_metadata_and_filters_expired_records() {
    let directory = TempDir::new();
    let store = LocalMemoryStore::new(directory.path.clone());
    let expires_at = (Utc::now() + Duration::hours(1)).to_rfc3339();

    store
        .save_behavioral(
            &prepare_behavioral(&behavioral(
                "纳西妲的元素爆发",
                "所闻遍计",
                Some(expires_at.clone()),
            ))
            .unwrap(),
        )
        .unwrap();
    store
        .save_behavioral(
            &prepare_behavioral(&behavioral(
                "雷电将军的元素爆发",
                "梦想一心",
                Some((Utc::now() - Duration::hours(1)).to_rfc3339()),
            ))
            .unwrap(),
        )
        .unwrap();

    let hits = store.list(None, 5).unwrap();
    assert_eq!(hits.len(), 1);
    let MemoryRecord::Behavioral(record) = &hits[0].record else {
        panic!("expected a behavioral record");
    };
    assert_eq!(record.key, "纳西妲的元素爆发");
    assert_eq!(record.expires_at, expires_at);
    assert_eq!(store.list(Some("梦想一心"), 5).unwrap().len(), 0);
}

/// Purpose: Verify records with missing or corrupt metadata are skipped without breaking the listing.
/// TestData: One concept .md file without metadata and one with invalid JSON metadata.
#[test]
fn test_local_memory_skips_records_with_missing_or_corrupt_metadata() {
    let directory = TempDir::new();
    fs::create_dir_all(directory.path.join("concept_memory")).unwrap();
    fs::write(directory.path.join("concept_memory/蒙德风神.md"), "巴巴托斯的摸鱼日常").unwrap();
    fs::write(directory.path.join("concept_memory/璃月岩王帝君.md"), "摩拉克斯的退休生活").unwrap();
    fs::write(directory.path.join("concept_memory/璃月岩王帝君.meta.json"), "派蒙不是JSON")
        .unwrap();

    let store = LocalMemoryStore::new(directory.path.clone());
    let records = store.list(None, 5).unwrap();

    assert_eq!(records.len(), 0);
}

/// Purpose: Verify invalid behavioral expiry values are rejected before any local memory files are written.
/// TestData: A behavioral memory about枫原万叶 with `expires_at` set to the invalid value `稻妻明天`.
#[test]
fn test_local_memory_rejects_invalid_expiry_without_writing_files() {
    let directory = TempDir::new();

    assert!(prepare_behavioral(&behavioral(
        "枫原万叶的诗",
        "风带来的故事",
        Some("稻妻明天".to_string())
    ))
    .is_err());
    assert!(!directory.path.join("behavioral_memory").exists());
}
