use crate::agent::brain::BrainTool;
use crate::graph::brain_tool_spec::BrainToolDefinition;
use crate::llm::LLMMessage;

#[derive(Clone)]
pub struct InferenceToolContext {
    pub last_user_text: String,
    pub workspace_path: Option<String>,
}

pub trait InferenceToolProvider: Send + Sync {
    fn augment_messages(&self, _messages: &mut Vec<LLMMessage>, _context: &InferenceToolContext) {}

    fn build_default_tools(&self, _context: &InferenceToolContext) -> Vec<Box<dyn BrainTool>> {
        Vec::new()
    }

    fn tool_definitions(&self) -> Vec<BrainToolDefinition>;
}
