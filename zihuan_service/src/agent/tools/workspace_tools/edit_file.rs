use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::brain::{BrainTool, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;
use super::super::common::StaticFunctionToolSpec;
use super::shared::{json_error, path_resource, resolve_tool_path, success_json};
pub(crate) const DEFAULT_TOOL_EDIT_FILE:&str="edit_file";
#[derive(Debug,Deserialize)]struct EditFileArgs{path:String,edits:Vec<LineEditSpec>}
#[derive(Debug,Clone,Deserialize)]struct LineEditSpec{start_line:usize,end_line:usize,replacement_lines:Vec<String>}
#[derive(Debug,Clone)]pub(crate)struct EditFileBrainTool{pub(crate)workspace_path:Option<PathBuf>}
impl BrainTool for EditFileBrainTool{
 fn spec(&self)->Arc<dyn FunctionTool>{Arc::new(StaticFunctionToolSpec{name:DEFAULT_TOOL_EDIT_FILE,description:"Replace or delete existing file lines using 1-based inclusive line ranges",parameters:serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"edits":{"type":"array","items":{"type":"object","properties":{"start_line":{"type":"integer","minimum":1},"end_line":{"type":"integer","minimum":1},"replacement_lines":{"type":"array","items":{"type":"string"}}},"required":["start_line","end_line","replacement_lines"]}}},"required":["path","edits"]})})}
 fn execute(&self,_:&str,a:&Value)->String{let args:EditFileArgs=match serde_json::from_value(a.clone()){Ok(v)=>v,Err(e)=>return json_error(format!("invalid edit_file arguments: {e}"))};let path=match resolve_tool_path(self.workspace_path.as_deref(),&args.path){Ok(v)=>v,Err(e)=>return json_error(e.to_string())};let original=match fs::read_to_string(&path){Ok(v)=>v,Err(e)=>return json_error(format!("failed to read file '{}': {e}",path.display()))};let trailing=original.ends_with('\n');let mut lines:Vec<String>=original.lines().map(ToOwned::to_owned).collect();let mut edits=args.edits;edits.sort_by(|a,b|b.start_line.cmp(&a.start_line).then_with(||b.end_line.cmp(&a.end_line)));for edit in edits{if edit.start_line==0||edit.end_line==0||edit.start_line>edit.end_line{return json_error(format!("invalid line range: start_line={} end_line={}",edit.start_line,edit.end_line))}if edit.end_line>lines.len(){return json_error(format!("line range [{}-{}] is out of bounds for file '{}' with {} lines",edit.start_line,edit.end_line,path.display(),lines.len()))}lines.splice(edit.start_line-1..edit.end_line,edit.replacement_lines);}let mut rewritten=lines.join("\n");if trailing&&!rewritten.is_empty(){rewritten.push('\n')}if let Err(e)=fs::write(&path,rewritten){return json_error(format!("failed to write edited file '{}': {e}",path.display()))}success_json(serde_json::json!({"ok":true,"path":path.display().to_string(),"line_count":lines.len()}))}
 fn execution_resource(&self,a:&Value)->ToolExecutionResource{serde_json::from_value::<EditFileArgs>(a.clone()).map(|v|path_resource(self.workspace_path.as_deref(),&v.path,true)).unwrap_or(ToolExecutionResource::Exclusive)}
}
