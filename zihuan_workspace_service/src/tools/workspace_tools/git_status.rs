use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::tools::{Tool, ToolExecutionResource};
use zihuan_core::model_inference::llm::tooling::FunctionTool;

use zihuan_core::model_inference::llm::tooling::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json};

pub(crate) const DEFAULT_TOOL_GIT_STATUS: &str = "git_status";
const DEFAULT_MAX_OUTPUT_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Deserialize)]
struct GitStatusArgs {
    #[serde(default = "default_path")]
    path: String,
    #[serde(default)]
    max_output_bytes: Option<usize>,
}

fn default_path() -> String { ".".to_string() }

#[derive(Debug, Clone)]
pub(crate) struct GitStatusTool { pub(crate) workspace_path: Option<PathBuf> }

impl Tool for GitStatusTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_GIT_STATUS,
            description: "Read a concise git branch and working tree status without executing a shell",
            parameters: serde_json::json!({"type":"object","properties":{
                "path":{"type":"string","description":"Repository directory"},
                "max_output_bytes":{"type":"integer","minimum":1}
            }}),
        })
    }

    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: GitStatusArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid git_status arguments: {err}")),
        };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) {
            Ok(path) => path,
            Err(err) => return json_error(err.to_string()),
        };
        if !path.is_dir() { return json_error(format!("repository path is not a directory: {}", path.display())); }
        let max_output_bytes = args.max_output_bytes.unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);
        if max_output_bytes == 0 { return json_error("max_output_bytes must be greater than zero"); }
        let output = match Command::new("git").args(["-C", &path.to_string_lossy(), "status", "--short", "--branch"]).output() {
            Ok(output) => output,
            Err(err) => return json_error(format!("failed to execute git status: {err}")),
        };
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() { return json_error(format!("git status failed: {}", stderr.trim())); }
        let (summary, truncated) = truncate_output(&stdout, max_output_bytes);
        let mut lines = summary.lines();
        let branch_line = lines.next().unwrap_or_default().to_string();
        let branch = branch_line.strip_prefix("## ").unwrap_or(&branch_line).to_string();
        let changes: Vec<Value> = lines.filter(|line| !line.trim().is_empty()).map(parse_status_line).collect();
        success_json(serde_json::json!({
            "ok": true,
            "path": path.display().to_string(),
            "branch": branch,
            "changes": changes,
            "summary": summary,
            "output_truncated": truncated,
        }))
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<GitStatusArgs>(arguments.clone()).map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false)).unwrap_or(ToolExecutionResource::Exclusive)
    }
}

fn parse_status_line(line: &str) -> Value {
    let status = line.get(..2).unwrap_or("").trim().to_string();
    let path = line.get(3..).unwrap_or(line).trim().to_string();
    serde_json::json!({"status":status,"path":path})
}

fn truncate_output(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes { return (value.to_string(), false); }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) { end -= 1; }
    (format!("{}\n[output truncated after {max_bytes} bytes]", &value[..end]), true)
}
