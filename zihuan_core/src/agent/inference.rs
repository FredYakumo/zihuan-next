use std::sync::Arc;

use crate::model_inference::message_content_utils::sanitize_messages_for_inference;
use crate::model_inference::resource_resolver::{build_llm_model, resolve_llm_service_config};
use crate::model_inference::system_config::{AgentConfig, AgentType, LlmRefConfig};
use crate::storage_handler::ConnectionConfig;
use tokio::sync::mpsc;
use crate::agent::brain::{
    Brain, BrainObserver, BrainStopReason, BrainTool, ToolExecutionOutput, ToolRunDuration, MAX_TOOL_ITERATIONS,
};
use crate::agent::brain_tool_factory::BrainToolFactory;
use crate::error::{Error, Result};
use crate::llm::llm_base::LLMBase;
use crate::llm::tooling::FunctionTool;
use crate::llm::{LLMMessage, MessageRole, StreamToken};
use crate::graph_engine::brain_tool_spec::BrainToolDefinition;

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

#[derive(Clone, Default)]
pub struct StaticInferenceToolProvider {
    tool_definitions: Vec<BrainToolDefinition>,
}

impl StaticInferenceToolProvider {
    pub fn new(tool_definitions: Vec<BrainToolDefinition>) -> Self {
        Self { tool_definitions }
    }
}

impl InferenceToolProvider for StaticInferenceToolProvider {
    fn tool_definitions(&self) -> Vec<BrainToolDefinition> {
        self.tool_definitions.clone()
    }
}

#[derive(Clone)]
pub struct LoadedInferenceAgent {
    agent: AgentConfig,
    model_name: String,
    llm: Arc<dyn LLMBase>,
    tools: Arc<dyn InferenceToolProvider>,
    brain_tool_factory: Option<Arc<dyn BrainToolFactory>>,
}

struct DynBrainToolWrapper(Box<dyn BrainTool>);

impl BrainTool for DynBrainToolWrapper {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.0.spec()
    }

    fn run_duration(&self) -> ToolRunDuration {
        self.0.run_duration()
    }

    fn execute(&self, call_content: &str, arguments: &serde_json::Value) -> String {
        self.0.execute(call_content, arguments)
    }

    fn execute_with_outcome(&self, call_content: &str, arguments: &serde_json::Value) -> ToolExecutionOutput {
        self.0.execute_with_outcome(call_content, arguments)
    }

    fn execute_with_progress(
        &self,
        call_content: &str,
        arguments: &serde_json::Value,
        on_output: Arc<dyn Fn(&str, &str) + Send + Sync>,
    ) -> ToolExecutionOutput {
        self.0.execute_with_progress(call_content, arguments, on_output)
    }
}

impl LoadedInferenceAgent {
    pub fn load(agent: &AgentConfig, connections: &[ConnectionConfig]) -> Result<Self> {
        let llm_refs = crate::model_inference::system_config::load_llm_refs()?;
        Self::load_with_refs(agent, &llm_refs, connections)
    }

    pub fn load_with_refs(
        agent: &AgentConfig,
        llm_refs: &[LlmRefConfig],
        _connections: &[ConnectionConfig],
    ) -> Result<Self> {
        Err(Error::ValidationError(
            "load_with_refs requires an InferenceToolProvider; use load_with_tools instead".to_string(),
        ))
    }

    pub fn load_with_refs_and_tools(
        agent: &AgentConfig,
        llm_refs: &[LlmRefConfig],
        tools: Arc<dyn InferenceToolProvider>,
    ) -> Result<Self> {
        Self::load_with_tools(agent, llm_refs, tools)
    }

    pub fn load_with_tools(
        agent: &AgentConfig,
        llm_refs: &[LlmRefConfig],
        tools: Arc<dyn InferenceToolProvider>,
    ) -> Result<Self> {
        Self::load_with_tools_and_factory(agent, llm_refs, tools, None)
    }

    pub fn load_with_tools_and_factory(
        agent: &AgentConfig,
        llm_refs: &[LlmRefConfig],
        tools: Arc<dyn InferenceToolProvider>,
        brain_tool_factory: Option<Arc<dyn BrainToolFactory>>,
    ) -> Result<Self> {
        if !agent.enabled {
            return Err(Error::ValidationError(format!("agent '{}' is disabled", agent.name)));
        }

        let llm_ref_id = match &agent.agent_type {
            AgentType::HttpStream(config) => config.llm_ref_id.as_deref(),
            AgentType::QqChat(config) => config.llm_ref_id.as_deref(),
            AgentType::Workspace(config) => config.llm_ref_id.as_deref(),
        };
        let llm_config = resolve_llm_service_config(llm_ref_id, llm_refs, &agent.name)?;
        let model_name = llm_config.model_name.clone();
        let llm = build_llm_model(&llm_config)?;

        Ok(Self {
            agent: agent.clone(),
            model_name,
            llm,
            tools,
            brain_tool_factory,
        })
    }

    pub fn agent_config(&self) -> &AgentConfig {
        &self.agent
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn infer_response(&self, messages: Vec<LLMMessage>) -> Result<LLMMessage> {
        let output_messages = self.infer_response_with_trace(messages)?;
        output_messages
            .into_iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant) && message.tool_calls.is_empty())
            .ok_or_else(|| {
                Error::StringError(format!("agent '{}' did not produce a final assistant message", self.agent.name))
            })
    }

    pub fn infer_response_with_trace(&self, messages: Vec<LLMMessage>) -> Result<Vec<LLMMessage>> {
        self.infer_response_with_trace_and_llm(messages, Arc::clone(&self.llm), None)
    }

    pub fn infer_response_with_trace_and_llm(
        &self,
        messages: Vec<LLMMessage>,
        llm: Arc<dyn LLMBase>,
        workspace_path: Option<String>,
    ) -> Result<Vec<LLMMessage>> {
        let context = build_inference_tool_context(&messages, workspace_path);

        let mut conversation = sanitize_messages_for_inference(messages);
        if conversation.is_empty() {
            return Err(Error::ValidationError(
                "messages must not be empty after sanitization".to_string(),
            ));
        }

        self.tools.augment_messages(&mut conversation, &context);
        let default_brain_tools = self.tools.build_default_tools(&context);

        run_agent_brain(
            &self.agent,
            llm,
            default_brain_tools,
            self.tools.tool_definitions(),
            conversation,
            self.brain_tool_factory.as_deref(),
        )
    }

    pub async fn infer_response_streaming_with_trace(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
        observer: Option<Arc<dyn BrainObserver>>,
        workspace_path: Option<String>,
    ) -> Result<(Vec<LLMMessage>, BrainStopReason)> {
        self.infer_response_streaming_with_trace_and_llm(
            messages,
            token_tx,
            observer,
            Arc::clone(&self.llm),
            workspace_path,
        )
        .await
    }

    pub async fn infer_response_streaming_with_trace_and_llm(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
        observer: Option<Arc<dyn BrainObserver>>,
        llm: Arc<dyn LLMBase>,
        workspace_path: Option<String>,
    ) -> Result<(Vec<LLMMessage>, BrainStopReason)> {
        let context = build_inference_tool_context(&messages, workspace_path);

        let mut conversation = sanitize_messages_for_inference(messages);
        if conversation.is_empty() {
            return Err(Error::ValidationError(
                "messages must not be empty after sanitization".to_string(),
            ));
        }

        self.tools.augment_messages(&mut conversation, &context);
        let default_brain_tools = self.tools.build_default_tools(&context);

        run_agent_brain_streaming(
            &self.agent,
            llm,
            default_brain_tools,
            self.tools.tool_definitions(),
            conversation,
            token_tx,
            observer,
            self.brain_tool_factory.as_deref(),
        )
        .await
    }
}

pub fn infer_agent_response(
    agent: &AgentConfig,
    llm_refs: &[LlmRefConfig],
    messages: Vec<LLMMessage>,
    tools: Arc<dyn InferenceToolProvider>,
) -> Result<LLMMessage> {
    infer_agent_response_with_model(agent, llm_refs, messages, None, tools)
}

pub fn infer_agent_response_with_model(
    agent: &AgentConfig,
    llm_refs: &[LlmRefConfig],
    messages: Vec<LLMMessage>,
    model_override: Option<&str>,
    tools: Arc<dyn InferenceToolProvider>,
) -> Result<LLMMessage> {
    let loaded = LoadedInferenceAgent::load_with_tools(agent, llm_refs, tools)?;
    let output_messages = if let Some(model_id) = model_override {
        let llm_config = resolve_llm_service_config(Some(model_id), llm_refs, &agent.name)?;
        let llm = build_llm_model(&llm_config)?;
        loaded.infer_response_with_trace_and_llm(messages, llm, None)?
    } else {
        loaded.infer_response_with_trace(messages)?
    };
    output_messages
        .into_iter()
        .rev()
        .find(|message| matches!(message.role, MessageRole::Assistant) && message.tool_calls.is_empty())
        .ok_or_else(|| Error::StringError(format!("agent '{}' did not produce a final assistant message", agent.name)))
}

pub fn infer_agent_response_with_trace(
    agent: &AgentConfig,
    llm_refs: &[LlmRefConfig],
    messages: Vec<LLMMessage>,
    tools: Arc<dyn InferenceToolProvider>,
) -> Result<Vec<LLMMessage>> {
    LoadedInferenceAgent::load_with_tools(agent, llm_refs, tools)?.infer_response_with_trace(messages)
}

pub fn resolve_agent_model_name(agent: &AgentConfig, llm_refs: &[LlmRefConfig]) -> Result<String> {
    resolve_agent_model_name_with_override(agent, llm_refs, None)
}

pub fn resolve_agent_model_name_with_override(
    agent: &AgentConfig,
    llm_refs: &[LlmRefConfig],
    model_override: Option<&str>,
) -> Result<String> {
    let llm_ref_id = match model_override {
        Some(id) => Some(id),
        None => match &agent.agent_type {
            AgentType::HttpStream(config) => config.llm_ref_id.as_deref(),
            AgentType::QqChat(config) => config.llm_ref_id.as_deref(),
            AgentType::Workspace(config) => config.llm_ref_id.as_deref(),
        },
    };
    Ok(resolve_llm_service_config(llm_ref_id, llm_refs, &agent.name)?.model_name)
}

fn build_inference_tool_context(messages: &[LLMMessage], workspace_path: Option<String>) -> InferenceToolContext {
    InferenceToolContext {
        last_user_text: messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, MessageRole::User))
            .and_then(|m| m.content_text())
            .map(ToOwned::to_owned)
            .unwrap_or_default(),
        workspace_path,
    }
}

fn build_brain(
    agent: &AgentConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn BrainTool>>,
    tool_definitions: Vec<BrainToolDefinition>,
    tool_factory: Option<&dyn BrainToolFactory>,
) -> Brain {
    let mut brain = Brain::new(llm);

    for tool in default_tools {
        brain.add_tool(DynBrainToolWrapper(tool));
    }

    for tool_def in tool_definitions {
        if let Some(factory) = tool_factory {
            if let Some(tool) = factory.create_tool(&tool_def) {
                brain.add_tool(DynBrainToolWrapper(tool));
                continue;
            }
        }
        // If no factory or factory returned None, skip the tool
        log::warn!(
            "agent '{}': no tool factory available for tool '{}', skipping",
            agent.name,
            tool_def.name
        );
    }

    brain
}

fn handle_brain_result(
    agent_name: &str,
    output_messages: Vec<LLMMessage>,
    stop_reason: BrainStopReason,
) -> Result<Vec<LLMMessage>> {
    match stop_reason {
        BrainStopReason::Done => Ok(output_messages),
        BrainStopReason::TransportError(content) => Err(Error::StringError(format!(
            "chat stream LLM request failed for '{}': {}",
            agent_name, content
        ))),
        BrainStopReason::MaxIterationsReached => Err(Error::StringError(format!(
            "chat stream exceeded max tool iterations ({MAX_TOOL_ITERATIONS}) for '{}'",
            agent_name
        ))),
        BrainStopReason::AwaitUserInput(request) => Ok(output_messages
            .into_iter()
            .chain(std::iter::once(LLMMessage::assistant_text(format!(
                "需要用户补充信息: {}",
                request.question
            ))))
            .collect()),
    }
}

fn handle_brain_result_with_reason(
    agent_name: &str,
    output_messages: Vec<LLMMessage>,
    stop_reason: BrainStopReason,
) -> Result<(Vec<LLMMessage>, BrainStopReason)> {
    match &stop_reason {
        BrainStopReason::Done | BrainStopReason::AwaitUserInput(_) => Ok((output_messages, stop_reason)),
        BrainStopReason::TransportError(content) => Err(Error::StringError(format!(
            "chat stream LLM request failed for '{}': {}",
            agent_name, content
        ))),
        BrainStopReason::MaxIterationsReached => Err(Error::StringError(format!(
            "chat stream exceeded max tool iterations ({MAX_TOOL_ITERATIONS}) for '{}'",
            agent_name
        ))),
    }
}

fn run_agent_brain(
    agent: &AgentConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn BrainTool>>,
    tool_definitions: Vec<BrainToolDefinition>,
    messages: Vec<LLMMessage>,
    tool_factory: Option<&dyn BrainToolFactory>,
) -> Result<Vec<LLMMessage>> {
    let brain = build_brain(agent, llm, default_tools, tool_definitions, tool_factory);
    let (output_messages, stop_reason) = brain.run(messages);
    handle_brain_result(&agent.name, output_messages, stop_reason)
}

async fn run_agent_brain_streaming(
    agent: &AgentConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn BrainTool>>,
    tool_definitions: Vec<BrainToolDefinition>,
    messages: Vec<LLMMessage>,
    token_tx: mpsc::UnboundedSender<StreamToken>,
    observer: Option<Arc<dyn BrainObserver>>,
    tool_factory: Option<&dyn BrainToolFactory>,
) -> Result<(Vec<LLMMessage>, BrainStopReason)> {
    let mut brain = build_brain(agent, llm, default_tools, tool_definitions, tool_factory);
    if let Some(obs) = observer {
        brain.set_observer(obs);
    }
    let (output_messages, stop_reason) = brain.run_streaming(messages, token_tx).await;
    handle_brain_result_with_reason(&agent.name, output_messages, stop_reason)
}
