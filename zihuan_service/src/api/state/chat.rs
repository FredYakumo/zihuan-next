use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone)]
pub struct RunningChatMessage {
    pub message_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub agent_type: String,
    pub agent_avatar_url: Option<String>,
    pub trace_id: String,
    pub workspace_path: Option<String>,
    pub model_config_id: Option<String>,
    pub image_understand_model_config_id: Option<String>,
    pub timestamp: String,
    pub content: String,
    pub reasoning_content: String,
    pub live_tool_calls: Vec<RunningChatToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunningChatToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
    pub result: String,
    pub done: bool,
}
