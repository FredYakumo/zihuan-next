pub mod agent;
pub mod llm_api;
pub mod function_tools;

use crate::llm::function_tools::{FunctionTool, ToolCalls};
use std::sync::Arc;

#[derive(Debug)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Convert a MessageRole to the string expected by chat APIs
pub fn role_to_str(role: &MessageRole) -> &'static str {
    match role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    }
}

/// Parse a role string from chat APIs into MessageRole
pub fn str_to_role(s: &str) -> MessageRole {
    match s {
        "system" => MessageRole::System,
        "user" => MessageRole::User,
        "tool" => MessageRole::Tool,
        _ => MessageRole::Assistant,
    }
}

pub struct Message {
    pub role: MessageRole,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCalls>,
}

pub struct InferenceParam {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Arc<dyn FunctionTool>>>,
}

pub trait LLMBase {
    fn get_model_name(&self) -> &str;

    fn inference(&self, param: &InferenceParam) -> Message;
}