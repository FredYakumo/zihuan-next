use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::tool_calling::{Tool, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::tooling::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json};
pub(crate) const DEFAULT_TOOL_CREATE_FILE: &str = "create_file";
#[derive(Debug, Deserialize)] struct CreateFileArgs { path: String, content: String, #[serde(default)] overwrite: bool }
#[derive(Debug, Clone)] pub(crate) struct CreateFileTool { pub(crate) workspace_path: Option<PathBuf> }
impl Tool for CreateFileTool {
 fn spec(&self)->Arc<dyn FunctionTool>{Arc::new(StaticFunctionToolSpec{name:DEFAULT_TOOL_CREATE_FILE,description:"Create a file using the provided content",parameters:serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"},"overwrite":{"type":"boolean"}},"required":["path","content"]})})}
 fn execute(&self,_:&str,arguments:&Value)->String{let args:CreateFileArgs=match serde_json::from_value(arguments.clone()){Ok(v)=>v,Err(e)=>return json_error(format!("invalid create_file arguments: {e}"))};let path=match resolve_tool_path(self.workspace_path.as_deref(),&args.path){Ok(v)=>v,Err(e)=>return json_error(e.to_string())};if path.exists()&&!args.overwrite{return json_error(format!("file already exists: {}",path.display()))}if let Some(parent)=path.parent(){if let Err(e)=fs::create_dir_all(parent){return json_error(format!("failed to create parent directory '{}': {e}",parent.display()))}}if let Err(e)=fs::write(&path,&args.content){return json_error(format!("failed to write file '{}': {e}",path.display()))}success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"line_count":args.content.lines().count()}))}
 fn execution_resource(&self,a:&Value)->ToolExecutionResource{serde_json::from_value::<CreateFileArgs>(a.clone()).map(|v|path_resource(self.workspace_path.as_deref(),&v.path,true)).unwrap_or(ToolExecutionResource::Exclusive)}
}
