use serde_json::Value;

use crate::error::Result;
use crate::model_inference::llm::tooling::FunctionTool;

/// LLM-facing spec of one memory tool; each tool declares its own parameters inline.
#[derive(Debug)]
pub(super) struct MemoryFunctionToolSpec {
    name: &'static str,
    description: &'static str,
    parameters: Value,
}

impl MemoryFunctionToolSpec {
    pub(super) fn new(name: &'static str, description: &'static str, parameters: Value) -> Self {
        Self { name, description, parameters }
    }
}

impl FunctionTool for MemoryFunctionToolSpec {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn parameters(&self) -> Value {
        self.parameters.clone()
    }
    fn call(&self, arguments: Value) -> Result<Value> {
        Ok(arguments)
    }
}
