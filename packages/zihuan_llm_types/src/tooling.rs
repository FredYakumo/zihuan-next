use serde_json::{json, Value};
use zihuan_core::error::Result;

pub trait FunctionTool: Send + Sync + std::fmt::Debug {
    fn name(&self) -> &str;
    fn description(&self) -> &str;

    fn parameters(&self) -> Value;

    fn get_json(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters(),
            }
        })
    }

    fn call(&self, arguments: Value) -> Result<Value>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCallsFuncSpec {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolCalls {
    pub id: String,
    pub type_name: String,
    pub function: ToolCallsFuncSpec,
}

