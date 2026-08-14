use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use zihuan_core::agent::brain::BrainTool;

use crate::tools::workspace_tools::{EditFileBrainTool, ReadFileBrainTool, DEFAULT_TOOL_EDIT_FILE};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-edit-file-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).expect("create temporary directory");
    path
}

fn read_snapshot(directory: &Path, file_name: &str) -> Value {
    let tool = ReadFileBrainTool {
        workspace_path: Some(directory.to_path_buf()),
    };
    let result = tool.execute("", &json!({ "path": file_name }));
    serde_json::from_str(&result).expect("valid read result")
}

fn edit_tool(directory: &Path) -> EditFileBrainTool {
    EditFileBrainTool {
        workspace_path: Some(directory.to_path_buf()),
    }
}

#[test]
fn edit_file_applies_multiple_content_anchored_ranges() {
    let directory = temp_dir();
    let path = directory.join("sample.txt");
    fs::write(&path, "one\ntwo\nthree\nfour\n").expect("write sample");
    let snapshot = read_snapshot(&directory, "sample.txt");
    let tool = edit_tool(&directory);

    assert_eq!(tool.spec().name(), DEFAULT_TOOL_EDIT_FILE);
    let result = tool.execute(
        "",
        &json!({
            "path": "sample.txt",
            "content_hash": snapshot["content_hash"],
            "edits": [
                {
                    "start_line": 2,
                    "end_line": 2,
                    "expected_lines": ["two"],
                    "replacement_lines": ["second", "inserted"]
                },
                {
                    "start_line": 4,
                    "end_line": 4,
                    "expected_lines": ["four"],
                    "replacement_lines": ["fourth"]
                }
            ]
        }),
    );
    let result: Value = serde_json::from_str(&result).expect("valid edit result");

    assert_eq!(result["ok"], true);
    assert_eq!(result["old_line_count"], 4);
    assert_eq!(result["line_count"], 5);
    assert_ne!(result["before_content_hash"], result["after_content_hash"]);
    assert_eq!(
        fs::read_to_string(&path).expect("read edited file"),
        "one\nsecond\ninserted\nthree\nfourth\n"
    );

    fs::remove_dir_all(directory).expect("remove temporary directory");
}

#[test]
fn edit_file_rejects_stale_hash_without_writing() {
    let directory = temp_dir();
    let path = directory.join("sample.txt");
    fs::write(&path, "one\ntwo\nthree\n").expect("write sample");
    let snapshot = read_snapshot(&directory, "sample.txt");
    fs::write(&path, "new\none\ntwo\nthree\n").expect("change sample after read");
    let current = fs::read_to_string(&path).expect("read current file");

    let result = edit_tool(&directory).execute(
        "",
        &json!({
            "path": "sample.txt",
            "content_hash": snapshot["content_hash"],
            "edits": [{
                "start_line": 2,
                "end_line": 2,
                "expected_lines": ["two"],
                "replacement_lines": ["second"]
            }]
        }),
    );
    let result: Value = serde_json::from_str(&result).expect("valid edit result");

    assert_eq!(result["ok"], false);
    assert_eq!(result["error_code"], "stale_file");
    assert_eq!(fs::read_to_string(&path).expect("read unchanged file"), current);

    fs::remove_dir_all(directory).expect("remove temporary directory");
}

#[test]
fn edit_file_rejects_expected_line_mismatch_without_writing() {
    let directory = temp_dir();
    let path = directory.join("sample.txt");
    let original = "fn build_edge_maps(\n    &self,\n) -> Result<OldType> {\n";
    fs::write(&path, original).expect("write sample");
    let snapshot = read_snapshot(&directory, "sample.txt");

    let result = edit_tool(&directory).execute(
        "",
        &json!({
            "path": "sample.txt",
            "content_hash": snapshot["content_hash"],
            "edits": [{
                "start_line": 1,
                "end_line": 3,
                "expected_lines": [") -> Result<OldType> {"],
                "replacement_lines": [") -> Result<NewType> {"]
            }]
        }),
    );
    let result: Value = serde_json::from_str(&result).expect("valid edit result");

    assert_eq!(result["ok"], false);
    assert_eq!(result["error_code"], "expected_lines_mismatch");
    assert_eq!(fs::read_to_string(&path).expect("read unchanged file"), original);

    fs::remove_dir_all(directory).expect("remove temporary directory");
}

#[test]
fn edit_file_rejects_overlapping_ranges_without_writing() {
    let directory = temp_dir();
    let path = directory.join("sample.txt");
    let original = "one\ntwo\nthree\nfour";
    fs::write(&path, original).expect("write sample");
    let snapshot = read_snapshot(&directory, "sample.txt");

    let result = edit_tool(&directory).execute(
        "",
        &json!({
            "path": "sample.txt",
            "content_hash": snapshot["content_hash"],
            "edits": [
                {
                    "start_line": 1,
                    "end_line": 2,
                    "expected_lines": ["one", "two"],
                    "replacement_lines": ["first"]
                },
                {
                    "start_line": 2,
                    "end_line": 3,
                    "expected_lines": ["two", "three"],
                    "replacement_lines": ["middle"]
                }
            ]
        }),
    );
    let result: Value = serde_json::from_str(&result).expect("valid edit result");

    assert_eq!(result["ok"], false);
    assert_eq!(result["error_code"], "overlapping_edits");
    assert_eq!(fs::read_to_string(&path).expect("read unchanged file"), original);

    fs::remove_dir_all(directory).expect("remove temporary directory");
}

#[test]
fn edit_file_preserves_crlf_and_unicode_content() {
    let directory = temp_dir();
    let path = directory.join("sample.txt");
    fs::write(&path, "第一行\r\n第二行\r\n第三行\r\n").expect("write sample");
    let snapshot = read_snapshot(&directory, "sample.txt");

    let result = edit_tool(&directory).execute(
        "",
        &json!({
            "path": "sample.txt",
            "content_hash": snapshot["content_hash"],
            "edits": [{
                "start_line": 2,
                "end_line": 2,
                "expected_lines": ["第二行"],
                "replacement_lines": ["替换行"]
            }]
        }),
    );
    let result: Value = serde_json::from_str(&result).expect("valid edit result");

    assert_eq!(result["ok"], true);
    assert_eq!(
        fs::read(&path).expect("read edited bytes"),
        "第一行\r\n替换行\r\n第三行\r\n".as_bytes()
    );

    fs::remove_dir_all(directory).expect("remove temporary directory");
}