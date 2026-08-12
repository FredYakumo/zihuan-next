use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use zihuan_core::agent::brain::BrainTool;

use crate::tools::workspace_tools::{ExecCmdBrainTool, ListDirBrainTool, ReadFileBrainTool, RgBrainTool};

fn temp_dir() -> PathBuf {
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let path = std::env::temp_dir().join(format!("zihuan-enhanced-tools-{}-{suffix}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

/// Purpose: Verify that ReadFileBrainTool supports base64 encoding with an
/// inclusive byte range, and that ListDirBrainTool filters entries by a
/// name_glob pattern in the same workspace.
///
/// Test Data: A temporary directory containing data.bin with bytes
/// [0, 1, 2, 255] and keep.txt with "keep". Read executes with path
/// "data.bin", encoding "base64", byte_start 1, byte_end 3, expecting
/// content "AQI=". List executes with path "." and name_glob "*.txt",
/// expecting exactly 1 entry.
#[test]
fn read_file_supports_base64_and_list_dir_name_glob() {
    let directory = temp_dir();
    fs::write(directory.join("data.bin"), [0_u8, 1, 2, 255]).unwrap();
    fs::write(directory.join("keep.txt"), "keep").unwrap();
    let read = ReadFileBrainTool { workspace_path: Some(directory.clone()) };
    let list = ListDirBrainTool { workspace_path: Some(directory.clone()) };

    let binary = serde_json::from_str::<serde_json::Value>(&read.execute("", &json!({"path":"data.bin","encoding":"base64","byte_start":1,"byte_end":3}))).unwrap();
    assert_eq!(binary["encoding"], "base64");
        assert_eq!(binary["content"], "AQI=");
    let listing = serde_json::from_str::<serde_json::Value>(&list.execute("", &json!({"path":".","name_glob":"*.txt"}))).unwrap();
    assert_eq!(listing["entries"].as_array().unwrap().len(), 1);

    fs::remove_dir_all(directory).unwrap();
}

/// Purpose: Verify that the rg tool extracts a named capture group via the
/// output template and deduplicates matching values when unique is true.
///
/// Test Data: A temporary directory containing ids.txt with
/// "id=one\nid=two\nid=one\n". Executes with path ".", pattern
/// "id=(?<id>[a-z]+)", output "$id", and unique true. Expects 2 matches
/// with the first value "one".
#[test]
fn rg_extracts_capture_group_and_deduplicates_values() {
    let directory = temp_dir();
    fs::write(directory.join("ids.txt"), "id=one\nid=two\nid=one\n").unwrap();
    let tool = RgBrainTool { workspace_path: Some(directory.clone()) };
    let result = serde_json::from_str::<serde_json::Value>(&tool.execute("", &json!({"path":".","pattern":"id=(?<id>[a-z]+)","output":"$id","unique":true}))).unwrap();
    assert_eq!(result["matches"].as_array().unwrap().len(), 2);
    assert_eq!(result["matches"][0]["value"], "one");

    fs::remove_dir_all(directory).unwrap();
}

/// Purpose: Verify that ExecCmdBrainTool captures stdout while passing
/// environment variables and stdin to the spawned subprocess.
///
/// Test Data: A temporary directory; executes a PowerShell command that
/// reads stdin via [Console]::In and echoes $env:ZIHUAN_TEST plus the input,
/// with env {"ZIHUAN_TEST": "ok"} and input "data". Expects ok true and
/// the combined stdout/stderr to contain "data".
#[cfg(windows)]
#[test]
fn exec_cmd_accepts_environment_and_stdin() {
    let directory = temp_dir();
    let tool = ExecCmdBrainTool { workspace_path: Some(directory.clone()) };
        let result = serde_json::from_str::<serde_json::Value>(&tool.execute("", &json!({"command":"$data = [Console]::In.ReadToEnd(); Write-Output ($env:ZIHUAN_TEST + ':' + $data)","env":{"ZIHUAN_TEST":"ok"},"input":"data"}))).unwrap();
    assert_eq!(result["ok"], true);
    let output = format!("{}{}", result["stdout"].as_str().unwrap_or_default(), result["stderr"].as_str().unwrap_or_default());
    assert!(output.contains("data"));
    fs::remove_dir_all(directory).unwrap();
}
