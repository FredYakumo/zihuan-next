use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zihuan_agent::brain::BrainTool;

use crate::agent::tools::{ListDirBrainTool, DEFAULT_TOOL_LIST_DIR};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-list-dir-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).expect("create temporary directory");
    path
}

/// Purpose: Verify that listing a directory returns entries in stable
/// alphabetical order (files and directories interleaved by name), and that
/// the tool advertises the expected tool name via its spec.
///
/// Test Data: A temporary directory containing z-dir (directory), a.txt and
/// m.txt (files). Executes with path ".". Expects names
/// ["a.txt", "m.txt", "z-dir"].
#[test]
fn list_dir_returns_stably_sorted_entries() {
    let directory = temp_dir();
    fs::create_dir(directory.join("z-dir")).expect("create directory");
    fs::write(directory.join("a.txt"), "a").expect("write file");
    fs::write(directory.join("m.txt"), "m").expect("write file");
    let tool = ListDirBrainTool {
        workspace_path: Some(directory.clone()),
    };

    assert_eq!(tool.spec().name(), DEFAULT_TOOL_LIST_DIR);
    let result = tool.execute("", &json!({"path": "."}));
    let result: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    let names: Vec<&str> = result["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .map(|entry| entry["name"].as_str().expect("entry name"))
        .collect();
    assert_eq!(names, vec!["a.txt", "m.txt", "z-dir"]);

    fs::remove_dir_all(directory).expect("remove temporary directory");
}

/// Purpose: Verify that recursive listing descends into nested directories
/// while excluding hidden entries such as dotfiles.
///
/// Test Data: A temporary directory containing nested/child.txt and a hidden
/// .hidden file. Executes with path "." and recursive true. Expects exactly
/// 2 entries ("nested" and "child.txt") and no ".hidden" entry.
#[test]
fn list_dir_can_recurse_and_skip_hidden_entries() {
    let directory = temp_dir();
    let nested = directory.join("nested");
    fs::create_dir(&nested).expect("create nested directory");
    fs::write(nested.join("child.txt"), "child").expect("write child");
    fs::write(directory.join(".hidden"), "hidden").expect("write hidden file");
    let tool = ListDirBrainTool {
        workspace_path: Some(directory.clone()),
    };

    let result = tool.execute("", &json!({"path": ".", "recursive": true}));
    let result: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    let entries = result["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|entry| entry["name"] == "nested"));
    assert!(entries.iter().any(|entry| entry["name"] == "child.txt"));
    assert!(!entries.iter().any(|entry| entry["name"] == ".hidden"));

    fs::remove_dir_all(directory).expect("remove temporary directory");
}
