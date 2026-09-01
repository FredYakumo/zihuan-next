use std::sync::Arc;

use serde_json::Value;

use zihuan_core::agent::tools::{Tool, ToolExecutionOutput, ToolRunDuration};
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

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.runner.execute_to_string(call_content, arguments)
    }

    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        ToolExecutionOutput::text(self.execute(call_content, arguments))
    }
}
