use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use zihuan_agent::brain::{BrainTool, ToolExecutionOutput};
use zihuan_core::error::Error;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::runtime::block_async;
use zihuan_core::workspace::AskUserRequest;

use super::common::StaticFunctionToolSpec;

pub(crate) const DEFAULT_TOOL_CREATE_FILE: &str = "create_file";
pub(crate) const DEFAULT_TOOL_DELETE_FILE: &str = "delete_file";
pub(crate) const DEFAULT_TOOL_EDIT_FILE: &str = "edit_file";
pub(crate) const DEFAULT_TOOL_EXEC_CMD: &str = "exec_cmd";
pub(crate) const DEFAULT_TOOL_ASK_USER: &str = "ask_user";

fn resolve_tool_path(workspace_path: Option<&Path>, raw_path: &str) -> Result<PathBuf, Error> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError("path must not be empty".to_string()));
    }

    let target = PathBuf::from(trimmed);
    if target.is_absolute() {
        return Ok(target);
    }

    let Some(base) = workspace_path else {
        return Err(Error::ValidationError(
            "workspace_path is required for relative file operations".to_string(),
        ));
    };
    Ok(base.join(target))
}

fn json_error(message: impl Into<String>) -> String {
    serde_json::json!({ "error": message.into() }).to_string()
}

fn success_json(value: Value) -> String {
    value.to_string()
}

#[derive(Debug, Clone)]
pub(crate) struct CreateFileBrainTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct CreateFileArgs {
    path: String,
    content: String,
    #[serde(default)]
    overwrite: bool,
}

impl BrainTool for CreateFileBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_CREATE_FILE,
            description: "Create a file using the provided content",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path or path relative to the current workspace" },
                    "content": { "type": "string", "description": "Full file content to write" },
                    "overwrite": { "type": "boolean", "description": "Whether to overwrite an existing file" }
                },
                "required": ["path", "content"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let args: CreateFileArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid create_file arguments: {err}")),
        };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) {
            Ok(path) => path,
            Err(err) => return json_error(err.to_string()),
        };
        if path.exists() && !args.overwrite {
            return json_error(format!("file already exists: {}", path.display()));
        }
        if let Some(parent) = path.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                return json_error(format!("failed to create parent directory '{}': {err}", parent.display()));
            }
        }
        if let Err(err) = fs::write(&path, args.content) {
            return json_error(format!("failed to write file '{}': {err}", path.display()));
        }
        success_json(serde_json::json!({
            "ok": true,
            "path": path.display().to_string(),
        }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DeleteFileBrainTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct DeleteFileArgs {
    path: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    missing_ok: bool,
}

impl BrainTool for DeleteFileBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_DELETE_FILE,
            description: "Delete a file or directory from disk",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path or path relative to the current workspace" },
                    "recursive": { "type": "boolean", "description": "Required when deleting a directory" },
                    "missing_ok": { "type": "boolean", "description": "Ignore missing target paths" }
                },
                "required": ["path"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let args: DeleteFileArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid delete_file arguments: {err}")),
        };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) {
            Ok(path) => path,
            Err(err) => return json_error(err.to_string()),
        };
        if !path.exists() {
            if args.missing_ok {
                return success_json(serde_json::json!({
                    "ok": true,
                    "path": path.display().to_string(),
                    "deleted": false,
                }));
            }
            return json_error(format!("path does not exist: {}", path.display()));
        }
        let delete_result = if path.is_dir() {
            if !args.recursive {
                return json_error("recursive=true is required to delete a directory".to_string());
            }
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match delete_result {
            Ok(()) => success_json(serde_json::json!({
                "ok": true,
                "path": path.display().to_string(),
                "deleted": true,
            })),
            Err(err) => json_error(format!("failed to delete '{}': {err}", path.display())),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EditFileBrainTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct EditFileArgs {
    path: String,
    edits: Vec<LineEditSpec>,
}

#[derive(Debug, Clone, Deserialize)]
struct LineEditSpec {
    start_line: usize,
    end_line: usize,
    replacement_lines: Vec<String>,
}

impl BrainTool for EditFileBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_EDIT_FILE,
            description: "Replace or delete existing file lines using 1-based inclusive line ranges",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path or path relative to the current workspace" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "start_line": { "type": "integer", "minimum": 1 },
                                "end_line": { "type": "integer", "minimum": 1 },
                                "replacement_lines": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["start_line", "end_line", "replacement_lines"]
                        }
                    }
                },
                "required": ["path", "edits"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let args: EditFileArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid edit_file arguments: {err}")),
        };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) {
            Ok(path) => path,
            Err(err) => return json_error(err.to_string()),
        };
        let original = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => return json_error(format!("failed to read file '{}': {err}", path.display())),
        };

        let trailing_newline = original.ends_with('\n');
        let mut lines: Vec<String> = original.lines().map(ToOwned::to_owned).collect();
        let mut edits = args.edits;
        edits.sort_by(|left, right| {
            right
                .start_line
                .cmp(&left.start_line)
                .then_with(|| right.end_line.cmp(&left.end_line))
        });

        for edit in edits {
            if edit.start_line == 0 || edit.end_line == 0 || edit.start_line > edit.end_line {
                return json_error(format!(
                    "invalid line range: start_line={} end_line={}",
                    edit.start_line, edit.end_line
                ));
            }
            if edit.end_line > lines.len() {
                return json_error(format!(
                    "line range [{}-{}] is out of bounds for file '{}' with {} lines",
                    edit.start_line,
                    edit.end_line,
                    path.display(),
                    lines.len()
                ));
            }
            let start = edit.start_line - 1;
            let end = edit.end_line;
            lines.splice(start..end, edit.replacement_lines.into_iter());
        }

        let mut rewritten = lines.join("\n");
        if trailing_newline && !rewritten.is_empty() {
            rewritten.push('\n');
        }
        if let Err(err) = fs::write(&path, rewritten) {
            return json_error(format!("failed to write edited file '{}': {err}", path.display()));
        }
        success_json(serde_json::json!({
            "ok": true,
            "path": path.display().to_string(),
            "line_count": lines.len(),
        }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ExecCmdBrainTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ExecCmdArgs {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

impl BrainTool for ExecCmdBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_EXEC_CMD,
            description: "Execute a shell command using PowerShell on Windows or Bash on other systems",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command text to execute" },
                    "cwd": { "type": "string", "description": "Optional working directory, absolute or relative to the current workspace" },
                    "timeout_secs": { "type": "integer", "minimum": 1, "description": "Optional timeout in seconds" }
                },
                "required": ["command"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let args: ExecCmdArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid exec_cmd arguments: {err}")),
        };
        let cwd = if let Some(raw_cwd) = args.cwd.as_deref() {
            match resolve_tool_path(self.workspace_path.as_deref(), raw_cwd) {
                Ok(path) => Some(path),
                Err(err) => return json_error(err.to_string()),
            }
        } else {
            self.workspace_path.clone()
        };

        let timeout_secs = args.timeout_secs.unwrap_or(30);
        let command_cwd = cwd.clone();
        let output_result = block_async(async move {
            let mut command = if cfg!(windows) {
                let mut cmd = Command::new("powershell");
                cmd.args(["-NoProfile", "-Command", &args.command]);
                cmd
            } else {
                let mut cmd = Command::new("bash");
                cmd.args(["-lc", &args.command]);
                cmd
            };
            if let Some(path) = command_cwd.as_ref() {
                command.current_dir(path);
            }
            timeout(Duration::from_secs(timeout_secs), command.output()).await
        });

        match output_result {
            Ok(Ok(output)) => success_json(serde_json::json!({
                "ok": output.status.success(),
                "status": output.status.code(),
                "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                "shell": if cfg!(windows) { "powershell" } else { "bash" },
                "cwd": cwd.as_ref().map(|path| path.display().to_string()),
            })),
            Ok(Err(err)) => json_error(format!("failed to execute command: {err}")),
            Err(_) => json_error(format!("command timed out after {timeout_secs}s")),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct AskUserBrainTool;

#[derive(Debug, Deserialize)]
struct AskUserArgs {
    question: String,
    #[serde(default)]
    details: Option<String>,
    #[serde(default)]
    placeholder: Option<String>,
}

impl BrainTool for AskUserBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_ASK_USER,
            description: "Ask the dashboard user for missing details and pause until they reply",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "The direct question to ask the user" },
                    "details": { "type": "string", "description": "Optional extra context or constraints" },
                    "placeholder": { "type": "string", "description": "Optional placeholder for the answer input box" }
                },
                "required": ["question"]
            }),
        })
    }

    fn execute_with_outcome(&self, _call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        let args: AskUserArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return ToolExecutionOutput::text(json_error(format!("invalid ask_user arguments: {err}"))),
        };
        let question = args.question.trim().to_string();
        if question.is_empty() {
            return ToolExecutionOutput::text(json_error("question must not be empty"));
        }
        let request = AskUserRequest {
            question: question.clone(),
            details: args.details.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
            placeholder: args
                .placeholder
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        };
        ToolExecutionOutput::ask_user(
            serde_json::json!({
                "ok": true,
                "awaiting_user_input": true,
                "question": question,
            })
            .to_string(),
            request,
        )
    }

    fn execute(&self, _call_content: &str, _arguments: &Value) -> String {
        unreachable!("ask_user uses execute_with_outcome")
    }
}
