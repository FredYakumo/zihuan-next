pub mod qq_chat_agent;
pub use qq_chat_agent as qq_message_agent_node;

/// Base trait for all event-driven agents.
///
/// An agent consumes an event and produces an output/decision.
///
use ims_bot_adapter::adapter::BotAdapter;
use zihuan_core::ims_bot_adapter::models::event_model::MessageEvent;

use crate::model::OpenAIMessage;

pub trait Agent: Send + Sync {
    type Output;

    fn name(&self) -> &'static str;

    fn on_event(&self, ims_bot_adapter: &mut BotAdapter, event: &MessageEvent) -> Self::Output;

    /// Invoke this agent using structured input (for agent-to-agent calls).
    /// Default implementation falls back to panic to surface unimplemented usage.
    fn on_agent_input(
        &self,
        ims_bot_adapter: &mut BotAdapter,
        event: &MessageEvent,
        messages: Vec<OpenAIMessage>,
    ) -> Self::Output;
}

pub trait FunctionToolsAgent: Send + Sync {
    fn get_tools(&self) -> Vec<&dyn crate::tooling::FunctionTool>;
}
