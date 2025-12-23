use serde_json::json;
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
        self.on_agent_input(Message {
            role: MessageRole::User,
            content: Some(prompt),
            tool_calls: Vec::new(),
        })
    }

    fn on_agent_input(&self, input: Message) -> Self::Output {
        let content = input.content.unwrap_or_default();
        let prompt = json!(content);
        let system = json!("You are a helpful assistant. Reply clearly and concisely.");
        let args = json!({ "prompt": prompt, "system": system });
        let result = self.tool.call(args)
            .map(|v| v.to_string())
            .unwrap_or_else(|e| format!("NL reply error: {e}"));
        Message { role: MessageRole::Tool, content: Some(result), tool_calls: Vec::new() }
    }
}
