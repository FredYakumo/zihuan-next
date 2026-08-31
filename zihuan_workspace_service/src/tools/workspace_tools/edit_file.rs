use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::tools::{Tool, ToolExecutionResource};
use zihuan_core::model_inference::llm::tooling::{FunctionTool, StaticFunctionToolSpec};

use super::shared::{json_error, path_resource, resolve_tool_path, success_json};

pub(crate) const DEFAULT_TOOL_EDIT_FILE: &str = "edit_file";

#[derive(Debug, Deserialize)]
struct EditFileArgs {
    patch: String,
}

#[derive(Debug, Clone)]
pub(crate) struct EditFileTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

#[derive(Debug)]
struct PlannedEdit {
    path: PathBuf,
    original: String,
    rewritten: String,
    added_lines: usize,
    removed_lines: usize,
}

impl Tool for EditFileTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_EDIT_FILE,
            description: "Apply a Codex-format context patch to one or more existing UTF-8 files. The patch must use *** Begin Patch, one or more *** Update File: path sections with @@ context chunks, and *** End Patch. Every removed/context line must match the current file exactly; if it does not, re-read the file and create a new patch. This tool never creates or deletes files.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": { "patch": { "type": "string", "minLength": 1 } },
                "required": ["patch"],
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: EditFileArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(error) => return json_error(format!("invalid edit_file arguments: {error}")),
        };
        let plans = match plan_patch(self.workspace_path.as_deref(), &args.patch) {
            Ok(plans) => plans,
            Err(error) => return json_error(error),
        };
        for plan in &plans {
            if let Err(error) = fs::write(&plan.path, &plan.rewritten) {
                return json_error(format!("failed to write edited file '{}': {error}", plan.path.display()));
            }
        }
        let files: Vec<Value> = plans
            .iter()
            .map(|plan| serde_json::json!({
                "path": plan.path.display().to_string(),
                "added_lines": plan.added_lines,
                "removed_lines": plan.removed_lines,
            }))
            .collect();
        let diff = plans.iter().map(|plan| unified_diff(plan)).collect::<Vec<_>>().join("\n");
        success_json(serde_json::json!({ "ok": true, "files": files, "diff": diff }))
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<EditFileArgs>(arguments.clone())
            .ok()
            .and_then(|args| first_patch_path(&args.patch))
            .and_then(|path| resolve_tool_path(self.workspace_path.as_deref(), &path).ok())
            .map(|path| path_resource(None, &path.to_string_lossy(), true))
            .unwrap_or(ToolExecutionResource::Exclusive)
    }
}

fn first_patch_path(patch: &str) -> Option<String> {
    patch.lines().find_map(|line| line.strip_prefix("*** Update File: ").map(|path| path.trim().to_string()))
}

fn plan_patch(workspace_path: Option<&std::path::Path>, patch: &str) -> Result<Vec<PlannedEdit>, String> {
    let lines: Vec<&str> = patch.lines().collect();
    if lines.first().copied() != Some("*** Begin Patch") || lines.last().copied() != Some("*** End Patch") {
        return Err("patch must start with '*** Begin Patch' and end with '*** End Patch'".to_string());
    }
    let mut index = 1;
    let mut plans = Vec::new();
    let mut paths = HashSet::new();
    while index + 1 < lines.len() {
        let Some(path_text) = lines[index].strip_prefix("*** Update File: ") else {
            return Err(format!("expected '*** Update File: path' on patch line {}", index + 1));
        };
        let path_text = path_text.trim();
        if path_text.is_empty() || !paths.insert(path_text.to_string()) {
            return Err(format!("invalid or duplicate update path '{path_text}'"));
        }
        let path = resolve_tool_path(workspace_path, path_text).map_err(|error| error.to_string())?;
        let original = fs::read_to_string(&path).map_err(|error| format!("failed to read file '{}': {error}", path.display()))?;
        let trailing_newline = original.ends_with('\n');
        let mut file_lines: Vec<String> = original.lines().map(ToOwned::to_owned).collect();
        index += 1;
        let mut search_from = 0;
        let mut added_lines = 0;
        let mut removed_lines = 0;
        let mut saw_chunk = false;
        while index + 1 < lines.len() && !lines[index].starts_with("*** Update File: ") {
            if lines[index] != "@@" {
                return Err(format!("expected '@@' context marker on patch line {}", index + 1));
            }
            index += 1;
            let chunk_start = index;
            while index + 1 < lines.len() && lines[index] != "@@" && !lines[index].starts_with("*** Update File: ") {
                let line = lines[index];
                if !matches!(line.as_bytes().first(), Some(b' ') | Some(b'+') | Some(b'-')) {
                    return Err(format!("invalid patch line {}: every chunk line must start with space, +, or -", index + 1));
                }
                index += 1;
            }
            if index == chunk_start { return Err(format!("empty patch chunk for '{}'", path.display())); }
            let old: Vec<String> = lines[chunk_start..index].iter().filter_map(|line| matches!(line.as_bytes().first(), Some(b' ') | Some(b'-')).then(|| line[1..].to_string())).collect();
            let replacement: Vec<String> = lines[chunk_start..index].iter().filter_map(|line| matches!(line.as_bytes().first(), Some(b' ') | Some(b'+')).then(|| line[1..].to_string())).collect();
            let matches: Vec<usize> = file_lines.windows(old.len()).enumerate().filter_map(|(at, window)| (window == old.as_slice() && at >= search_from).then_some(at)).collect();
            if matches.len() != 1 {
                return Err(if matches.is_empty() { format!("failed to find expected context in '{}'", path.display()) } else { format!("patch context is ambiguous in '{}'; include more surrounding lines", path.display()) });
            }
            let at = matches[0];
            added_lines += lines[chunk_start..index].iter().filter(|line| line.starts_with('+')).count();
            removed_lines += lines[chunk_start..index].iter().filter(|line| line.starts_with('-')).count();
            file_lines.splice(at..at + old.len(), replacement);
            search_from = at + 1;
            saw_chunk = true;
        }
        if !saw_chunk { return Err(format!("update for '{}' has no chunks", path.display())); }
        let mut rewritten = file_lines.join("\n");
        if trailing_newline && !rewritten.is_empty() { rewritten.push('\n'); }
        plans.push(PlannedEdit { path, original, rewritten, added_lines, removed_lines });
    }
    if plans.is_empty() { return Err("patch must contain at least one Update File section".to_string()); }
    Ok(plans)
}

fn unified_diff(plan: &PlannedEdit) -> String {
    format!("--- {}\n+++ {}\n@@\n-{}\n+{}", plan.path.display(), plan.path.display(), plan.original, plan.rewritten)
}
