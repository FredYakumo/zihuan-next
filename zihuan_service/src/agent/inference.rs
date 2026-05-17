use std::collections::HashMap;
use std::sync::Arc;

use crate::nodes::tool_subgraph::{ToolResultMode, ToolSubgraphRunner};
use model_inference::message_content_utils::sanitize_messages_for_inference;
use model_inference::system_config::{AgentConfig, AgentType, LlmRefConfig};
use storage_handler::{load_connections, ConnectionConfig};
use tokio::sync::mpsc;
use zihuan_agent::brain::{Brain, BrainStopReason, BrainTool, MAX_TOOL_ITERATIONS};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{MessageRole, OpenAIMessage};
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;

use crate::resource_resolver::{build_llm_model, resolve_llm_service_config};

#[derive(Clone)]
pub struct InferenceToolContext {
    pub last_user_text: String,
}

pub trait InferenceToolProvider: Send + Sync {
    fn augment_messages(
        &self,
        _messages: &mut Vec<OpenAIMessage>,
        _context: &InferenceToolContext,
    ) {
    }

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
}

struct ServiceSubgraphBrainTool {
    runner: ToolSubgraphRunner,
}

impl BrainTool for ServiceSubgraphBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.runner.spec()
    }

    fn execute(&self, call_content: &str, arguments: &serde_json::Value) -> String {
        self.runner.execute_to_string(call_content, arguments)
    }
}

struct DynBrainToolWrapper(Box<dyn BrainTool>);

impl BrainTool for DynBrainToolWrapper {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.0.spec()
    }

    fn execute(&self, call_content: &str, arguments: &serde_json::Value) -> String {
        self.0.execute(call_content, arguments)
    }
}

impl LoadedInferenceAgent {
    pub fn load(agent: &AgentConfig, connections: &[ConnectionConfig]) -> Result<Self> {
        let llm_refs = model_inference::system_config::load_llm_refs()?;
        Self::load_with_refs(agent, &llm_refs, connections)
    }

    pub fn load_with_refs(
        agent: &AgentConfig,
        llm_refs: &[LlmRefConfig],
        connections: &[ConnectionConfig],
    ) -> Result<Self> {
        let tools = super::build_inference_tool_provider(agent, connections)?;
        Self::load_with_tools(agent, llm_refs, tools)
    }

    pub fn load_with_tools(
        agent: &AgentConfig,
        llm_refs: &[LlmRefConfig],
        tools: Arc<dyn InferenceToolProvider>,
    ) -> Result<Self> {
        if !agent.enabled {
            return Err(Error::ValidationError(format!(
                "agent '{}' is disabled",
                agent.name
            )));
        }

        let llm_ref_id = match &agent.agent_type {
            AgentType::HttpStream(config) => config.llm_ref_id.as_deref(),
            AgentType::QqChat(config) => config.llm_ref_id.as_deref(),
        };
        let llm_config = resolve_llm_service_config(llm_ref_id, llm_refs, &agent.name)?;
        let model_name = llm_config.model_name.clone();
        let llm = build_llm_model(&llm_config)?;

        Ok(Self {
            agent: agent.clone(),
            model_name,
            llm,
            tools,
        })
    }

    pub fn agent_config(&self) -> &AgentConfig {
        &self.agent
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn infer_response(&self, messages: Vec<OpenAIMessage>) -> Result<OpenAIMessage> {
        let output_messages = self.infer_response_with_trace(messages)?;
        output_messages
            .into_iter()
            .rev()
            .find(|message| {
                matches!(message.role, MessageRole::Assistant) && message.tool_calls.is_empty()
            })
            .ok_or_else(|| {
                Error::StringError(format!(
                    "agent '{}' did not produce a final assistant message",
                    self.agent.name
                ))
            })
    }

    pub fn infer_response_with_trace(
        &self,
        messages: Vec<OpenAIMessage>,
    ) -> Result<Vec<OpenAIMessage>> {
        let context = build_inference_tool_context(&messages);

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
            Arc::clone(&self.llm),
            default_brain_tools,
            self.tools.tool_definitions(),
            conversation,
        )
    }

    pub async fn infer_response_streaming_with_trace(
        &self,
        messages: Vec<OpenAIMessage>,
        token_tx: mpsc::UnboundedSender<String>,
    ) -> Result<Vec<OpenAIMessage>> {
        let context = build_inference_tool_context(&messages);

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
            Arc::clone(&self.llm),
            default_brain_tools,
            self.tools.tool_definitions(),
            conversation,
            token_tx,
        )
        .await
    }
}

pub fn infer_agent_response(
    agent: &AgentConfig,
    llm_refs: &[LlmRefConfig],
    messages: Vec<OpenAIMessage>,
) -> Result<OpenAIMessage> {
    let connections = load_connections().unwrap_or_default();
    LoadedInferenceAgent::load_with_refs(agent, llm_refs, &connections)?.infer_response(messages)
}

pub fn infer_agent_response_with_trace(
    agent: &AgentConfig,
    llm_refs: &[LlmRefConfig],
    messages: Vec<OpenAIMessage>,
) -> Result<Vec<OpenAIMessage>> {
    let connections = load_connections().unwrap_or_default();
    LoadedInferenceAgent::load_with_refs(agent, llm_refs, &connections)?
        .infer_response_with_trace(messages)
}

pub fn resolve_agent_model_name(agent: &AgentConfig, llm_refs: &[LlmRefConfig]) -> Result<String> {
    let llm_ref_id = match &agent.agent_type {
        AgentType::HttpStream(config) => config.llm_ref_id.as_deref(),
        AgentType::QqChat(config) => config.llm_ref_id.as_deref(),
    };
    Ok(resolve_llm_service_config(llm_ref_id, llm_refs, &agent.name)?.model_name)
}

fn build_inference_tool_context(messages: &[OpenAIMessage]) -> InferenceToolContext {
    InferenceToolContext {
        last_user_text: messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, MessageRole::User))
            .and_then(|m| m.content_text())
            .map(ToOwned::to_owned)
            .unwrap_or_default(),
    }
}

fn build_brain(
    agent: &AgentConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn BrainTool>>,
    tool_definitions: Vec<BrainToolDefinition>,
) -> Brain {
    let mut brain = Brain::new(llm);

    for tool in default_tools {
        brain.add_tool(DynBrainToolWrapper(tool));
    }

    for tool_def in tool_definitions {
        brain.add_tool(ServiceSubgraphBrainTool {
            runner: ToolSubgraphRunner {
                node_id: format!("agent_inference_{}", agent.id),
                owner_node_type: "brain".to_string(),
                shared_inputs: Vec::new(),
                definition: tool_def,
                shared_runtime_values: HashMap::new(),
                result_mode: ToolResultMode::JsonObject,
            },
        });
    }

    brain
}

fn handle_brain_result(
    agent_name: &str,
    output_messages: Vec<OpenAIMessage>,
    stop_reason: BrainStopReason,
) -> Result<Vec<OpenAIMessage>> {
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
    }
}

fn run_agent_brain(
    agent: &AgentConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn BrainTool>>,
    tool_definitions: Vec<BrainToolDefinition>,
    messages: Vec<OpenAIMessage>,
) -> Result<Vec<OpenAIMessage>> {
    let brain = build_brain(agent, llm, default_tools, tool_definitions);
    let (output_messages, stop_reason) = brain.run(messages);
    handle_brain_result(&agent.name, output_messages, stop_reason)
}

async fn run_agent_brain_streaming(
    agent: &AgentConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn BrainTool>>,
    tool_definitions: Vec<BrainToolDefinition>,
    messages: Vec<OpenAIMessage>,
    token_tx: mpsc::UnboundedSender<String>,
) -> Result<Vec<OpenAIMessage>> {
    let brain = build_brain(agent, llm, default_tools, tool_definitions);
    let (output_messages, stop_reason) = brain.run_streaming(messages, token_tx).await;
    handle_brain_result(&agent.name, output_messages, stop_reason)
}
