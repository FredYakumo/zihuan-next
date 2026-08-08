use std::path::PathBuf;
use std::sync::Arc;
use serde_json::Value;
use zihuan_agent::brain::{BrainTool, ToolExecutionResource};
use zihuan_core::llm::tooling::FunctionTool;
use super::super::common::StaticFunctionToolSpec;
use super::shared::{execute_search, json_error, path_resource, SearchArgs};
pub(crate) const DEFAULT_TOOL_RG: &str = "rg";
#[derive(Debug, Clone)] pub(crate) struct RgBrainTool { pub(crate) workspace_path: Option<PathBuf> }
impl BrainTool for RgBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> { Arc::new(StaticFunctionToolSpec { name: DEFAULT_TOOL_RG, description: "Search workspace text files recursively using a regular expression", parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"pattern":{"type":"string"},"glob":{"type":"string"},"max_results":{"type":"integer","minimum":1},"context_lines":{"type":"integer","minimum":0},"case_sensitive":{"type":"boolean"}},"required":["path","pattern"]}) }) }
    fn execute(&self, _: &str, arguments: &Value) -> String { let args: SearchArgs = match serde_json::from_value(arguments.clone()) { Ok(value) => value, Err(err) => return json_error(format!("invalid rg arguments: {err}")) }; execute_search(args, self.workspace_path.as_deref(), true) }
    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource { serde_json::from_value::<SearchArgs>(arguments.clone()).map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false)).unwrap_or(ToolExecutionResource::Exclusive) }
}
