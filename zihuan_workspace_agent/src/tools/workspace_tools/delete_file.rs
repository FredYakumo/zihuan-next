use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::tools::{Tool, ToolExecutionResource};
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::model_inference::llm::tooling::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json};
pub(crate) const DEFAULT_TOOL_DELETE_FILE:&str="delete_file";
#[derive(Debug,Deserialize)]struct DeleteFileArgs{path:String,#[serde(default)]recursive:bool,#[serde(default)]missing_ok:bool}
#[derive(Debug,Clone)]pub(crate)struct DeleteFileTool{pub(crate)workspace_path:Option<PathBuf>}
impl Tool for DeleteFileTool{
 fn spec(&self)->Arc<dyn FunctionTool>{Arc::new(StaticFunctionToolSpec{name:DEFAULT_TOOL_DELETE_FILE,description:"Delete a file or directory from disk",parameters:serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"recursive":{"type":"boolean"},"missing_ok":{"type":"boolean"}},"required":["path"]})})}
 fn execute(&self,_:&str,a:&Value)->String{let args:DeleteFileArgs=match serde_json::from_value(a.clone()){Ok(v)=>v,Err(e)=>return json_error(format!("invalid delete_file arguments: {e}"))};let path=match resolve_tool_path(self.workspace_path.as_deref(),&args.path){Ok(v)=>v,Err(e)=>return json_error(e.to_string())};if !path.exists(){if args.missing_ok{return success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"deleted":false}))}return json_error(format!("path does not exist: {}",path.display()))}let line_count=if path.is_file(){fs::read_to_string(&path).ok().map(|s|s.lines().count())}else{None};let result=if path.is_dir(){if !args.recursive{return json_error("recursive=true is required to delete a directory")}fs::remove_dir_all(&path)}else{fs::remove_file(&path)};match result{Ok(())=>success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"deleted":true,"line_count":line_count})),Err(e)=>json_error(format!("failed to delete '{}': {e}",path.display()))}}
 fn execution_resource(&self,a:&Value)->ToolExecutionResource{serde_json::from_value::<DeleteFileArgs>(a.clone()).map(|v|path_resource(self.workspace_path.as_deref(),&v.path,true)).unwrap_or(ToolExecutionResource::Exclusive)}
}
