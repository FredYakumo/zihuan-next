use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zihuan_core::agent::tools::Tool;

use crate::tools::workspace_tools::{RgTool, DEFAULT_TOOL_RG};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-rg-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).expect("create temporary directory");
    path
}

/// Purpose: Verify that the rg tool finds matches using a regular expression
/// pattern and reports the total match count, while advertising the expected
/// tool name via its spec.
///
/// Test Data: A temporary directory containing numbers.txt with
/// "item-1\nitem-20\nother". Executes with path "." and pattern
/// "item-[0-9]+". Expects ok true and total_matches 2.
#[test]
fn rg_finds_regular_expression_matches() {
    let directory = temp_dir();
    fs::write(directory.join("numbers.txt"), "item-1\nitem-20\nother").expect("write sample");
    let tool = RgTool {
        workspace_path: Some(directory.clone()),
    };

    assert_eq!(tool.spec().name(), DEFAULT_TOOL_RG);
    let result = tool.execute("", &json!({"path": ".", "pattern": "item-[0-9]+"}));
    let result: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    assert_eq!(result["ok"], true);
    assert_eq!(result["total_matches"], 2);

    fs::remove_dir_all(directory).expect("remove temporary directory");
}

/// Purpose: Verify that an invalid regular expression produces a clear
/// "invalid rg pattern" error instead of panicking.
///
/// Test Data: A temporary directory containing sample.txt with "text".
/// Executes with path "." and pattern "[". Expects an error message containing
/// "invalid rg pattern".
#[test]
fn rg_returns_a_clear_error_for_invalid_regular_expressions() {
    let directory = temp_dir();
    fs::write(directory.join("sample.txt"), "text").expect("write sample");
    let tool = RgTool {
        workspace_path: Some(directory.clone()),
    };

    let result = tool.execute("", &json!({"path": ".", "pattern": "["}));
    let result: serde_json::Value = serde_json::from_str(&result).expect("valid JSON result");
    assert!(result["error"].as_str().unwrap_or_default().contains("invalid rg pattern"));

    fs::remove_dir_all(directory).expect("remove temporary directory");
}
