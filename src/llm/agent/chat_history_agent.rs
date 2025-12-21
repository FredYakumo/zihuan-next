use serde_json::json;
use serde_json::Value;

use crate::bot_adapter::models::MessageEvent;
use crate::bot_adapter::models::message::{Message as MsgEnum, ReplyMessage};
use crate::llm::agent::Agent;
use crate::llm::function_tools::{ChatHistoryTool, FunctionTool};
use crate::llm::{Message, MessageRole};

pub struct ChatHistoryAgent {
    tool: ChatHistoryTool,
}

impl ChatHistoryAgent {
    pub fn new() -> Self { Self { tool: ChatHistoryTool::new() } }
}

impl Agent for ChatHistoryAgent {
    type Output = Message;

    fn name(&self) -> &'static str { "chat_history_agent" }

    fn on_event(&self, event: &MessageEvent) -> Self::Output {
        let mut target_id = event.message_id.to_string();
        for m in &event.message_list {
            if let MsgEnum::Reply(ReplyMessage { id, .. }) = m {
                target_id = id.to_string();
                break;
            }
        }
        self.on_agent_input(json!({"message_id": target_id}))
    }

    fn on_agent_input(&self, input: Value) -> Self::Output {
        let result = self.tool.call(input)
            .map(|v| v.to_string())
            .unwrap_or_else(|e| format!("Error fetching history: {e}"));
        Message { role: MessageRole::Tool, content: Some(result), tool_calls: Vec::new() }
    }
}
