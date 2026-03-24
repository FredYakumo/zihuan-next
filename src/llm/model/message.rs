use serde::{Deserialize, Serialize};

use crate::llm::function_tools::ToolCalls;

use super::message_role::MessageRole;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCalls>,
}

impl Message {
    /// Create a system message with the given content and no tool calls.
    pub fn system<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::System,
            content: Some(content.into()),
            tool_calls: Vec::new(),
        }
    }

    /// Create a user message with the given content and no tool calls.
    pub fn user<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::User,
            content: Some(content.into()),
            tool_calls: Vec::new(),
        }
    }
}