#[path = "../src/api/workspace_changes.rs"]
mod workspace_changes;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use workspace_changes::{accept, cancel, WorkspaceChangeOperation, WorkspaceChangeRecorder};

fn temp_workspace() -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("zihuan-workspace-change-{suffix}"))
}

/// Purpose: Verify consecutive edits to one file share one merged change record and can be rolled back atomically.
/// TestData: A two-line UTF-8 file, followed by two successful edits changing one line each.
#[test]
fn edit_changes_merge_and_cancel_restores_original_content() {
    let root = temp_workspace();
    let session_id = format!("test-session-{}", root.file_name().unwrap().to_string_lossy());
    fs::create_dir_all(&root).unwrap();
    let path = root.join("note.txt");
    fs::write(&path, "one\ntwo\n").unwrap();
    let recorder = WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));
    let args = serde_json::json!({ "path": "note.txt" });

    recorder.start("first", WorkspaceChangeOperation::Edit, &args);
    fs::write(&path, "ONE\ntwo\n").unwrap();
    let first = recorder.finish("first", r#"{"ok":true}"#).unwrap();
    recorder.start("second", WorkspaceChangeOperation::Edit, &args);
    fs::write(&path, "ONE\nTWO\n").unwrap();
    let second = recorder.finish("second", r#"{"ok":true}"#).unwrap();

    assert_eq!(first.change_id, second.change_id);
    assert_eq!(second.merged_count, 2);
    cancel(&session_id, &second.change_id).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "one\ntwo\n");
    let _ = fs::remove_dir_all(root);
}

/// Purpose: Verify Accept marks a change as handled without writing or altering the changed file.
/// TestData: A newly created file containing the text `kept` and one pending create operation.
#[test]
fn accept_does_not_change_disk() {
    let root = temp_workspace();
    let session_id = format!("accept-session-{}", root.file_name().unwrap().to_string_lossy());
    fs::create_dir_all(&root).unwrap();
    let path = root.join("created.txt");
    let recorder = WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));
    recorder.start("create", WorkspaceChangeOperation::Create, &serde_json::json!({ "path": "created.txt" }));
    fs::write(&path, "kept").unwrap();
    let change = recorder.finish("create", r#"{"ok":true}"#).unwrap();
    accept(&session_id, &change.change_id).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "kept");
    let _ = fs::remove_dir_all(root);
}