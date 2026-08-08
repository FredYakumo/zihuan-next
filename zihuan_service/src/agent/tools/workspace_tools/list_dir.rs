use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use serde::Deserialize;
use serde_json::Value;
use zihuan_agent::brain::{BrainTool, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;
use super::super::common::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json, DEFAULT_MAX_ENTRIES};

pub(crate) const DEFAULT_TOOL_LIST_DIR: &str = "list_dir";
#[derive(Debug, Clone, Deserialize)] struct ListDirArgs { path: String, #[serde(default)] recursive: bool, #[serde(default)] include_hidden: bool, #[serde(default)] max_entries: Option<usize> }
#[derive(Debug, Clone)] pub(crate) struct ListDirBrainTool { pub(crate) workspace_path: Option<PathBuf> }
impl BrainTool for ListDirBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> { Arc::new(StaticFunctionToolSpec { name: DEFAULT_TOOL_LIST_DIR, description: "List files and directories in a workspace path, optionally recursively", parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"recursive":{"type":"boolean"},"include_hidden":{"type":"boolean"},"max_entries":{"type":"integer","minimum":1}},"required":["path"]}) }) }
    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: ListDirArgs = match serde_json::from_value(arguments.clone()) { Ok(value) => value, Err(err) => return json_error(format!("invalid list_dir arguments: {err}")) };
        let path = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) { Ok(path) => path, Err(err) => return json_error(err.to_string()) };
        if !path.is_dir() { return json_error(format!("directory does not exist: {}", path.display())); }
        let max_entries = args.max_entries.unwrap_or(DEFAULT_MAX_ENTRIES); if max_entries == 0 { return json_error("max_entries must be greater than zero"); }
        let mut pending = VecDeque::from([path.clone()]); let mut entries = Vec::new();
        while let Some(directory) = pending.pop_front() {
            let mut children = Vec::new();
            for entry in match fs::read_dir(&directory) { Ok(value) => value, Err(err) => return json_error(format!("failed to list directory '{}': {err}", directory.display())) } {
                let entry = match entry { Ok(value) => value, Err(err) => return json_error(format!("failed to inspect directory '{}': {err}", directory.display())) };
                let name = entry.file_name().to_string_lossy().to_string(); if !args.include_hidden && name.starts_with('.') { continue; }
                children.push((name, entry.path(), entry.file_type()));
            }
            children.sort_by(|left, right| left.0.cmp(&right.0));
            for (name, child_path, file_type) in children {
                let file_type = match file_type { Ok(value) => value, Err(err) => return json_error(format!("failed to inspect '{}': {err}", child_path.display())) };
                entries.push(serde_json::json!({"name":name,"path":child_path.display().to_string(),"type":if file_type.is_dir() {"directory"} else {"file"}}));
                if args.recursive && file_type.is_dir() { pending.push_back(child_path); } if entries.len() >= max_entries { break; }
            }
            if entries.len() >= max_entries { break; }
        }
        success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"recursive":args.recursive,"entries":entries,"truncated":entries.len() >= max_entries}))
    }
    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource { serde_json::from_value::<ListDirArgs>(arguments.clone()).map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false)).unwrap_or(ToolExecutionResource::Exclusive) }
}
