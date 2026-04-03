use zihuan_core::error::Result;
use serde_json::{json, Value};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct DummyTool;

    impl FunctionTool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }

        fn description(&self) -> &str {
            "dummy tool"
        }

        fn parameters(&self) -> Value {
            json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        }

        fn call(&self, _arguments: Value) -> Result<Value> {
            Ok(Value::Null)
        }
    }

    #[test]
    fn tool_json_includes_type_and_function_wrapper() {
        let tool = DummyTool;
        let v = tool.get_json();

        assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("function"));
        assert_eq!(
            v.get("function")
                .and_then(|f| f.get("name"))
                .and_then(|x| x.as_str()),
            Some("dummy")
        );
    }
}
