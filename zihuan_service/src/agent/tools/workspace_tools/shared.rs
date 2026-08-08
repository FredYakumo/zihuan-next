use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use zihuan_agent::brain::ToolExecutionResource;
use zihuan_core::error::Error;

pub(crate) const DEFAULT_MAX_RESULTS: usize = 100;
pub(crate) const DEFAULT_MAX_ENTRIES: usize = 200;
const MAX_SEARCH_FILE_BYTES: u64 = 4 * 1024 * 1024;

pub(crate) fn resolve_tool_path(workspace_path: Option<&Path>, raw_path: &str) -> Result<PathBuf, Error> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError("path must not be empty".to_string()));
    }
    let target = PathBuf::from(trimmed);
    if target.is_absolute() {
        return Ok(target);
    }
    let Some(base) = workspace_path else {
        return Err(Error::ValidationError("workspace_path is required for relative file operations".to_string()));
    };
    Ok(base.join(target))
}

pub(crate) fn json_error(message: impl Into<String>) -> String {
    serde_json::json!({ "error": message.into() }).to_string()
}

pub(crate) fn success_json(value: Value) -> String { value.to_string() }

pub(crate) fn path_resource(workspace_path: Option<&Path>, raw_path: &str, write: bool) -> ToolExecutionResource {
    match resolve_tool_path(workspace_path, raw_path) {
        Ok(path) if write => ToolExecutionResource::Write(path),
        Ok(path) => ToolExecutionResource::Read(path),
        Err(_) => ToolExecutionResource::Exclusive,
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SearchArgs {
    pub(crate) path: String,
    pub(crate) pattern: String,
    #[serde(default)] pub(crate) glob: Option<String>,
    #[serde(default)] pub(crate) max_results: Option<usize>,
    #[serde(default)] pub(crate) context_lines: usize,
    #[serde(default)] pub(crate) case_sensitive: bool,
}

#[derive(Debug, Clone)]
struct SearchMatch { path: String, line: usize, content: String, context_before: Vec<String>, context_after: Vec<String> }

fn wildcard_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let mut dp = vec![vec![false; text.len() + 1]; pattern.len() + 1];
    dp[0][0] = true;
    for i in 1..=pattern.len() {
        if pattern[i - 1] == '*' { dp[i][0] = dp[i - 1][0]; }
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

fn glob_matches(path: &Path, root: &Path, glob: Option<&str>) -> bool {
    let Some(glob) = glob.map(str::trim).filter(|value| !value.is_empty()) else { return true; };
    let relative = path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/");
    wildcard_matches(glob, &relative)
        || wildcard_matches(glob, path.file_name().and_then(|value| value.to_str()).unwrap_or_default())
}

fn collect_search_files(root: &Path, glob: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let mut pending = VecDeque::from([root.to_path_buf()]);
    let mut files = Vec::new();
    while let Some(path) = pending.pop_front() {
        let metadata = fs::metadata(&path).map_err(|err| format!("failed to inspect '{}': {err}", path.display()))?;
        if metadata.is_file() {
            if metadata.len() <= MAX_SEARCH_FILE_BYTES && glob_matches(&path, root, glob) { files.push(path); }
            continue;
        }
        if !metadata.is_dir() { continue; }
        let mut children = Vec::new();
        for entry in fs::read_dir(&path).map_err(|err| format!("failed to list directory '{}': {err}", path.display()))? {
            let entry = entry.map_err(|err| format!("failed to inspect directory '{}': {err}", path.display()))?;
            if !entry.file_name().to_string_lossy().starts_with('.') { children.push(entry.path()); }
        }
        children.sort();
        pending.extend(children);
    }
    files.sort();
    Ok(files)
}

fn search_file(path: &Path, matcher: &dyn Fn(&str) -> bool, context_lines: usize) -> Result<Vec<SearchMatch>, String> {
    let content = fs::read_to_string(path).map_err(|err| format!("failed to read '{}': {err}", path.display()))?;
    let lines: Vec<&str> = content.lines().collect();
    let mut matches = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if !matcher(line) { continue; }
        let start = index.saturating_sub(context_lines);
        let end = (index + context_lines + 1).min(lines.len());
        matches.push(SearchMatch {
            path: path.display().to_string(), line: index + 1, content: (*line).to_string(),
            context_before: lines[start..index].iter().map(|value| (*value).to_string()).collect(),
            context_after: lines[index + 1..end].iter().map(|value| (*value).to_string()).collect(),
        });
    }
    Ok(matches)
}

pub(crate) fn execute_search(args: SearchArgs, workspace_path: Option<&Path>, regex_mode: bool) -> String {
    let path = match resolve_tool_path(workspace_path, &args.path) { Ok(path) => path, Err(err) => return json_error(err.to_string()) };
    if !path.exists() { return json_error(format!("path does not exist: {}", path.display())); }
    let max_results = args.max_results.unwrap_or(DEFAULT_MAX_RESULTS);
    if max_results == 0 { return json_error("max_results must be greater than zero"); }
    let pattern = args.pattern.clone();
    let regex_pattern = if args.case_sensitive { args.pattern.clone() } else if regex_mode { format!("(?i:{})", args.pattern) } else { format!("(?i){}", regex::escape(&args.pattern)) };
    let regex = if regex_mode { match Regex::new(&regex_pattern) { Ok(value) => Some(value), Err(err) => return json_error(format!("invalid rg pattern: {err}")) } } else { None };
    let matcher = |line: &str| regex.as_ref().map_or_else(|| if args.case_sensitive { line.contains(&args.pattern) } else { line.to_lowercase().contains(&args.pattern.to_lowercase()) }, |value| value.is_match(line));
    let files = match collect_search_files(&path, args.glob.as_deref()) { Ok(files) => files, Err(err) => return json_error(err) };
    let mut matches = Vec::new();
    for file in files { if let Ok(mut file_matches) = search_file(&file, &matcher, args.context_lines) { matches.append(&mut file_matches); } }
    let total_matches = matches.len();
    let results: Vec<Value> = matches.into_iter().take(max_results).map(|item| serde_json::json!({
        "path": item.path, "line": item.line, "content": item.content,
        "context_before": item.context_before, "context_after": item.context_after,
    })).collect();
    success_json(serde_json::json!({ "ok": true, "path": path.display().to_string(), "pattern": pattern,
        "matches": results, "total_matches": total_matches, "truncated": total_matches > max_results }))
}
