use serde_json::json;
use serde_json::Value;
use std::sync::Arc;

use crate::bot_adapter::models::MessageEvent;
use crate::bot_adapter::models::message::{Message as MsgEnum, PlainTextMessage};
use crate::llm::agent::Agent;
use crate::llm::function_tools::{FunctionTool, NaturalLanguageReplyTool};
use crate::llm::{LLMBase, Message, MessageRole};

pub struct NaturalLanguageReplyAgent {
    tool: NaturalLanguageReplyTool,
}

impl NaturalLanguageReplyAgent {
    pub fn new(llm: Arc<dyn LLMBase + Send + Sync>) -> Self {
        Self { tool: NaturalLanguageReplyTool::new(llm) }
    }

    fn aggregate_text(event: &MessageEvent) -> String {
        let mut parts = Vec::new();
        for m in &event.message_list {
            if let MsgEnum::PlainText(PlainTextMessage { text }) = m {
                parts.push(text.clone());
            }
        }
        parts.join(" ")
    }
}

impl Agent for NaturalLanguageReplyAgent {
    type Output = Message;

    fn name(&self) -> &'static str { "nl_reply_agent" }

    fn on_event(&self, event: &MessageEvent) -> Self::Output {
        let prompt = Self::aggregate_text(event);
        self.on_agent_input(json!({"prompt": prompt}))
    }

    fn on_agent_input(&self, input: Value) -> Self::Output {
        let prompt = input.get("prompt").cloned().unwrap_or_else(|| json!(""));
        let system = input.get("system").cloned().unwrap_or_else(|| json!("You are a helpful assistant. Reply clearly and concisely."));
        let args = json!({ "prompt": prompt, "system": system });
        let result = self.tool.call(args)
            .map(|v| v.to_string())
            .unwrap_or_else(|e| format!("NL reply error: {e}"));
        Message { role: MessageRole::Tool, content: Some(result), tool_calls: Vec::new() }
    }
}
