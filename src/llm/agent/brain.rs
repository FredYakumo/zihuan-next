use std::sync::Arc;

use crate::bot_adapter::models::MessageEvent;
use crate::llm::agent::Agent;
use crate::llm::{LLMBase, Message};
use crate::error::Result;

pub struct BrainAgent {
    llm: Arc<dyn LLMBase + Send + Sync>,
}

impl BrainAgent {
    pub fn new(llm: Arc<dyn LLMBase + Send + Sync>) -> Self {
        Self { llm }
    }
}

impl Agent for BrainAgent {
    type Output = Result<()>;

    fn on_event(&self, _event: &MessageEvent) -> Self::Output {
        
        Ok(())
    }

    fn on_agent_input(&self, _input: Message) -> Self::Output {
        Ok(())
    }
}