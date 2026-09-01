use super::shared::{execute_search, json_error, path_resource, SearchArgs};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use zihuan_core::agent::tools::{Tool, ToolExecutionResource};
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::model_inference::llm::tooling::StaticFunctionToolSpec;
pub(crate) const DEFAULT_TOOL_GREP: &str = "grep";
#[derive(Debug, Clone)]
pub(crate) struct GrepTool {
    pub(crate) workspace_path: Option<PathBuf>,
}
impl Tool for GrepTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_GREP,
            description: "Search workspace text files recursively for a literal string",
            parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"},"pattern":{"type":"string"},"glob":{"type":"string"},"max_results":{"type":"integer","minimum":1},"context_lines":{"type":"integer","minimum":0},"context_before":{"type":"integer","minimum":0},"context_after":{"type":"integer","minimum":0},"case_sensitive":{"type":"boolean"}},"required":["path","pattern"]}),
        })
    }
    fn execute(&self, _: &str, arguments: &Value) -> String {
        let args: SearchArgs = match serde_json::from_value(arguments.clone()) {
            Ok(value) => value,
            Err(err) => return json_error(format!("invalid grep arguments: {err}")),
        };
        execute_search(args, self.workspace_path.as_deref(), false)
    }
    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        serde_json::from_value::<SearchArgs>(arguments.clone())
            .map(|args| path_resource(self.workspace_path.as_deref(), &args.path, false))
            .unwrap_or(ToolExecutionResource::Exclusive)
    }
}
