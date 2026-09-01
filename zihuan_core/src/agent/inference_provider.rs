use std::sync::Arc;

use crate::agent::tools::Tool;
use crate::graph::tool_spec::ToolDefinition;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::LLMMessage;
use crate::ims_bot_adapter::models::message::PersistedMedia;

#[derive(Clone)]
pub struct InferenceToolContext {
    pub last_user_text: String,
    pub workspace_path: Option<String>,
    pub session_id: Option<String>,
    pub llm: Arc<dyn LLMBase>,
    pub image_media: Vec<PersistedMedia>,
}

pub trait InferenceToolProvider: Send + Sync {
    fn augment_messages(&self, _messages: &mut Vec<LLMMessage>, _context: &InferenceToolContext) {}

    fn build_default_tools(&self, _context: &InferenceToolContext) -> Vec<Box<dyn Tool>> {
        Vec::new()
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition>;
}
