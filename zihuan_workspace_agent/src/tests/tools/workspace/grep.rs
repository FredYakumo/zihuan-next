use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zihuan_core::agent::tools::Tool;

use crate::tools::workspace_tools::{GrepTool, DEFAULT_TOOL_GREP};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-grep-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).expect("create temporary directory");
    path
}

/// Purpose: Verify that the grep tool finds literal string matches recursively
/// from the workspace path and returns the surrounding context lines, while
/// still reporting the correct tool name via its spec.
///
/// Test Data: A temporary directory containing root.txt with
/// "before\nneedle here\nafter" and nested/nested.txt with "another needle".
/// Executes with path ".", pattern "needle", context_lines 1, glob "*.txt".
/// Expects 2 total matches; the root.txt match is on line 2 with
/// context_before "before" and context_after "after".
#[test]
fn grep_finds_literal_matches_recursively_with_context() {
    let directory = temp_dir();
    let nested = directory.join("nested");
    fs::create_dir(&nested).expect("create nested directory");
    fs::write(directory.join("root.txt"), "before\nneedle here\nafter").expect("write root file");
    fs::write(nested.join("nested.txt"), "another needle").expect("write nested file");
    let tool = GrepTool {
        workspace_path: Some(directory.clone()),
    };

    assert_eq!(tool.spec().name(), DEFAULT_TOOL_GREP);
    let result = tool.execute(
        "",
        &json!({"path": ".", "pattern": "needle", "context_lines": 1, "glob": "*.txt"}),
    );
    let result: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    assert_eq!(result["ok"], true);
    assert_eq!(result["total_matches"], 2);
    let matches = result["matches"].as_array().expect("matches array");
    let root_match = matches
        .iter()
        .find(|item| item["path"].as_str().unwrap_or_default().ends_with("root.txt"))
        .expect("root match");
    assert_eq!(root_match["line"], 2);
    assert_eq!(root_match["context_before"][0], "before");
    assert_eq!(root_match["context_after"][0], "after");

    fs::remove_dir_all(directory).expect("remove temporary directory");
}

/// Purpose: Verify that max_results caps the number of returned matches while
/// total_matches still reports the full match count, and that binary files
/// are skipped during the search.
///
/// Test Data: A temporary directory containing text.txt with
/// "needle\nneedle\nneedle" and binary.bin with bytes [0, 159, 146, 150].
/// Executes with pattern "needle" and max_results 2. Expects total_matches 3,
/// 2 entries in the returned matches array, and truncated == true.
#[test]
fn grep_honors_max_results_and_skips_binary_files() {
    let directory = temp_dir();
    fs::write(directory.join("text.txt"), "needle\nneedle\nneedle").expect("write text file");
    fs::write(directory.join("binary.bin"), [0_u8, 159, 146, 150]).expect("write binary file");
    let tool = GrepTool {
        workspace_path: Some(directory.clone()),
    };

    let result = tool.execute(
        "",
        &json!({"path": ".", "pattern": "needle", "max_results": 2}),
    );
    let result: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    assert_eq!(result["total_matches"], 3);
    assert_eq!(result["matches"].as_array().expect("matches array").len(), 2);
    assert_eq!(result["truncated"], true);

    fs::remove_dir_all(directory).expect("remove temporary directory");
}
