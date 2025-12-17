pub mod agent;
pub mod llm_api;
pub mod function_tools;

pub use llm_api::LLMAPI;
use crate::llm::function_tools::{FunctionTool, ToolCalls};

pub enum MessageRole {
    System,
    User,
    Assistant,
}

pub struct Message {
    pub role: MessageRole,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCalls>,
}

pub struct InferenceParam {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Box<dyn FunctionTool>>>,
}

pub trait LLMBase {
    fn get_model_name(&self) -> &str;

    fn inference(&self, param: &InferenceParam) -> Message;
}