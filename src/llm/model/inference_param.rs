use std::sync::Arc;

use crate::llm::function_tools::FunctionTool;

use super::message::OpenAIMessage;

pub struct InferenceParam<'a> {
    pub messages: &'a Vec<OpenAIMessage>,
    pub tools: Option<&'a Vec<Arc<dyn FunctionTool>>>,
}