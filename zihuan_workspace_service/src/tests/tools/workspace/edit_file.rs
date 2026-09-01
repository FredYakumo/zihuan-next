use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zihuan_core::agent::tools::Tool;

use crate::tools::workspace_tools::{EditFileTool, DEFAULT_TOOL_EDIT_FILE};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path =
        std::env::temp_dir().join(format!("zihuan-edit-file-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn test_edit_file_applies_context_patch() {
    let directory = temp_dir();
    let file_path = directory.join("sample.txt");
    fs::write(&file_path, "one\ntwo\nthree\n").unwrap();
    let tool = EditFileTool { workspace_path: Some(directory.clone()) };
    let result = serde_json::from_str::<serde_json::Value>(&tool.execute("", &json!({ "patch": "*** Begin Patch\n*** Update File: sample.txt\n@@\n one\n-two\n+second\n three\n*** End Patch" }))).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "one\nsecond\nthree\n");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn test_edit_file_rejects_missing_context_without_writing() {
    let directory = temp_dir();
    let file_path = directory.join("sample.txt");
    fs::write(&file_path, "one\ntwo\n").unwrap();
    let tool = EditFileTool { workspace_path: Some(directory.clone()) };
    let result = serde_json::from_str::<serde_json::Value>(&tool.execute("", &json!({ "patch": "*** Begin Patch\n*** Update File: sample.txt\n@@\n-missing\n+changed\n*** End Patch" }))).unwrap();
    assert!(result["error"].as_str().unwrap().contains("failed to find expected context"));
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "one\ntwo\n");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn test_edit_file_spec_requires_patch() {
    let tool = EditFileTool { workspace_path: None };
    let specification = tool.spec();
    assert_eq!(specification.name(), DEFAULT_TOOL_EDIT_FILE);
    assert_eq!(specification.parameters()["required"], json!(["patch"]));
}
