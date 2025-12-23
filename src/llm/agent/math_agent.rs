use serde_json::json;

use crate::bot_adapter::models::MessageEvent;
use crate::bot_adapter::models::message::{Message as MsgEnum, PlainTextMessage};
use crate::llm::agent::Agent;
use crate::llm::function_tools::{FunctionTool, MathTool};
use crate::llm::{Message, MessageRole};

pub struct MathAgent {
    tool: MathTool,
}

impl MathAgent {
    pub fn new() -> Self { Self { tool: MathTool::new() } }

    fn parse_simple_expr(text: &str) -> Option<(f64, &'static str, f64)> {
        let tokens: Vec<&str> = text.split_whitespace().collect();
        if tokens.len() >= 3 {
            if let (Ok(a), Ok(b)) = (tokens[0].parse::<f64>(), tokens[2].parse::<f64>()) {
                let op = match tokens[1] {
                    "+" | "加" => Some("add"),
                    "-" | "减" => Some("sub"),
                    "*" | "×" | "乘" => Some("mul"),
                    "/" | "÷" | "除" => Some("div"),
                    _ => None,
                };
                if let Some(op) = op { return Some((a, op, b)); }
            }
        }
        None
    }

    fn aggregate_text(event: &MessageEvent) -> String {
        let mut text = String::new();
        for m in &event.message_list {
            if let MsgEnum::PlainText(PlainTextMessage { text: t }) = m {
                if !text.is_empty() { text.push(' '); }
                text.push_str(t);
            }
        }
        text
    }
}

impl Agent for MathAgent {
    type Output = Message;

    fn name(&self) -> &'static str { "math_agent" }

    fn on_event(&self, event: &MessageEvent) -> Self::Output {
        let text = Self::aggregate_text(event);
        self.on_agent_input(Message {
            role: MessageRole::User,
            content: Some(text),
            tool_calls: Vec::new(),
        })
    }

    fn on_agent_input(&self, input: Message) -> Self::Output {
        let input_json = if let Some(content) = input.content {
            serde_json::json!({"text": content})
        } else {
            serde_json::json!({})
        };
        let content = if input_json.get("a").is_some() && input_json.get("b").is_some() {
            match self.tool.call(input_json) {
                Ok(v) => v.to_string(),
                Err(e) => format!("Math error: {e}"),
            }
        } else if let Some(text) = input_json.get("text").and_then(|v| v.as_str()) {
            if let Some((a, op, b)) = Self::parse_simple_expr(text) {
                let args = json!({"a": a, "b": b, "op": op});
                match self.tool.call(args) {
                    Ok(v) => v.to_string(),
                    Err(e) => format!("Math error: {e}"),
                }
            } else {
                let nums: Vec<f64> = text.split(|c: char| !c.is_ascii_digit() && c != '.')
                    .filter_map(|s| if s.is_empty() { None } else { s.parse::<f64>().ok() })
                    .collect();
                if nums.is_empty() {
                    "No arithmetic expression recognized.".to_string()
                } else {
                    let sum: f64 = nums.iter().sum();
                    json!({"op":"sum","terms":nums,"result":sum}).to_string()
                }
            }
        } else {
            "No math input provided.".to_string()
        };

        Message { role: MessageRole::Tool, content: Some(content), tool_calls: Vec::new() }
    }
}
