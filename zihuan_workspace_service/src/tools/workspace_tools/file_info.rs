use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::tools::{Tool, ToolExecutionResource};
use zihuan_core::model_inference::llm::tooling::FunctionTool;

use zihuan_core::model_inference::llm::tooling::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json};

pub(crate) const DEFAULT_TOOL_FILE_INFO: &str = "file_info";

#[derive(Debug, Clone, Deserialize)] struct FileInfoArgs { path: String }
#[derive(Debug, Clone)] pub(crate) struct FileInfoTool { pub(crate) workspace_path: Option<PathBuf> }

impl Tool for FileInfoTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec { name: DEFAULT_TOOL_FILE_INFO, description: "Return file, directory, and symlink metadata", parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}) })
    }

    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: FileInfoArgs = match serde_json::from_value(arguments.clone()) { Ok(value) => value, Err(err) => return json_error(format!("invalid file_info arguments: {err}")) };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) { Ok(path) => path, Err(err) => return json_error(err.to_string()) };
        let symlink = fs::symlink_metadata(&path).ok().is_some_and(|metadata| metadata.file_type().is_symlink());
        let metadata = match fs::metadata(&path) { Ok(value) => value, Err(err) => return json_error(format!("failed to inspect '{}': {err}", path.display())) };
        let modified = metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|duration| duration.as_secs());
        if metadata.is_dir() {
            let mut entry_count = 0usize;
            let mut extensions = serde_json::Map::new();
            let entries = match fs::read_dir(&path) { Ok(value) => value, Err(err) => return json_error(format!("failed to list directory '{}': {err}", path.display())) };
            for entry in entries.flatten() {
                entry_count += 1;
                if entry.file_type().map(|kind| kind.is_file()).unwrap_or(false) {
                    let extension = entry.path().extension().and_then(|value| value.to_str()).unwrap_or("<none>").to_string();
                    let count = extensions.get(&extension).and_then(Value::as_u64).unwrap_or(0) + 1;
                    extensions.insert(extension, Value::from(count));
                }
            }
            return success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"type":"directory","size_bytes":metadata.len(),"line_count":null,"modified_unix_secs":modified,"is_binary":false,"is_symlink":symlink,"entry_count":entry_count,"extensions":extensions}));
        }
        let bytes = match fs::read(&path) { Ok(value) => value, Err(err) => return json_error(format!("failed to read '{}': {err}", path.display())) };
        let is_binary = std::str::from_utf8(&bytes).is_err();
        let line_count = if is_binary { None } else { Some(String::from_utf8_lossy(&bytes).lines().count()) };
        success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"type":"file","size_bytes":metadata.len(),"line_count":line_count,"modified_unix_secs":modified,"is_binary":is_binary,"is_symlink":symlink}))
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource { serde_json::from_value::<FileInfoArgs>(arguments.clone()).map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false)).unwrap_or(ToolExecutionResource::Exclusive) }
}
