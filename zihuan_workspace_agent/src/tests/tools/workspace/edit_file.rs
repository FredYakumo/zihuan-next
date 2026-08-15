use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zihuan_core::agent::brain::BrainTool;

use crate::tools::workspace_tools::{EditFileBrainTool, DEFAULT_TOOL_EDIT_FILE};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-edit-file-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn test_edit_file_replaces_and_deletes_with_flat_arguments() {
    let directory = temp_dir();
    let file_path = directory.join("sample.txt");
    fs::write(&file_path, "one\ntwo\nthree\nfour\n").unwrap();
    let tool = EditFileBrainTool {
        workspace_path: Some(directory.clone()),
    };

    let replaced = serde_json::from_str::<serde_json::Value>(&tool.execute(
        "",
        &json!({
            "path": "sample.txt",
            "start_line": 2,
            "end_line": 3,
            "replacement_lines": ["second", "third"]
        }),
    ))
    .unwrap();
    assert_eq!(replaced["ok"], true);
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "one\nsecond\nthird\nfour\n");

    let deleted = serde_json::from_str::<serde_json::Value>(&tool.execute(
        "",
        &json!({
            "path": "sample.txt",
            "start_line": 2,
            "end_line": 3,
            "replacement_lines": []
        }),
    ))
    .unwrap();
    assert_eq!(deleted["ok"], true);
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "one\nfour\n");

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn test_edit_file_spec_uses_flat_required_arguments() {
    let tool = EditFileBrainTool {
        workspace_path: None,
    };
    let specification = tool.spec();
    let parameters = specification.parameters();

    assert_eq!(specification.name(), DEFAULT_TOOL_EDIT_FILE);
    assert!(parameters["properties"].get("edits").is_none());
    assert!(parameters["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "path"));
    assert!(parameters["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "start_line"));
    assert!(parameters["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "end_line"));
    assert!(parameters["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "replacement_lines"));
}

#[test]
fn test_edit_file_rejects_missing_start_line_without_changing_file() {
    let directory = temp_dir();
    let file_path = directory.join("sample.txt");
    fs::write(&file_path, "one\ntwo\n").unwrap();
    let tool = EditFileBrainTool {
        workspace_path: Some(directory.clone()),
    };

    let result = serde_json::from_str::<serde_json::Value>(&tool.execute(
        "",
        &json!({
            "path": "sample.txt",
            "end_line": 2,
            "replacement_lines": ["changed"]
        }),
    ))
    .unwrap();

    assert!(result["error"]
        .as_str()
        .unwrap()
        .contains("missing field `start_line`"));
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "one\ntwo\n");

    fs::remove_dir_all(directory).unwrap();
}
