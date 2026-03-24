use std::sync::Arc;

use crate::llm::function_tools::FunctionTool;

use super::message::Message;

pub struct InferenceParam<'a> {
    pub messages: &'a Vec<Message>,
    pub tools: Option<&'a Vec<Arc<dyn FunctionTool>>>,
}