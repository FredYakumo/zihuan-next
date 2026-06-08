use std::sync::Arc;

use crate::llm::tooling::FunctionTool;

use super::llm_message::LLMMessage;

pub struct InferenceParam<'a> {
    pub messages: &'a Vec<LLMMessage>,
    pub tools: Option<&'a Vec<Arc<dyn FunctionTool>>>,
}
