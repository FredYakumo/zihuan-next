use std::fs;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use zihuan_agent::brain::{BrainTool, ToolExecutionOutput, ToolExecutionResource};
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
pub(crate) const DEFAULT_TOOL_READ_FILE: &str = "read_file";
pub(crate) const DEFAULT_TOOL_LIST_DIR: &str = "list_dir";
pub(crate) const DEFAULT_TOOL_GREP: &str = "grep";
pub(crate) const DEFAULT_TOOL_RG: &str = "rg";

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

fn path_resource(workspace_path: Option<&Path>, raw_path: &str, write: bool) -> ToolExecutionResource {
    match resolve_tool_path(workspace_path, raw_path) {
        Ok(path) if write => ToolExecutionResource::Write(path),
        Ok(path) => ToolExecutionResource::Read(path),
        Err(_) => ToolExecutionResource::Exclusive,
    }
}

const DEFAULT_MAX_RESULTS: usize = 100;
const DEFAULT_MAX_ENTRIES: usize = 200;
const MAX_SEARCH_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
struct ReadFileArgs {
    path: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
    end_line: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct ReadFileBrainTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

impl BrainTool for ReadFileBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_READ_FILE,
            description: "Read a UTF-8 text file, optionally selecting a 1-based inclusive line range",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path or path relative to the current workspace" },
                    "start_line": { "type": "integer", "minimum": 1, "description": "Optional first line, 1-based and inclusive" },
                    "end_line": { "type": "integer", "minimum": 1, "description": "Optional last line, 1-based and inclusive" }
                },
                "required": ["path"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let args: ReadFileArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid read_file arguments: {err}")),
        };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) {
            Ok(path) => path,
            Err(err) => return json_error(err.to_string()),
        };
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => return json_error(format!("failed to read file '{}': {err}", path.display())),
        };
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let start_line = args.start_line.unwrap_or(1);
        let end_line = args.end_line.unwrap_or_else(|| total_lines.max(1));
        if start_line == 0 || end_line == 0 || start_line > end_line {
            return json_error(format!("invalid line range: start_line={start_line} end_line={end_line}"));
        }
        if total_lines == 0 {
            if args.start_line.is_some() || args.end_line.is_some() {
                return json_error("line range is out of bounds for an empty file");
            }
            return success_json(serde_json::json!({
                "ok": true,
                "path": path.display().to_string(),
                "start_line": 0,
                "end_line": 0,
                "total_lines": 0,
                "content": "",
            }));
        }
        if start_line > total_lines || end_line > total_lines {
            return json_error(format!(
                "line range [{start_line}-{end_line}] is out of bounds for file '{}' with {total_lines} lines",
                path.display()
            ));
        }
        let selected = lines[start_line - 1..end_line].join("\n");
        success_json(serde_json::json!({
            "ok": true,
            "path": path.display().to_string(),
            "start_line": start_line,
            "end_line": end_line,
            "total_lines": total_lines,
            "content": selected,
        }))
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<ReadFileArgs>(arguments.clone())
            .map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false))
            .unwrap_or(ToolExecutionResource::Exclusive)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ListDirArgs {
    path: String,
    #[serde(default)]
    recursive: bool,
    #[serde(default)]
    include_hidden: bool,
    #[serde(default)]
    max_entries: Option<usize>,
}

#[derive(Debug, Clone)]
pub(crate) struct ListDirBrainTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

impl BrainTool for ListDirBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_LIST_DIR,
            description: "List files and directories in a workspace path, optionally recursively",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path or path relative to the current workspace" },
                    "recursive": { "type": "boolean", "description": "Whether to list nested directories" },
                    "include_hidden": { "type": "boolean", "description": "Whether to include dot-prefixed entries" },
                    "max_entries": { "type": "integer", "minimum": 1, "description": "Maximum number of entries to return" }
                },
                "required": ["path"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let args: ListDirArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid list_dir arguments: {err}")),
        };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) {
            Ok(path) => path,
            Err(err) => return json_error(err.to_string()),
        };
        if !path.is_dir() {
            return json_error(format!("directory does not exist: {}", path.display()));
        }
        let max_entries = args.max_entries.unwrap_or(DEFAULT_MAX_ENTRIES);
        if max_entries == 0 {
            return json_error("max_entries must be greater than zero");
        }
        let mut pending = VecDeque::from([path.clone()]);
        let mut entries = Vec::new();
        while let Some(directory) = pending.pop_front() {
            let read_dir = match fs::read_dir(&directory) {
                Ok(value) => value,
                Err(err) => return json_error(format!("failed to list directory '{}': {err}", directory.display())),
            };
            let mut children = Vec::new();
            for entry in read_dir {
                let entry = match entry {
                    Ok(value) => value,
                    Err(err) => return json_error(format!("failed to inspect directory '{}': {err}", directory.display())),
                };
                let name = entry.file_name().to_string_lossy().to_string();
                if !args.include_hidden && name.starts_with('.') {
                    continue;
                }
                children.push((name, entry.path(), entry.file_type()));
            }
            children.sort_by(|left, right| left.0.cmp(&right.0));
            for (name, child_path, file_type) in children {
                let file_type = match file_type {
                    Ok(value) => value,
                    Err(err) => return json_error(format!("failed to inspect '{}': {err}", child_path.display())),
                };
                let kind = if file_type.is_dir() { "directory" } else { "file" };
                entries.push(serde_json::json!({
                    "name": name,
                    "path": child_path.display().to_string(),
                    "type": kind,
                }));
                if args.recursive && file_type.is_dir() {
                    pending.push_back(child_path);
                }
                if entries.len() >= max_entries {
                    break;
                }
            }
            if entries.len() >= max_entries {
                break;
            }
        }
        success_json(serde_json::json!({
            "ok": true,
            "path": path.display().to_string(),
            "recursive": args.recursive,
            "entries": entries,
            "truncated": entries.len() >= max_entries,
        }))
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<ListDirArgs>(arguments.clone())
            .map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false))
            .unwrap_or(ToolExecutionResource::Exclusive)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SearchArgs {
    path: String,
    pattern: String,
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    max_results: Option<usize>,
    #[serde(default)]
    context_lines: usize,
    #[serde(default)]
    case_sensitive: bool,
}

#[derive(Debug, Clone)]
struct SearchMatch {
    path: String,
    line: usize,
    content: String,
    context_before: Vec<String>,
    context_after: Vec<String>,
}

fn collect_search_files(root: &Path, glob: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let mut pending = VecDeque::from([root.to_path_buf()]);
    let mut files = Vec::new();
    while let Some(path) = pending.pop_front() {
        let metadata = fs::metadata(&path).map_err(|err| format!("failed to inspect '{}': {err}", path.display()))?;
        if metadata.is_file() {
            if metadata.len() <= MAX_SEARCH_FILE_BYTES && glob_matches(&path, root, glob) {
                files.push(path);
            }
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let read_dir = fs::read_dir(&path).map_err(|err| format!("failed to list directory '{}': {err}", path.display()))?;
        let mut children = Vec::new();
        for entry in read_dir {
            let entry = entry.map_err(|err| format!("failed to inspect directory '{}': {err}", path.display()))?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            children.push(entry.path());
        }
        children.sort();
        pending.extend(children);
    }
    files.sort();
    Ok(files)
}

fn glob_matches(path: &Path, root: &Path, glob: Option<&str>) -> bool {
    let Some(glob) = glob.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/");
    wildcard_matches(glob, &relative) || wildcard_matches(glob, path.file_name().and_then(|value| value.to_str()).unwrap_or_default())
}

fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; text.len() + 1]; pattern.len() + 1];
    dp[0][0] = true;
    for i in 1..=pattern.len() {
        if pattern[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
        for j in 1..=text.len() {
            dp[i][j] = match pattern[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                character => dp[i - 1][j - 1] && character == text[j - 1],
            };
        }
    }
    dp[pattern.len()][text.len()]
}

fn search_file(path: &Path, matcher: &dyn Fn(&str) -> bool, context_lines: usize) -> Result<Vec<SearchMatch>, String> {
    let content = fs::read_to_string(path).map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
    let lines: Vec<&str> = content.lines().collect();
    let mut matches = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !matcher(line) {
            continue;
        }
        let start = index.saturating_sub(context_lines);
        let end = (index + context_lines + 1).min(lines.len());
        matches.push(SearchMatch {
            path: path.display().to_string(),
            line: index + 1,
            content: (*line).to_string(),
            context_before: lines[start..index].iter().map(|value| (*value).to_string()).collect(),
            context_after: lines[index + 1..end].iter().map(|value| (*value).to_string()).collect(),
        });
    }
    Ok(matches)
}

fn search_result(matches: Vec<SearchMatch>, max_results: usize, path: &Path, pattern: &str) -> String {
    let total_matches = matches.len();
    let results: Vec<Value> = matches
        .into_iter()
        .take(max_results)
        .map(|item| {
            serde_json::json!({
                "path": item.path,
                "line": item.line,
                "content": item.content,
                "context_before": item.context_before,
                "context_after": item.context_after,
            })
        })
        .collect();
    success_json(serde_json::json!({
        "ok": true,
        "path": path.display().to_string(),
        "pattern": pattern,
        "matches": results,
        "total_matches": total_matches,
        "truncated": total_matches > max_results,
    }))
}

#[derive(Debug, Clone)]
pub(crate) struct GrepBrainTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

impl BrainTool for GrepBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_GREP,
            description: "Search workspace text files recursively for a literal string",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path or path relative to the current workspace" },
                    "pattern": { "type": "string", "description": "Literal text to find" },
                    "glob": { "type": "string", "description": "Optional file glob such as *.rs" },
                    "max_results": { "type": "integer", "minimum": 1 },
                    "context_lines": { "type": "integer", "minimum": 0 },
                    "case_sensitive": { "type": "boolean" }
                },
                "required": ["path", "pattern"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let args: SearchArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid grep arguments: {err}")),
        };
        execute_search(args, self.workspace_path.as_deref(), false)
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<SearchArgs>(arguments.clone())
            .map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false))
            .unwrap_or(ToolExecutionResource::Exclusive)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RgBrainTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

impl BrainTool for RgBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_RG,
            description: "Search workspace text files recursively using a regular expression",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path or path relative to the current workspace" },
                    "pattern": { "type": "string", "description": "Regular expression to find" },
                    "glob": { "type": "string", "description": "Optional file glob such as *.rs" },
                    "max_results": { "type": "integer", "minimum": 1 },
                    "context_lines": { "type": "integer", "minimum": 0 },
                    "case_sensitive": { "type": "boolean" }
                },
                "required": ["path", "pattern"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let args: SearchArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid rg arguments: {err}")),
        };
        execute_search(args, self.workspace_path.as_deref(), true)
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<SearchArgs>(arguments.clone())
            .map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false))
            .unwrap_or(ToolExecutionResource::Exclusive)
    }
}

fn execute_search(args: SearchArgs, workspace_path: Option<&Path>, regex_mode: bool) -> String {
    let path = match resolve_tool_path(workspace_path, &args.path) {
        Ok(path) => path,
        Err(err) => return json_error(err.to_string()),
    };
    if !path.exists() {
        return json_error(format!("path does not exist: {}", path.display()));
    }
    let max_results = args.max_results.unwrap_or(DEFAULT_MAX_RESULTS);
    if max_results == 0 {
        return json_error("max_results must be greater than zero");
    }
    let pattern = args.pattern.clone();
    let regex_pattern = if args.case_sensitive {
        args.pattern.clone()
    } else if regex_mode {
        format!("(?i:{})", args.pattern)
    } else {
        format!("(?i){}", regex::escape(&args.pattern))
    };
    let regex = if regex_mode {
        match Regex::new(&regex_pattern) {
            Ok(value) => Some(value),
            Err(err) => return json_error(format!("invalid rg pattern: {err}")),
        }
    } else {
        None
    };
    let matcher = |line: &str| {
        if let Some(regex) = &regex {
            regex.is_match(line)
        } else if args.case_sensitive {
            line.contains(&args.pattern)
        } else {
            line.to_lowercase().contains(&args.pattern.to_lowercase())
        }
    };
    let files = match collect_search_files(&path, args.glob.as_deref()) {
        Ok(files) => files,
        Err(err) => return json_error(err),
    };
    let mut matches = Vec::new();
    for file in files {
        match search_file(&file, &matcher, args.context_lines) {
            Ok(mut file_matches) => matches.append(&mut file_matches),
            Err(_) => continue,
        }
    }
    search_result(matches, max_results, &path, &pattern)
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
        if let Err(err) = fs::write(&path, &args.content) {
            return json_error(format!("failed to write file '{}': {err}", path.display()));
        }
        let line_count = args.content.lines().count();
        success_json(serde_json::json!({
            "ok": true,
            "path": path.display().to_string(),
            "line_count": line_count,
        }))
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<CreateFileArgs>(arguments.clone())
            .map(|args| path_resource(self.workspace_path.as_deref(), &args.path, true))
            .unwrap_or(ToolExecutionResource::Exclusive)
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
        let line_count = if path.is_file() {
            fs::read_to_string(&path).ok().map(|s| s.lines().count())
        } else {
            None
        };
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
                "line_count": line_count,
            })),
            Err(err) => json_error(format!("failed to delete '{}': {err}", path.display())),
        }
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<DeleteFileArgs>(arguments.clone())
            .map(|args| path_resource(self.workspace_path.as_deref(), &args.path, true))
            .unwrap_or(ToolExecutionResource::Exclusive)
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

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<EditFileArgs>(arguments.clone())
            .map(|args| path_resource(self.workspace_path.as_deref(), &args.path, true))
            .unwrap_or(ToolExecutionResource::Exclusive)
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

    fn execution_resource(&self, _arguments: &Value) -> ToolExecutionResource {
        ToolExecutionResource::Exclusive
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
            details: args
                .details
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
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

    fn execution_resource(&self, _arguments: &Value) -> ToolExecutionResource {
        ToolExecutionResource::Exclusive
    }
}
