use std::sync::Arc;

use serde_json::Value;

use zihuan_core::agent::tools::{Tool, ToolRunDuration};
use zihuan_core::error::Result;
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::tool_subgraph::ToolSubgraphRunner;

pub(crate) struct EditableQqAgentTool {
    pub(crate) runner: ToolSubgraphRunner,
}

impl Tool for EditableQqAgentTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.runner.spec()
    }

    fn run_duration(&self) -> ToolRunDuration {
        self.runner.definition.run_duration
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> Result<String> {
        self.runner.execute_result(call_content, arguments)
    }
}
