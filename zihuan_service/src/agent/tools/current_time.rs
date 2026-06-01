use std::sync::Arc;

use chrono::Local;
use serde_json::Value;

use zihuan_agent::brain::BrainTool;
use zihuan_core::llm::tooling::{FunctionTool, StaticFunctionToolSpec};

pub(crate) struct CurrentTimeBrainTool;

impl BrainTool for CurrentTimeBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "get_current_time",
            description: "获取当前本地时间。只有在确实需要知道现在几点、今天日期或生成带时间语义的回答时才调用。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, _arguments: &Value) -> String {
        serde_json::json!({
            "current_time": Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
        })
        .to_string()
    }
}
