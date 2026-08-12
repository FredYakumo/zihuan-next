use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zihuan_core::agent::brain::BrainTool;

use crate::tools::workspace_tools::{FindFilesBrainTool, DEFAULT_TOOL_FIND_FILES};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-find-files-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

/// Purpose: Verify that find_files filters results by name glob and type
/// while excluding directories listed in the exclude option, and that it
/// advertises the expected tool name via its spec.
///
/// Test Data: A temporary directory containing src/lib.rs with "lib" and
/// target/nested/lib.rs with "ignored". Executes with path ".", name "*.rs",
/// type "file", and exclude ["target"]. Expects ok true and exactly 1 match
/// whose path ends with "lib.rs".
#[test]
fn find_files_filters_name_type_and_excluded_directories() {
    let directory = temp_dir();
    fs::create_dir_all(directory.join("target/nested")).unwrap();
    fs::create_dir_all(directory.join("src")).unwrap();
    fs::write(directory.join("src/lib.rs"), "lib").unwrap();
    fs::write(directory.join("target/nested/lib.rs"), "ignored").unwrap();
    let tool = FindFilesBrainTool { workspace_path: Some(directory.clone()) };

    assert_eq!(tool.spec().name(), DEFAULT_TOOL_FIND_FILES);
    let result = tool.execute("", &json!({"path":".","name":"*.rs","type":"file","exclude":["target"]}));
    let result: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(result["ok"], true);
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert!(matches[0]["path"].as_str().unwrap().ends_with("lib.rs"));

    fs::remove_dir_all(directory).unwrap();
}
