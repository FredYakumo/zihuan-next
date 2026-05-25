use std::sync::Arc;

use serde_json::Value;

use crate::nodes::tool_subgraph::ToolSubgraphRunner;
use zihuan_agent::brain::{BrainTool, ToolRunDuration};
use zihuan_core::llm::tooling::FunctionTool;

pub(crate) struct EditableQqAgentTool {
    pub(crate) runner: ToolSubgraphRunner,
}

impl BrainTool for EditableQqAgentTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.runner.spec()
    }

    fn run_duration(&self) -> ToolRunDuration {
        self.runner.definition.run_duration
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.runner.execute_to_string(call_content, arguments)
    }
}
