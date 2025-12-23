use crate::bot_adapter::models::MessageEvent;
use crate::llm::agent::Agent;
use crate::llm::Message;
use crate::error::Result;

pub struct BrainAgent {

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