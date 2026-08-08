use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::brain::{BrainTool, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;

use zihuan_core::llm::tooling::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json, wildcard_matches};

pub const DEFAULT_TOOL_FIND_FILES: &str = "find_files";

#[derive(Debug, Clone, Deserialize)]
struct FindFilesArgs {
    path: String,
    #[serde(default)] name: Option<String>,
    #[serde(default)] glob: Option<String>,
    #[serde(default, rename = "type")] kind: Option<String>,
    #[serde(default)] exclude: Vec<String>,
    #[serde(default)] max_results: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct FindFilesBrainTool { pub(crate) workspace_path: Option<PathBuf> }

impl BrainTool for FindFilesBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_FIND_FILES,
            description: "Find files or directories by name or glob, with directory exclusions",
            parameters: serde_json::json!({"type":"object","properties":{
                "path":{"type":"string"}, "name":{"type":"string"}, "glob":{"type":"string"},
                "type":{"type":"string","enum":["file","dir","all"]},
                "exclude":{"type":"array","items":{"type":"string"}},
                "max_results":{"type":"integer","minimum":1}
            },"required":["path"]}),
        })
    }

    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: FindFilesArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid find_files arguments: {err}")),
        };
        if args.name.is_none() && args.glob.is_none() { return json_error("name or glob is required"); }
        let kind = args.kind.as_deref().unwrap_or("all");
        if !matches!(kind, "file" | "dir" | "all") { return json_error("type must be file, dir, or all"); }
        let root = match resolve_tool_path(self.workspace_path.as_deref(), &args.path) { Ok(path) => path, Err(err) => return json_error(err.to_string()) };
        if !root.is_dir() { return json_error(format!("directory does not exist: {}", root.display())); }
        let max_results = args.max_results.unwrap_or(200);
        if max_results == 0 { return json_error("max_results must be greater than zero"); }
        let mut pending = VecDeque::from([root.clone()]);
        let mut results = Vec::new();
        let mut truncated = false;
        while let Some(directory) = pending.pop_front() {
            let mut children: Vec<_> = match fs::read_dir(&directory) {
                Ok(entries) => entries.filter_map(Result::ok).collect(),
                Err(err) => return json_error(format!("failed to list directory '{}': {err}", directory.display())),
            };
            children.sort_by_key(|entry| entry.file_name());
            for entry in children {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') { continue; }
                let child = entry.path();
                let file_type = match entry.file_type() { Ok(value) => value, Err(_) => continue };
                let is_dir = file_type.is_dir();
                    let relative = child.strip_prefix(&root).unwrap_or(&child).to_string_lossy().replace('\\', "/");
                    let excluded = args.exclude.iter().any(|pattern| {
                        let pattern = pattern.trim_start_matches('!').trim_matches('/').replace('\\', "/");
                        let matched = wildcard_matches(&pattern, &name) || wildcard_matches(&pattern, &relative);
                        matched || relative.starts_with(&(pattern + "/"))
                    });
                if is_dir && !excluded { pending.push_back(child.clone()); }
                if excluded { continue; }
                let type_match = kind == "all" || (kind == "dir" && is_dir) || (kind == "file" && !is_dir);
                let name_match = args.name.as_deref().is_none_or(|pattern| wildcard_matches(pattern, &name));
                let glob_match = args.glob.as_deref().is_none_or(|pattern| wildcard_matches(pattern, &relative) || wildcard_matches(pattern, &name));
                if type_match && name_match && glob_match {
                    results.push(serde_json::json!({"name":name,"path":child.display().to_string(),"type":if is_dir {"directory"} else {"file"}}));
                    if results.len() >= max_results { truncated = true; break; }
                }
            }
            if truncated { break; }
        }
        success_json(serde_json::json!({"ok":true,"path":root.display().to_string(),"matches":results,"total_matches":results.len(),"truncated":truncated}))
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<FindFilesArgs>(arguments.clone()).map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false)).unwrap_or(ToolExecutionResource::Exclusive)
    }
}