use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::tool_calling::{Tool, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;

use zihuan_core::llm::tooling::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json};

pub(crate) const DEFAULT_TOOL_MOVE_FILE: &str = "move_file";

#[derive(Debug, Clone, Deserialize)]
struct MoveFileArgs { src: String, dest: String, #[serde(default)] overwrite: bool }

#[derive(Debug, Clone)]
pub(crate) struct MoveFileTool { pub(crate) workspace_path: Option<PathBuf> }

impl Tool for MoveFileTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec { name: DEFAULT_TOOL_MOVE_FILE, description: "Move or rename a file or directory", parameters: serde_json::json!({"type":"object","properties":{"src":{"type":"string"},"dest":{"type":"string"},"overwrite":{"type":"boolean"}},"required":["src","dest"]}) })
    }

    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: MoveFileArgs = match serde_json::from_value(arguments.clone()) { Ok(value) => value, Err(err) => return json_error(format!("invalid move_file arguments: {err}")) };
        let src = match resolve_tool_path(self.workspace_path.as_deref(), &args.src) { Ok(path) => path, Err(err) => return json_error(err.to_string()) };
        let dest = match resolve_tool_path(self.workspace_path.as_deref(), &args.dest) { Ok(path) => path, Err(err) => return json_error(err.to_string()) };
        if !src.exists() { return json_error(format!("source does not exist: {}", src.display())); }
        if dest.exists() {
            if !args.overwrite { return json_error(format!("destination already exists: {}", dest.display())); }
            let removal = if dest.is_dir() { fs::remove_dir_all(&dest) } else { fs::remove_file(&dest) };
            if let Err(err) = removal { return json_error(format!("failed to replace destination '{}': {err}", dest.display())); }
        }
        if let Some(parent) = dest.parent() { if let Err(err) = fs::create_dir_all(parent) { return json_error(format!("failed to create destination parent '{}': {err}", parent.display())); } }
        if let Err(err) = fs::rename(&src, &dest) { return json_error(format!("failed to move '{}' to '{}': {err}", src.display(), dest.display())); }
        success_json(serde_json::json!({"ok":true,"src":src.display().to_string(),"dest":dest.display().to_string(),"type":if dest.is_dir(){"directory"}else{"file"},"overwritten":args.overwrite}))
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<MoveFileArgs>(arguments.clone()).map(|args| path_resource(self.workspace_path.as_deref(), &args.dest, true)).unwrap_or(ToolExecutionResource::Exclusive)
    }
}
