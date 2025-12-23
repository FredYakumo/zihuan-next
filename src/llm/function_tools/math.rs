use super::FunctionTool;
use crate::error::Result;
use serde_json::{json, Value};

#[derive(Debug, Default)]
pub struct MathTool;

impl MathTool {
    pub fn new() -> Self { Self }
}

impl FunctionTool for MathTool {
    fn name(&self) -> &str { "math" }

    fn description(&self) -> &str {
        "Perform basic arithmetic on two numbers: add, sub, mul, div."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "a": { "type": "number", "description": "First operand" },
                "b": { "type": "number", "description": "Second operand" },
                "op": { "type": "string", "enum": ["add","sub","mul","div"], "description": "Operation to perform" }
            },
            "required": ["a","b"],
            "additionalProperties": false
        })
    }

    fn call(&self, arguments: Value) -> Result<Value> {
        let a = arguments.get("a").and_then(|v| v.as_f64()).ok_or_else(|| crate::string_error!("missing number 'a'"))?;
        let b = arguments.get("b").and_then(|v| v.as_f64()).ok_or_else(|| crate::string_error!("missing number 'b'"))?;
        let op = arguments.get("op").and_then(|v| v.as_str()).unwrap_or("add");

        let result = match op {
            "add" => a + b,
            "sub" => a - b,
            "mul" => a * b,
            "div" => {
                if b == 0.0 { return Err(crate::string_error!("division by zero")); }
                a / b
            },
            _ => return Err(crate::string_error!("unsupported op: {}", op)),
        };

        Ok(json!({
            "a": a,
            "b": b,
            "op": op,
            "result": result
        }))
    }
}

