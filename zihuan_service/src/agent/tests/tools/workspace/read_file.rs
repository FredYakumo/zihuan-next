use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zihuan_agent::brain::BrainTool;

use crate::agent::tools::{ReadFileBrainTool, DEFAULT_TOOL_READ_FILE};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-read-file-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).expect("create temporary directory");
    path
}

#[test]
fn read_file_returns_complete_utf8_content_by_default() {
    let directory = temp_dir();
    let path = directory.join("sample.txt");
    fs::write(&path, "第一行\n第二行\n第三行").expect("write sample");
    let tool = ReadFileBrainTool {
        workspace_path: Some(directory.clone()),
    };

    assert_eq!(tool.spec().name(), DEFAULT_TOOL_READ_FILE);
    let result = tool.execute("", &json!({"path": "sample.txt"}));
    let result: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    assert_eq!(result["ok"], true);
    assert_eq!(result["total_lines"], 3);
    assert_eq!(result["content"], "第一行\n第二行\n第三行");

    fs::remove_dir_all(directory).expect("remove temporary directory");
}

#[test]
fn read_file_supports_one_based_inclusive_line_ranges() {
    let directory = temp_dir();
    fs::write(directory.join("sample.txt"), "one\ntwo\nthree\nfour").expect("write sample");
    let tool = ReadFileBrainTool {
        workspace_path: Some(directory.clone()),
    };

    let result = tool.execute(
        "",
        &json!({"path": "sample.txt", "start_line": 2, "end_line": 3}),
    );
    let result: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    assert_eq!(result["start_line"], 2);
    assert_eq!(result["end_line"], 3);
    assert_eq!(result["content"], "two\nthree");

    fs::remove_dir_all(directory).expect("remove temporary directory");
}

#[test]
fn read_file_rejects_out_of_bounds_ranges() {
    let directory = temp_dir();
    fs::write(directory.join("sample.txt"), "one\ntwo").expect("write sample");
    let tool = ReadFileBrainTool {
        workspace_path: Some(directory.clone()),
    };

    let result = tool.execute(
        "",
        &json!({"path": "sample.txt", "start_line": 3, "end_line": 3}),
    );
    let result: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    assert!(result["error"].as_str().unwrap_or_default().contains("out of bounds"));

    fs::remove_dir_all(directory).expect("remove temporary directory");
}
