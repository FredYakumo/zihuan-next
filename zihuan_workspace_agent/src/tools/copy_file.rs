use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::brain::{BrainTool, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;

use zihuan_core::llm::tooling::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json};

pub(crate) const DEFAULT_TOOL_COPY_FILE: &str = "copy_file";

#[derive(Debug, Clone, Deserialize)]
struct CopyFileArgs {
    src: String,
    dest: String,
    #[serde(default)] overwrite: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct CopyFileBrainTool { pub(crate) workspace_path: Option<PathBuf> }

impl BrainTool for CopyFileBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec { name: DEFAULT_TOOL_COPY_FILE, description: "Copy a file or directory", parameters: serde_json::json!({"type":"object","properties":{"src":{"type":"string"},"dest":{"type":"string"},"overwrite":{"type":"boolean"}},"required":["src","dest"]}) })
    }

    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: CopyFileArgs = match serde_json::from_value(arguments.clone()) { Ok(value) => value, Err(err) => return json_error(format!("invalid copy_file arguments: {err}")) };
        let src = match resolve_tool_path(self.workspace_path.as_deref(), &args.src) { Ok(path) => path, Err(err) => return json_error(err.to_string()) };
        let dest = match resolve_tool_path(self.workspace_path.as_deref(), &args.dest) { Ok(path) => path, Err(err) => return json_error(err.to_string()) };
        if !src.exists() { return json_error(format!("source does not exist: {}", src.display())); }
        if dest.exists() && !args.overwrite { return json_error(format!("destination already exists: {}", dest.display())); }
        if let Some(parent) = dest.parent() { if let Err(err) = fs::create_dir_all(parent) { return json_error(format!("failed to create destination parent '{}': {err}", parent.display())); } }
        let result = if src.is_dir() {
            if dest.exists() && dest.is_dir() && args.overwrite { copy_dir_contents(&src, &dest) } else { fs::create_dir(&dest).map(|_| ()) }
        } else {
            fs::copy(&src, &dest).map(|_| ())
        };
        if let Err(err) = result { return json_error(format!("failed to copy '{}' to '{}': {err}", src.display(), dest.display())); }
        success_json(serde_json::json!({"ok":true,"src":src.display().to_string(),"dest":dest.display().to_string(),"type":if src.is_dir(){"directory"}else{"file"},"overwritten":args.overwrite && dest.exists()}))
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<CopyFileArgs>(arguments.clone()).map(|args| path_resource(self.workspace_path.as_deref(), &args.dest, true)).unwrap_or(ToolExecutionResource::Exclusive)
    }
}

fn copy_dir_contents(src: &std::path::Path, dest: &std::path::Path) -> Result<(), std::io::Error> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let source = entry.path();
        let target = dest.join(entry.file_name());
        if source.is_dir() {
            fs::create_dir_all(&target)?;
            copy_dir_contents(&source, &target)?;
        } else {
            fs::copy(source, target)?;
        }
    }
    Ok(())
}
