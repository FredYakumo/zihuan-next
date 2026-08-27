use std::sync::Arc;
use serde::Deserialize;
use serde_json::Value;
use zihuan_core::agent::tools::{Tool, ToolExecutionOutput, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::workspace::AskUserRequest;
use zihuan_core::llm::tooling::StaticFunctionToolSpec;
use super::shared::json_error;
pub(crate) const DEFAULT_TOOL_ASK_USER:&str="ask_user";
#[derive(Debug,Clone,Default)]pub(crate)struct AskUserTool;
#[derive(Debug,Deserialize)]struct AskUserArgs{question:String,#[serde(default)]details:Option<String>,#[serde(default)]placeholder:Option<String>}
impl Tool for AskUserTool{
 fn spec(&self)->Arc<dyn FunctionTool>{Arc::new(StaticFunctionToolSpec{name:DEFAULT_TOOL_ASK_USER,description:"Ask the dashboard user for missing details and pause until they reply",parameters:serde_json::json!({"type":"object","properties":{"question":{"type":"string"},"details":{"type":"string"},"placeholder":{"type":"string"}},"required":["question"]})})}
fn execute_with_outcome(&self,_:&str,a:&Value)->ToolExecutionOutput{let args:AskUserArgs=match serde_json::from_value(a.clone()){Ok(v)=>v,Err(e)=>return ToolExecutionOutput::text(json_error(format!("invalid ask_user arguments: {e}")))};let question=args.question.trim().to_string();if question.is_empty(){return ToolExecutionOutput::text(json_error("question must not be empty"))}let request=AskUserRequest{question:question.clone(),details:args.details.map(|v|v.trim().to_string()).filter(|v|!v.is_empty()),placeholder:args.placeholder.map(|v|v.trim().to_string()).filter(|v|!v.is_empty()),command_confirmation:None};ToolExecutionOutput::ask_user(serde_json::json!({"ok":true,"awaiting_user_input":true,"question":question}).to_string(),request)}
 fn execute(&self,_:&str,_:&Value)->String{unreachable!("ask_user uses execute_with_outcome")}
 fn execution_resource(&self,_:&Value)->ToolExecutionResource{ToolExecutionResource::Exclusive}
}
