use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::workspace_changes::{
    accept, cancel, pending, WorkspaceChangeOperation, WorkspaceChangeRecorder,
};

fn temp_workspace() -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("zihuan-workspace-change-{suffix}"))
}

/// Purpose: Verify consecutive edits share one logical change and Reject restores the original file.
/// Test Data: `teyvat.txt` starts with Mondstadt and Liyue, then changes to Inazuma and Sumeru.
#[test]
fn edit_changes_merge_and_cancel_restores_original_content() {
    let root = temp_workspace();
    let session_id = format!("test-session-{}", root.file_name().unwrap().to_string_lossy());
    fs::create_dir_all(&root).unwrap();
    let path = root.join("teyvat.txt");
    fs::write(&path, "Mondstadt\nLiyue\n").unwrap();
    let recorder =
        WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));
    let args = serde_json::json!({ "path": "teyvat.txt" });

    recorder.start("first", WorkspaceChangeOperation::Edit, &args);
    fs::write(&path, "Inazuma\nLiyue\n").unwrap();
    let first = recorder.finish("first", r#"{"ok":true}"#).unwrap();
    recorder.start("second", WorkspaceChangeOperation::Edit, &args);
    fs::write(&path, "Inazuma\nSumeru\n").unwrap();
    let second = recorder.finish("second", r#"{"ok":true}"#).unwrap();

    assert_eq!(first.change_id, second.change_id);
    assert_eq!(second.merged_count, 2);
    cancel(&session_id, &second.change_id).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "Mondstadt\nLiyue\n");
    let _ = fs::remove_dir_all(root);
}

/// Purpose: Verify Accept marks a change as handled without changing its final filesystem content.
/// Test Data: A new `nahida-note.txt` file containing a short Sumeru research note.
#[test]
fn accept_does_not_change_disk() {
    let root = temp_workspace();
    let session_id = format!("accept-session-{}", root.file_name().unwrap().to_string_lossy());
    fs::create_dir_all(&root).unwrap();
    let path = root.join("nahida-note.txt");
    let recorder =
        WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));
    recorder.start(
        "create",
        WorkspaceChangeOperation::Create,
        &serde_json::json!({ "path": "nahida-note.txt" }),
    );
    fs::write(&path, "Nahida studies Irminsul.\n").unwrap();
    let change = recorder.finish("create", r#"{"ok":true}"#).unwrap();

    accept(&session_id, &change.change_id).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "Nahida studies Irminsul.\n");
    let _ = fs::remove_dir_all(root);
}

/// Purpose: Verify create-delete-create collapses into one final create record with the original rollback baseline.
/// Test Data: `paimon-guide.md` is first created with Mondstadt notes, deleted, then recreated with Liyue notes.
#[test]
fn create_delete_create_merges_into_one_pending_change() {
    let root = temp_workspace();
    let session_id =
        format!("create-delete-create-{}", root.file_name().unwrap().to_string_lossy());
    fs::create_dir_all(&root).unwrap();
    let path = root.join("paimon-guide.md");
    let recorder =
        WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));
    let args = serde_json::json!({ "path": "paimon-guide.md" });

    recorder.start("create-first", WorkspaceChangeOperation::Create, &args);
    fs::write(&path, "Mondstadt has dandelion wine.\n").unwrap();
    let first = recorder.finish("create-first", r#"{"ok":true}"#).unwrap();
    recorder.start("delete", WorkspaceChangeOperation::Delete, &args);
    fs::remove_file(&path).unwrap();
    let resolved = recorder.finish("delete", r#"{"ok":true}"#).unwrap();
    recorder.start("create-second", WorkspaceChangeOperation::Create, &args);
    fs::write(&path, "Liyue has lantern rite.\n").unwrap();
    let final_change = recorder.finish("create-second", r#"{"ok":true}"#).unwrap();

    assert!(matches!(
        resolved.status,
        crate::api::workspace_changes::WorkspaceChangeStatus::Resolved
    ));
    assert_eq!(first.change_id, final_change.change_id);
    assert_eq!(final_change.merged_count, 3);
    assert!(matches!(final_change.operation, WorkspaceChangeOperation::Create));
    assert_eq!(pending(&session_id).unwrap().len(), 1);
    cancel(&session_id, &final_change.change_id).unwrap();
    assert!(!path.exists());
    let _ = fs::remove_dir_all(root);
}

/// Purpose: Verify a change that returns to its original content no longer appears as pending.
/// Test Data: `b.txt` changes from a Mondstadt greeting to a Liyue greeting and then back again.
#[test]
fn returning_to_the_original_content_resolves_the_change() {
    let root = temp_workspace();
    let session_id = format!("resolved-change-{}", root.file_name().unwrap().to_string_lossy());
    fs::create_dir_all(&root).unwrap();
    let path = root.join("b.txt");
    fs::write(&path, "Welcome to Mondstadt.\n").unwrap();
    let recorder =
        WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));
    let args = serde_json::json!({ "path": "b.txt" });

    recorder.start("liyue", WorkspaceChangeOperation::Edit, &args);
    fs::write(&path, "Welcome to Liyue Harbor.\n").unwrap();
    recorder.finish("liyue", r#"{"ok":true}"#).unwrap();
    recorder.start("mondstadt", WorkspaceChangeOperation::Edit, &args);
    fs::write(&path, "Welcome to Mondstadt.\n").unwrap();
    let resolved = recorder.finish("mondstadt", r#"{"ok":true}"#).unwrap();

    assert!(matches!(
        resolved.status,
        crate::api::workspace_changes::WorkspaceChangeStatus::Resolved
    ));
    assert!(pending(&session_id).unwrap().is_empty());
    let _ = fs::remove_dir_all(root);
}

/// Purpose: Verify a pending file change remains merged when a later chat round creates a new recorder for the same session.
/// Test Data: `b.txt` changes from Mondstadt to Inazuma in round one, then to Fontaine in round two.
#[test]
fn later_chat_round_merges_with_the_existing_pending_change() {
    let root = temp_workspace();
    let session_id = format!("multi-round-{}", root.file_name().unwrap().to_string_lossy());
    fs::create_dir_all(&root).unwrap();
    let path = root.join("b.txt");
    fs::write(&path, "Mondstadt\n").unwrap();
    let args = serde_json::json!({ "path": "b.txt" });

    let first_round =
        WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));
    first_round.start("round-one", WorkspaceChangeOperation::Edit, &args);
    fs::write(&path, "Inazuma\n").unwrap();
    let first_change = first_round.finish("round-one", r#"{"ok":true}"#).unwrap();

    let second_round =
        WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));
    second_round.start("round-two", WorkspaceChangeOperation::Edit, &args);
    fs::write(&path, "Fontaine\n").unwrap();
    let second_change = second_round.finish("round-two", r#"{"ok":true}"#).unwrap();

    assert_eq!(first_change.change_id, second_change.change_id);
    assert_eq!(second_change.merged_count, 2);
    assert_eq!(pending(&session_id).unwrap().len(), 1);
    cancel(&session_id, &second_change.change_id).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "Mondstadt\n");
    let _ = fs::remove_dir_all(root);
}

/// Purpose: Verify an edit followed by a move is one move record and Reject restores the original source path.
/// Test Data: `a.txt` changes from Amber's message to Yoimiya's message before moving to `b.txt`.
#[test]
fn edit_then_move_merges_and_reject_restores_the_source_file() {
    let root = temp_workspace();
    let session_id = format!("edit-move-{}", root.file_name().unwrap().to_string_lossy());
    fs::create_dir_all(&root).unwrap();
    let source = root.join("a.txt");
    let destination = root.join("b.txt");
    fs::write(&source, "Amber is gliding.\n").unwrap();
    let recorder =
        WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));

    recorder.start("edit", WorkspaceChangeOperation::Edit, &serde_json::json!({ "path": "a.txt" }));
    fs::write(&source, "Yoimiya is preparing fireworks.\n").unwrap();
    recorder.finish("edit", r#"{"ok":true}"#).unwrap();
    recorder.start(
        "move",
        WorkspaceChangeOperation::Move,
        &serde_json::json!({ "src": "a.txt", "dest": "b.txt" }),
    );
    fs::rename(&source, &destination).unwrap();
    let change = recorder.finish("move", r#"{"ok":true}"#).unwrap();

    assert!(matches!(change.operation, WorkspaceChangeOperation::Move));
    assert_eq!(change.source_path.as_deref(), Some("a.txt"));
    assert_eq!(change.destination_path.as_deref(), Some("b.txt"));
    assert_eq!(change.merged_count, 2);
    cancel(&session_id, &change.change_id).unwrap();
    assert_eq!(fs::read_to_string(&source).unwrap(), "Amber is gliding.\n");
    assert!(!destination.exists());
    let _ = fs::remove_dir_all(root);
}

/// Purpose: Verify copy is represented as one atomic record and Reject removes only the copied destination.
/// Test Data: `traveler.txt` contains a note from Aether that is copied to `lumine.txt`.
#[test]
fn copy_is_one_atomic_change_and_reject_removes_the_destination() {
    let root = temp_workspace();
    let session_id = format!("copy-{}", root.file_name().unwrap().to_string_lossy());
    fs::create_dir_all(&root).unwrap();
    let source = root.join("traveler.txt");
    let destination = root.join("lumine.txt");
    fs::write(&source, "Aether is looking for his sibling.\n").unwrap();
    let recorder =
        WorkspaceChangeRecorder::new(&session_id, Some(root.to_string_lossy().to_string()));

    recorder.start(
        "copy",
        WorkspaceChangeOperation::Copy,
        &serde_json::json!({ "src": "traveler.txt", "dest": "lumine.txt" }),
    );
    fs::copy(&source, &destination).unwrap();
    let change = recorder.finish("copy", r#"{"ok":true}"#).unwrap();

    assert!(matches!(change.operation, WorkspaceChangeOperation::Copy));
    assert_eq!(change.source_path.as_deref(), Some("traveler.txt"));
    assert_eq!(change.destination_path.as_deref(), Some("lumine.txt"));
    assert_eq!(pending(&session_id).unwrap().len(), 1);
    cancel(&session_id, &change.change_id).unwrap();
    assert_eq!(fs::read_to_string(&source).unwrap(), "Aether is looking for his sibling.\n");
    assert!(!destination.exists());
    let _ = fs::remove_dir_all(root);
}
