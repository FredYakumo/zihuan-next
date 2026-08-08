use std::path::PathBuf;
use std::sync::Arc;
use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use zihuan_agent::brain::{BrainTool, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::runtime::block_async;
use super::super::common::StaticFunctionToolSpec;
use super::shared::{json_error, resolve_tool_path, success_json};
pub(crate) const DEFAULT_TOOL_EXEC_CMD:&str="exec_cmd";
#[derive(Debug,Deserialize)]struct ExecCmdArgs{command:String,#[serde(default)]cwd:Option<String>,#[serde(default)]timeout_secs:Option<u64>}
#[derive(Debug,Clone)]pub(crate)struct ExecCmdBrainTool{pub(crate)workspace_path:Option<PathBuf>}
impl BrainTool for ExecCmdBrainTool{
 fn spec(&self)->Arc<dyn FunctionTool>{Arc::new(StaticFunctionToolSpec{name:DEFAULT_TOOL_EXEC_CMD,description:"Execute a shell command using PowerShell on Windows or Bash on other systems",parameters:serde_json::json!({"type":"object","properties":{"command":{"type":"string"},"cwd":{"type":"string"},"timeout_secs":{"type":"integer","minimum":1}},"required":["command"]})})}
 fn execute(&self,_:&str,a:&Value)->String{let args:ExecCmdArgs=match serde_json::from_value(a.clone()){Ok(v)=>v,Err(e)=>return json_error(format!("invalid exec_cmd arguments: {e}"))};let cwd=if let Some(raw)=args.cwd.as_deref(){match resolve_tool_path(self.workspace_path.as_deref(),raw){Ok(v)=>Some(v),Err(e)=>return json_error(e.to_string())}}else{self.workspace_path.clone()};let secs=args.timeout_secs.unwrap_or(30);let command_cwd=cwd.clone();let result=block_async(async move{let mut command=if cfg!(windows){let mut c=Command::new("powershell");c.args(["-NoProfile","-Command",&args.command]);c}else{let mut c=Command::new("bash");c.args(["-lc",&args.command]);c};if let Some(path)=command_cwd.as_ref(){command.current_dir(path);}timeout(Duration::from_secs(secs),command.output()).await});match result{Ok(Ok(output))=>success_json(serde_json::json!({"ok":output.status.success(),"status":output.status.code(),"stdout":String::from_utf8_lossy(&output.stdout).to_string(),"stderr":String::from_utf8_lossy(&output.stderr).to_string(),"shell":if cfg!(windows){"powershell"}else{"bash"},"cwd":cwd.as_ref().map(|p|p.display().to_string())})),Ok(Err(e))=>json_error(format!("failed to execute command: {e}")),Err(_)=>json_error(format!("command timed out after {secs}s"))}}
 fn execution_resource(&self,_:&Value)->ToolExecutionResource{ToolExecutionResource::Exclusive}
}
