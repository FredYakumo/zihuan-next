use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zihuan_core::agent::brain::BrainTool;

use crate::agent::tools::{CopyFileBrainTool, FileInfoBrainTool, MoveFileBrainTool, DEFAULT_TOOL_COPY_FILE, DEFAULT_TOOL_FILE_INFO, DEFAULT_TOOL_MOVE_FILE};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-file-ops-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

/// Purpose: Verify the common copy -> move -> file-info workflow: copying a
/// file duplicates it, moving renames it, and file info reports the correct
/// metadata, while all three tools advertise their expected names via spec.
///
/// Test Data: A temporary directory containing source.txt with
/// "one\ntwo\n". Copy executes with src "source.txt" and dest "backup.txt";
/// move executes with src "backup.txt" and dest "renamed.txt"; info executes
/// with path "renamed.txt". Expects ok true for copy and move, and info
/// reporting type "file", line_count 2, and is_binary false.
#[test]
fn copy_move_and_file_info_support_common_file_workflow() {
    let directory = temp_dir();
    fs::write(directory.join("source.txt"), "one\ntwo\n").unwrap();
    let copy = CopyFileBrainTool { workspace_path: Some(directory.clone()) };
    let move_tool = MoveFileBrainTool { workspace_path: Some(directory.clone()) };
    let info = FileInfoBrainTool { workspace_path: Some(directory.clone()) };

    assert_eq!(copy.spec().name(), DEFAULT_TOOL_COPY_FILE);
    assert_eq!(move_tool.spec().name(), DEFAULT_TOOL_MOVE_FILE);
    assert_eq!(info.spec().name(), DEFAULT_TOOL_FILE_INFO);
    let copied = serde_json::from_str::<serde_json::Value>(&copy.execute("", &json!({"src":"source.txt","dest":"backup.txt"}))).unwrap();
    assert_eq!(copied["ok"], true);
    let moved = serde_json::from_str::<serde_json::Value>(&move_tool.execute("", &json!({"src":"backup.txt","dest":"renamed.txt"}))).unwrap();
    assert_eq!(moved["ok"], true);
    let metadata = serde_json::from_str::<serde_json::Value>(&info.execute("", &json!({"path":"renamed.txt"}))).unwrap();
    assert_eq!(metadata["type"], "file");
    assert_eq!(metadata["line_count"], 2);
    assert_eq!(metadata["is_binary"], false);

    fs::remove_dir_all(directory).unwrap();
}
