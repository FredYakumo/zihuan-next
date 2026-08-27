use std::sync::Arc;

use super::tools::{Tool, ToolExecutionOutput, ToolExecutionResource, ToolRunDuration};
use crate::llm::tooling::FunctionTool;
use serde_json::Value;

/// Adapts a shared `Arc<dyn Tool>` to the `Tool` value expected by the engine.
///
/// This wrapper only forwards the tool contract and execution behavior; it
/// does not add business logic. The shared ownership lets agents register the
/// same tool instance without taking ownership of it.
pub(crate) struct SharedTool(Arc<dyn Tool>);

impl SharedTool {
    pub(crate) fn new(tool: Arc<dyn Tool>) -> Self {
        Self(tool)
    }
}

impl Tool for SharedTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.0.spec()
    }

    fn execute(&self, content: &str, arguments: &Value) -> String {
        self.0.execute(content, arguments)
    }

    fn execute_with_outcome(&self, content: &str, arguments: &Value) -> ToolExecutionOutput {
        self.0.execute_with_outcome(content, arguments)
    }

    fn run_duration(&self) -> ToolRunDuration {
        self.0.run_duration()
    }

    fn execution_resource(&self, arguments: &Value) -> ToolExecutionResource {
        self.0.execution_resource(arguments)
    }
}
