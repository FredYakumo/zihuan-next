use serde_json::{json, Value};

use crate::error::Result;

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

/// A static [`FunctionTool`] implementation backed by compile-time constants.
///
/// Useful for simple built-in tools where the name, description, and parameter
/// schema are known at compile time and no argument forwarding is required.
#[derive(Debug)]
pub struct StaticFunctionToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

impl FunctionTool for StaticFunctionToolSpec {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    fn call(&self, _arguments: Value) -> Result<Value> {
        Ok(Value::Null)
    }
}
