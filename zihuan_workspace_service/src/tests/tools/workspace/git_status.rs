use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::json;
use zihuan_core::agent::tools::Tool;

use crate::tests::unique_temp_dir;
use crate::tools::workspace_tools::{GitStatusTool, DEFAULT_TOOL_GIT_STATUS};

fn temp_dir() -> PathBuf {
    unique_temp_dir("zihuan-git-status")
}

/// Purpose: Verify that git_status reports the branch and worktree changes
/// by invoking git directly without a shell, and that it advertises the
/// expected tool name via its spec.
///
/// Test Data: A temporary directory initialized with `git init -q` and
/// containing changed.txt with "changed". Executes with path ".". Expects
/// ok true, a branch containing "No commits yet" or "HEAD", and a changes
/// array that includes path "changed.txt".
#[test]
fn git_status_returns_branch_and_worktree_changes_without_a_shell() {
    let directory = temp_dir();
    let init = Command::new("git")
        .args(["-C", directory.to_str().unwrap(), "init", "-q"])
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    fs::write(directory.join("changed.txt"), "changed").unwrap();
    let tool = GitStatusTool { workspace_path: Some(directory.clone()) };

    assert_eq!(tool.spec().name(), DEFAULT_TOOL_GIT_STATUS);
    let output = tool.execute("", &json!({"path":"."})).expect("git_status executes");
    let result = serde_json::from_str::<serde_json::Value>(&output).unwrap();
    assert_eq!(result["ok"], true);
    assert!(
        result["branch"].as_str().unwrap().contains("No commits yet")
            || result["branch"].as_str().unwrap().contains("HEAD")
    );
    assert!(result["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["path"] == "changed.txt"));

    fs::remove_dir_all(directory).unwrap();
}
