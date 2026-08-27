use std::sync::Arc;

use async_trait::async_trait;

use super::SharedTool;
use crate::agent::agent::{Agent, AgentContext, AgentDescriptor};
use crate::agent::tools::{Tool, ToolCallingEngine, ToolCallingObserver, ToolCallingStopReason};
use crate::error::{Error, Result};
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::{LLMMessage, MessageRole, StreamToken};
use tokio::sync::mpsc;

/// The primary request-scoped intelligence of a RoleService.
pub struct BrainAgent {
    id: String,
    name: String,
    system_prompt: String,
    llm: Arc<dyn LLMBase>,
    tools: Vec<Arc<dyn Tool>>,
}

impl BrainAgent {
    pub fn new(id: impl Into<String>, name: impl Into<String>, system_prompt: impl Into<String>, llm: Arc<dyn LLMBase>, tools: Vec<Arc<dyn Tool>>) -> Self {
        Self { id: id.into(), name: name.into(), system_prompt: system_prompt.into(), llm, tools }
    }

    fn prepare_messages(&self, mut messages: Vec<LLMMessage>) -> Vec<LLMMessage> {
        if !self.system_prompt.trim().is_empty() && !messages.iter().any(|message| matches!(message.role, MessageRole::System)) {
            messages.insert(0, LLMMessage::system(self.system_prompt.clone()));
        }
        messages
    }

    fn engine(&self, llm: Arc<dyn LLMBase>, observer: Option<Arc<dyn ToolCallingObserver>>) -> ToolCallingEngine {
        let mut engine = ToolCallingEngine::new(llm);
        for tool in &self.tools {
            engine.add_tool(SharedTool::new(Arc::clone(tool)));
        }
        if let Some(observer) = observer {
            engine.set_observer(observer);
        }
        engine
    }

    /// Run a streamed Brain turn while preserving the complete message trace and stop reason.
    pub async fn run_streaming(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
    ) -> (Vec<LLMMessage>, ToolCallingStopReason) {
        self.engine(Arc::clone(&self.llm), observer)
            .run_streaming(self.prepare_messages(messages), token_tx)
            .await
    }

    /// Run a streamed Brain turn with a request-scoped model override.
    pub async fn run_streaming_with_llm(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
        llm: Arc<dyn LLMBase>,
    ) -> (Vec<LLMMessage>, ToolCallingStopReason) {
        self.engine(llm, observer)
            .run_streaming(self.prepare_messages(messages), token_tx)
            .await
    }
}

#[async_trait]
impl Agent for BrainAgent {
    type Input = Vec<LLMMessage>;
    type Output = Vec<LLMMessage>;

    fn descriptor(&self) -> AgentDescriptor {
        AgentDescriptor::new(Box::leak(self.id.clone().into_boxed_str()), Box::leak(self.name.clone().into_boxed_str()), vec!["primary_reasoning"])
    }

    async fn run(&self, _context: AgentContext, messages: Self::Input) -> Result<Self::Output> {
        let (output, reason) = self.engine(Arc::clone(&self.llm), None).run(self.prepare_messages(messages));
        match reason {
            ToolCallingStopReason::Done | ToolCallingStopReason::AwaitUserInput(_) => Ok(output),
            ToolCallingStopReason::TransportError(error) => Err(Error::ValidationError(format!("BrainAgent '{}' transport error: {error}", self.name))),
            ToolCallingStopReason::MaxIterationsReached => Err(Error::ValidationError(format!("BrainAgent '{}' exceeded tool iterations", self.name))),
        }
    }
}
