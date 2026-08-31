use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use zihuan_core::tool_subgraph::{ToolResultMode, ToolSubgraphRunner};
use zihuan_core::model_inference::message_content_utils::sanitize_messages_for_inference;
use zihuan_core::agent::service_config::{RoleServiceConfig, RoleServiceType};
use zihuan_core::config::llm_refs::{load_llm_refs, LlmRefConfig};
use zihuan_core::storage::{load_connections, ConnectionConfig};
use tokio::sync::mpsc;
use zihuan_core::agent::tools::{
    ToolCallingEngine, ToolCallingObserver, ToolCallingStopReason, Tool, ToolExecutionOutput, ToolRunDuration, MAX_TOOL_ITERATIONS,
};
use zihuan_core::error::{Error, Result};
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::model_inference::llm::{LLMMessage, MessageRole, StreamToken};
use zihuan_core::model_inference::inference_function::compact_message::{compact_message_history, compaction_threshold, estimate_messages_tokens};
use zihuan_core::system_config::current_context_compaction_percent;
use zihuan_core::graph::tool_spec::ToolDefinition;

use zihuan_core::agent::resource_resolver::{build_llm_model, resolve_llm_service_config};
use zihuan_core::role::{RoleService, RoleServiceContext, RoleServiceDescriptor, RoleServiceKind};

pub use zihuan_core::agent::inference_provider::{InferenceToolContext, InferenceToolProvider};

#[derive(Debug, Clone)]
pub enum ContextCompactionEvent {
    Started,
    Completed {
        estimated_tokens_before: usize,
        estimated_tokens_after: usize,
        duration: Duration,
    },
    Failed,
}

pub type ContextCompactionObserver = Arc<dyn Fn(ContextCompactionEvent) + Send + Sync>;

#[derive(Clone, Default)]
pub struct StaticInferenceToolProvider {
    tool_definitions: Vec<ToolDefinition>,
}

impl StaticInferenceToolProvider {
    pub fn new(tool_definitions: Vec<ToolDefinition>) -> Self {
        Self { tool_definitions }
    }
}

impl InferenceToolProvider for StaticInferenceToolProvider {
    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool_definitions.clone()
    }
}

#[derive(Clone)]
/// The primary BrainAgent assembled for a configured RoleService.
pub struct RoleBrainAgent {
    agent: RoleServiceConfig,
    model_name: String,
    llm: Arc<dyn LLMBase>,
    tools: Arc<dyn InferenceToolProvider>,
}

struct ServiceSubgraphTool {
    runner: ToolSubgraphRunner,
}

impl Tool for ServiceSubgraphTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        self.runner.spec()
    }

    fn run_duration(&self) -> ToolRunDuration {
        self.runner.definition.run_duration
    }

    fn execute(&self, call_content: &str, arguments: &serde_json::Value) -> String {
        self.runner.execute_to_string(call_content, arguments)
    }

    fn execute_with_outcome(&self, call_content: &str, arguments: &serde_json::Value) -> ToolExecutionOutput {
        ToolExecutionOutput::text(self.execute(call_content, arguments))
    }

}

struct DynToolWrapper(Box<dyn Tool>);

impl Tool for DynToolWrapper {
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

impl RoleBrainAgent {
    pub fn load(agent: &RoleServiceConfig, connections: &[ConnectionConfig]) -> Result<Self> {
        let llm_refs = load_llm_refs()?;
        Self::load_with_refs(agent, &llm_refs, connections)
    }

    pub fn load_with_refs(
        agent: &RoleServiceConfig,
        llm_refs: &[LlmRefConfig],
        connections: &[ConnectionConfig],
    ) -> Result<Self> {
        let tools = crate::role::build_role_tool_provider(agent, connections)?;
        Self::load_with_tools(agent, llm_refs, tools)
    }

    pub fn load_with_tools(
        agent: &RoleServiceConfig,
        llm_refs: &[LlmRefConfig],
        tools: Arc<dyn InferenceToolProvider>,
    ) -> Result<Self> {
        if !agent.enabled {
            return Err(Error::ValidationError(format!("agent '{}' is disabled", agent.name)));
        }

        let llm_ref_id = match &agent.role_service_type {
            RoleServiceType::QqChat(config) => config.llm_ref_id.as_deref(),
            RoleServiceType::Workspace(config) => config.llm_ref_id.as_deref(),
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

    pub fn agent(&self) -> &RoleServiceConfig {
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
                zihuan_core::string_error!("agent '{}' did not produce a final assistant message", self.agent.name)
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
        let context = build_inference_tool_context(&messages, workspace_path, None, Arc::clone(&llm));

        let mut conversation = sanitize_messages_for_inference(messages);
        if conversation.is_empty() {
            return Err(Error::ValidationError(
                "messages must not be empty after sanitization".to_string(),
            ));
        }

        self.tools.augment_messages(&mut conversation, &context);
        let default_brain_tools = self.tools.build_default_tools(&context);

        run_agent_tool_calling(
            &self.agent,
            llm,
            default_brain_tools,
            self.tools.tool_definitions(),
            conversation,
        )
    }

    pub async fn infer_response_streaming_with_trace(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
        compaction_observer: Option<ContextCompactionObserver>,
        workspace_path: Option<String>,
        session_id: Option<String>,
    ) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)> {
        self.infer_response_streaming_with_trace_and_llm(
            messages,
            token_tx,
            observer,
            compaction_observer,
            Arc::clone(&self.llm),
            workspace_path,
            session_id,
        )
        .await
    }

    pub async fn infer_response_streaming_with_trace_and_llm(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
        compaction_observer: Option<ContextCompactionObserver>,
        llm: Arc<dyn LLMBase>,
        workspace_path: Option<String>,
        session_id: Option<String>,
    ) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)> {
        let context = build_inference_tool_context(&messages, workspace_path, session_id, Arc::clone(&llm));

        let mut conversation = sanitize_messages_for_inference(messages);
        if conversation.is_empty() {
            return Err(Error::ValidationError(
                "messages must not be empty after sanitization".to_string(),
            ));
        }

        if matches!(self.agent.role_service_type, RoleServiceType::Workspace(_)) {
            if let (Some(observer), Some(latest_user_index)) = (
                compaction_observer,
                conversation.iter().rposition(|message| matches!(message.role, MessageRole::User)),
            ) {
                let latest_user_message = conversation.remove(latest_user_index);
                let estimated_tokens_before = estimate_messages_tokens(&conversation)
                    + estimate_messages_tokens(std::slice::from_ref(&latest_user_message));
                let threshold = compaction_threshold(
                    llm.context_length(),
                    current_context_compaction_percent(),
                );
                if estimated_tokens_before > threshold {
                    observer(ContextCompactionEvent::Started);
                    let started_at = Instant::now();
                    let compact_result = compact_message_history(
                        &llm,
                        conversation,
                        threshold,
                        &latest_user_message,
                    );
                    conversation = compact_result.messages;
                    if compact_result.did_compact {
                        observer(ContextCompactionEvent::Completed {
                            estimated_tokens_before,
                            estimated_tokens_after: compact_result.estimated_tokens_after
                                + estimate_messages_tokens(std::slice::from_ref(&latest_user_message)),
                            duration: started_at.elapsed(),
                        });
                    } else {
                        observer(ContextCompactionEvent::Failed);
                    }
                }
                conversation.push(latest_user_message);
            }
        }

        self.tools.augment_messages(&mut conversation, &context);
        let default_brain_tools = self.tools.build_default_tools(&context);

        run_agent_tool_calling_streaming(
            &self.agent,
            llm,
            default_brain_tools,
            self.tools.tool_definitions(),
            conversation,
            token_tx,
            observer,
        )
        .await
    }
}

#[async_trait]
impl RoleService for RoleBrainAgent {
    type Input = Vec<LLMMessage>;
    type Output = Vec<LLMMessage>;

    fn descriptor(&self) -> RoleServiceDescriptor {
        RoleServiceDescriptor {
            id: self.agent.id.clone(),
            name: self.agent.name.clone(),
            kind: match self.agent.role_service_type {
                RoleServiceType::QqChat(_) => RoleServiceKind::QqChat,
                RoleServiceType::Workspace(_) => RoleServiceKind::Workspace,
            },
        }
    }

    async fn handle(
        &self,
        _context: RoleServiceContext,
        input: Self::Input,
    ) -> Result<Self::Output> {
        self.infer_response_with_trace(input)
    }
}

pub fn infer_role_response(
    agent: &RoleServiceConfig,
    llm_refs: &[LlmRefConfig],
    messages: Vec<LLMMessage>,
) -> Result<LLMMessage> {
    infer_role_response_with_model(agent, llm_refs, messages, None)
}

pub fn infer_role_response_with_model(
    agent: &RoleServiceConfig,
    llm_refs: &[LlmRefConfig],
    messages: Vec<LLMMessage>,
    model_override: Option<&str>,
) -> Result<LLMMessage> {
    let connections = load_connections().unwrap_or_default();
    let loaded = RoleBrainAgent::load_with_refs(agent, llm_refs, &connections)?;
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
        .ok_or_else(|| zihuan_core::string_error!("agent '{}' did not produce a final assistant message", agent.name))
}

pub fn infer_role_response_with_trace(
    agent: &RoleServiceConfig,
    llm_refs: &[LlmRefConfig],
    messages: Vec<LLMMessage>,
) -> Result<Vec<LLMMessage>> {
    let connections = load_connections().unwrap_or_default();
    RoleBrainAgent::load_with_refs(agent, llm_refs, &connections)?.infer_response_with_trace(messages)
}

pub fn resolve_role_model_name(agent: &RoleServiceConfig, llm_refs: &[LlmRefConfig]) -> Result<String> {
    resolve_role_model_name_with_override(agent, llm_refs, None)
}

pub fn resolve_role_model_name_with_override(
    agent: &RoleServiceConfig,
    llm_refs: &[LlmRefConfig],
    model_override: Option<&str>,
) -> Result<String> {
    let llm_ref_id = match model_override {
        Some(id) => Some(id),
        None => match &agent.role_service_type {
            RoleServiceType::QqChat(config) => config.llm_ref_id.as_deref(),
            RoleServiceType::Workspace(config) => config.llm_ref_id.as_deref(),
        },
    };
    Ok(resolve_llm_service_config(llm_ref_id, llm_refs, &agent.name)?.model_name)
}

fn build_inference_tool_context(
    messages: &[LLMMessage],
    workspace_path: Option<String>,
    session_id: Option<String>,
    llm: Arc<dyn LLMBase>,
) -> InferenceToolContext {
    InferenceToolContext {
        last_user_text: messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, MessageRole::User))
            .and_then(|m| m.content_text())
            .map(ToOwned::to_owned)
            .unwrap_or_default(),
        workspace_path,
        session_id,
        llm,
    }
}

fn build_tool_calling_engine(
    agent: &RoleServiceConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn Tool>>,
    tool_definitions: Vec<ToolDefinition>,
) -> ToolCallingEngine {
    let mut brain = ToolCallingEngine::new(llm);

    for tool in default_tools {
        brain.add_tool(DynToolWrapper(tool));
    }

    for tool_def in tool_definitions {
        brain.add_tool(ServiceSubgraphTool {
            runner: ToolSubgraphRunner {
                node_id: format!("agent_inference_{}", agent.id),
                owner_node_type: "tool_calling".to_string(),
                shared_inputs: Vec::new(),
                definition: tool_def,
                shared_runtime_values: Arc::new(Mutex::new(HashMap::new())),
                qq_chat_agent: None,
                result_mode: ToolResultMode::JsonObject,
                builtin_executor: Some(zihuan_ims_service::qq_tool_subgraph_hooks::image_understand_executor()),
                progress_notifier: Some(zihuan_ims_service::qq_tool_subgraph_hooks::qq_progress_notifier()),
            },
        });
    }

    brain
}

fn handle_tool_calling_result(
    agent_name: &str,
    output_messages: Vec<LLMMessage>,
    stop_reason: ToolCallingStopReason,
) -> Result<Vec<LLMMessage>> {
    match stop_reason {
        ToolCallingStopReason::Done => Ok(output_messages),
        ToolCallingStopReason::TransportError(content) => Err(zihuan_core::string_error!(
            "chat stream LLM request failed for '{}': {}",
            agent_name, content
        )),
        ToolCallingStopReason::MaxIterationsReached => Err(zihuan_core::string_error!(
            "chat stream exceeded max tool iterations ({MAX_TOOL_ITERATIONS}) for '{}'",
            agent_name
        )),
        ToolCallingStopReason::AwaitUserInput(request) | ToolCallingStopReason::ToolCallLimitReached(request) => Ok(output_messages
            .into_iter()
            .chain(std::iter::once(LLMMessage::assistant_text(format!(
                "需要用户补充信息: {}",
                request.question
            ))))
            .collect()),
    }
}

fn handle_tool_calling_result_with_reason(
    agent_name: &str,
    output_messages: Vec<LLMMessage>,
    stop_reason: ToolCallingStopReason,
) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)> {
    match &stop_reason {
        ToolCallingStopReason::Done | ToolCallingStopReason::AwaitUserInput(_) | ToolCallingStopReason::ToolCallLimitReached(_) => Ok((output_messages, stop_reason)),
        ToolCallingStopReason::TransportError(content) => Err(zihuan_core::string_error!(
            "chat stream LLM request failed for '{}': {}",
            agent_name, content
        )),
        ToolCallingStopReason::MaxIterationsReached => Err(zihuan_core::string_error!(
            "chat stream exceeded max tool iterations ({MAX_TOOL_ITERATIONS}) for '{}'",
            agent_name
        )),
    }
}

fn run_agent_tool_calling(
    agent: &RoleServiceConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn Tool>>,
    tool_definitions: Vec<ToolDefinition>,
    messages: Vec<LLMMessage>,
) -> Result<Vec<LLMMessage>> {
    let brain = build_tool_calling_engine(agent, llm, default_tools, tool_definitions);
    let (output_messages, stop_reason) = brain.run(messages);
    handle_tool_calling_result(&agent.name, output_messages, stop_reason)
}

async fn run_agent_tool_calling_streaming(
    agent: &RoleServiceConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn Tool>>,
    tool_definitions: Vec<ToolDefinition>,
    messages: Vec<LLMMessage>,
    token_tx: mpsc::UnboundedSender<StreamToken>,
    observer: Option<Arc<dyn ToolCallingObserver>>,
) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)> {
    let mut brain = build_tool_calling_engine(agent, llm, default_tools, tool_definitions);
    if let Some(obs) = observer {
        brain.set_observer(obs);
    }
    let (output_messages, stop_reason) = brain.run_streaming(messages, token_tx).await;
    handle_tool_calling_result_with_reason(&agent.name, output_messages, stop_reason)
}
