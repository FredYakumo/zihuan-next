use serde::Serialize;
use serde_json::{Value, json};

pub trait FunctionTool: Send + Sync {
    fn name(&self) -> & str;
    fn description(&self) -> & str;

    /// JSON Schema-like parameters definition.
    ///
    /// Example:
    /// {"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}
    fn parameters(&self) -> Value;

    fn get_json(&self) -> Value {
        json!({
            "name": self.name(),
            "description": self.description(),
            "parameters": self.parameters(),
        })
    }

    /// Tool execute function
    fn call(&self, arguments: Value) -> Result<Value, String>;
}

pub struct ToolCallsFuncSpec {
    pub name: String,
    pub arguments: Value
}

pub struct ToolCalls {
    pub id: String,
    pub type_name: String,
    pub function: ToolCallsFuncSpec,
}

