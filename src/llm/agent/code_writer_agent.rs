use serde_json::json;
use std::sync::Arc;

use crate::bot_adapter::models::MessageEvent;
use crate::bot_adapter::models::message::{Message as MsgEnum, PlainTextMessage};
use crate::llm::agent::Agent;
use crate::llm::function_tools::{CodeWriterTool, FunctionTool};
use crate::llm::{LLMBase, Message, MessageRole};

pub struct CodeWriterAgent {
    tool: CodeWriterTool,
}

impl CodeWriterAgent {
    pub fn new(llm: Arc<dyn LLMBase + Send + Sync>) -> Self {
        Self { tool: CodeWriterTool::new(llm) }
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

impl Agent for CodeWriterAgent {
    type Output = Message;

    fn name(&self) -> &'static str { "code_writer_agent" }

    fn on_event(&self, event: &MessageEvent) -> Self::Output {
        let task = Self::aggregate_text(event);
        self.on_agent_input(Message {
            role: MessageRole::User,
            content: Some(task),
            tool_calls: Vec::new(),
        })
    }

    fn on_agent_input(&self, input: Message) -> Self::Output {
        let content = input.content.unwrap_or_else(|| "Write a small example function.".to_string());
        let task = json!(content);
        let language = json!("");
        let constraints = json!("");
        let args = json!({ "task": task, "language": language, "constraints": constraints });
        let result = self.tool.call(args)
            .map(|v| v.to_string())
            .unwrap_or_else(|e| format!("Code writer error: {e}"));
        Message { role: MessageRole::Tool, content: Some(result), tool_calls: Vec::new() }
    }
}
