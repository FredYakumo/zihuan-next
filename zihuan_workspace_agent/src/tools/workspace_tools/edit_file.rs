use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::tools::{Tool, ToolExecutionResource};
use zihuan_core::llm::tooling::{FunctionTool, StaticFunctionToolSpec};

use super::shared::{json_error, path_resource, resolve_tool_path, success_json};

pub(crate) const DEFAULT_TOOL_EDIT_FILE: &str = "edit_file";

#[derive(Debug, Deserialize)]
struct EditFileArgs {
    path: String,
    start_line: usize,
    end_line: usize,
    replacement_lines: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct EditFileTool {
    pub(crate) workspace_path: Option<PathBuf>,
}

impl Tool for EditFileTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_EDIT_FILE,
            description: "Replace or delete one existing 1-based inclusive line range. Supply path, start_line, end_line, and replacement_lines at the top level. Call once per edit.",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 },
                    "replacement_lines": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["path", "start_line", "end_line", "replacement_lines"]
            }),
        })
    }

    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: EditFileArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(error) => return json_error(format!("invalid edit_file arguments: {error}")),
        };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) {
            Ok(path) => path,
            Err(error) => return json_error(error.to_string()),
        };
        let original = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) => return json_error(format!("failed to read file '{}': {error}", path.display())),
        };
        let trailing_newline = original.ends_with('\n');
        let mut lines: Vec<String> = original.lines().map(ToOwned::to_owned).collect();

        if args.start_line == 0 || args.end_line == 0 || args.start_line > args.end_line {
            return json_error(format!(
                "invalid line range: start_line={} end_line={}",
                args.start_line, args.end_line
            ));
        }
        if args.end_line > lines.len() {
            return json_error(format!(
                "line range [{}-{}] is out of bounds for file '{}' with {} lines",
                args.start_line,
                args.end_line,
                path.display(),
                lines.len()
            ));
        }

        lines.splice(args.start_line - 1..args.end_line, args.replacement_lines);
        let mut rewritten = lines.join("\n");
        if trailing_newline && !rewritten.is_empty() {
            rewritten.push('\n');
        }
        if let Err(error) = fs::write(&path, rewritten) {
            return json_error(format!("failed to write edited file '{}': {error}", path.display()));
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
