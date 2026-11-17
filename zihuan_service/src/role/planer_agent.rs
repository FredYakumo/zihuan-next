//! Independent coordination agent. No Workspace resources, rules, or tools are loaded here.
use async_trait::async_trait;
use std::sync::Arc;
use zihuan_core::agent::tools::{Tool, ToolCallingEngine, ToolCallingRequest, ToolCallingResult};
use zihuan_core::agent::{Agent, AgentContext, SharedTool};
use zihuan_core::error::Result;
use zihuan_core::model_inference::llm::{llm_base::LLMBase, LLMMessage};

pub struct PlanerAgentConfig {
    pub llm: Arc<dyn LLMBase>,
    pub system_prompt: String,
    pub tools: Vec<Arc<dyn Tool>>,
}
pub struct PlanerAgent {
    config: PlanerAgentConfig,
}
#[async_trait]
impl Agent for PlanerAgent {
    type Config = PlanerAgentConfig;
    type Input = ToolCallingRequest;
    type Output = ToolCallingResult;
    fn name(&self) -> &str {
        "planer"
    }
    fn llm(&self) -> Arc<dyn LLMBase> {
        self.config.llm.clone()
    }
    fn new(config: Self::Config) -> Result<Self> {
        Ok(Self { config })
    }
    async fn run(&self, mut context: AgentContext, input: Self::Input) -> Result<Self::Output> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        context.token_tx = Some(tx);
        let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
        let result = self.run_streaming(context, input).await;
        drain.abort();
        result
    }
    async fn run_streaming(
        &self,
        context: AgentContext,
        input: Self::Input,
    ) -> Result<Self::Output> {
        let mut engine = ToolCallingEngine::new(self.llm());
        for tool in &self.config.tools {
            engine.add_tool(SharedTool::new(tool.clone()));
        }
        if let Some(observer) = context.observer {
            engine.set_observer(observer);
        }
        if let Some(cancellation) = context.cancellation {
            engine.set_cancellation(cancellation);
        }
        let tokens = context.token_tx.ok_or_else(|| {
            zihuan_core::string_error!("Planer streaming turn requires a token sink")
        })?;
        let mut messages = vec![LLMMessage::system(self.config.system_prompt.clone())];
        messages.extend(input.messages);
        let (messages, stop_reason) = engine.run_streaming(messages, tokens).await;
        Ok(ToolCallingResult { messages, stop_reason })
    }
}
