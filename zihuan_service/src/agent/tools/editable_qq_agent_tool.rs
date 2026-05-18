use std::sync::Arc;

use serde_json::Value;

use crate::nodes::tool_subgraph::ToolSubgraphRunner;
use zihuan_agent::brain::BrainTool;
use zihuan_core::llm::tooling::FunctionTool;

use super::common::send_editable_tool_progress_notification;

pub(crate) struct EditableQqAgentTool {
    pub(crate) runner: ToolSubgraphRunner,
}

impl BrainTool for EditableQqAgentTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.runner.spec()
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        send_editable_tool_progress_notification(&self.runner.shared_runtime_values, call_content);
        self.runner.execute_to_string(call_content, arguments)
    }
}
