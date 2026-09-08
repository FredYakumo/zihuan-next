use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::mpsc;
use zihuan_core::agent::service_config::RoleServiceConfig;
use zihuan_core::agent::tools::{
    Tool, ToolCallingEngine, ToolCallingObserver, ToolCallingStopReason, ToolExecutionOutput,
    ToolExecutionResource, ToolRunDuration, MAX_TOOL_ITERATIONS,
};
use zihuan_core::agent::{
    Agent, AgentCancellation, AgentContext, ContextCompactionEvent, ContextCompactionObserver,
};
use zihuan_core::config::llm_refs::{load_llm_refs, LlmRefConfig};
use zihuan_core::error::{Error, Result};
use zihuan_core::graph::tool_spec::ToolDefinition;
use zihuan_core::model_inference::inference_function::compact_message::{
    compact_message_history, compaction_threshold, estimate_messages_tokens,
};
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::model_inference::llm::{LLMMessage, MessageRole, StreamToken};
use zihuan_core::model_inference::message_content_utils::sanitize_messages_for_inference;
use zihuan_core::storage::ConnectionConfig;
use zihuan_core::system_config::current_context_compaction_percent;
use zihuan_core::tool_subgraph::{ToolResultMode, ToolSubgraphRunner};

use zihuan_core::agent::resource_provider::SharedAgentResourceProvider;
use zihuan_core::agent::resource_resolver::{build_llm_model, resolve_llm_service_config};
use zihuan_core::role::{RoleService, RoleServiceContext, RoleServiceDescriptor};

pub use zihuan_core::agent::inference_provider::{InferenceToolContext, InferenceToolProvider};

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
/// Configuration for constructing a [`RoleAgent`]. The model bindings are resolved by the
/// caller (from the role's configured LLM or a per-turn override); the agent is then built
/// via [`Agent::new`].
pub struct RoleAgentConfig {
    pub agent: RoleServiceConfig,
    pub model_name: String,
    pub llm: Arc<dyn LLMBase>,
    pub image_understand_llm: Option<Arc<dyn LLMBase>>,
    pub tools: Arc<dyn InferenceToolProvider>,
}

#[derive(Clone)]
/// The primary Agent assembled for a configured RoleService.
///
/// **Design:** A RoleAgent instance is a turn-scoped model binding: the heavy resources (the
/// configured tool provider) live behind an `Arc` shared across instances, while the LLM
/// bindings are cheap to replace via [`RoleAgent::with_llm_override`]. A per-turn agent is
/// constructed at inference time so the models used for that turn are fixed into the instance
/// the `Agent` trait is called on.
pub struct RoleAgent {
    agent: RoleServiceConfig,
    model_name: String,
    llm: Arc<dyn LLMBase>,
    image_understand_llm: Option<Arc<dyn LLMBase>>,
    tools: Arc<dyn InferenceToolProvider>,
}

impl RoleAgent {
    /// Validate the configuration and construct the agent from it.
    fn from_config(config: RoleAgentConfig) -> Result<Self> {
        if !config.agent.enabled {
            return Err(Error::ValidationError(format!(
                "agent '{}' is disabled",
                config.agent.name
            )));
        }
        Ok(Self {
            agent: config.agent,
            model_name: config.model_name,
            llm: config.llm,
            image_understand_llm: config.image_understand_llm,
            tools: config.tools,
        })
    }
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

    fn execute_with_outcome(
        &self,
        call_content: &str,
        arguments: &serde_json::Value,
    ) -> ToolExecutionOutput {
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

    fn execute_with_outcome(
        &self,
        call_content: &str,
        arguments: &serde_json::Value,
    ) -> ToolExecutionOutput {
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

    fn execution_resource(&self, arguments: &serde_json::Value) -> ToolExecutionResource {
        self.0.execution_resource(arguments)
    }

    fn requires_user_confirmation(&self, arguments: &serde_json::Value) -> bool {
        self.0.requires_user_confirmation(arguments)
    }
}

impl RoleAgent {
    /// The RoleService's own configured LLM, used by chat turns without a model override.
    pub fn llm(&self) -> Arc<dyn LLMBase> {
        Arc::clone(&self.llm)
    }

    /// The optional image-understanding model bound to this instance.
    pub fn image_understand_llm(&self) -> Option<Arc<dyn LLMBase>> {
        self.image_understand_llm.clone()
    }

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

    /// Resolve the role's configured model bindings and construct the agent.
    pub fn load_with_tools(
        agent: &RoleServiceConfig,
        llm_refs: &[LlmRefConfig],
        tools: Arc<dyn InferenceToolProvider>,
    ) -> Result<Self> {
        let llm_ref_id_owned =
            super::service_type_ext::primary_llm_ref_id(&agent.role_service_type);
        let llm_config =
            resolve_llm_service_config(llm_ref_id_owned.as_deref(), llm_refs, &agent.name)?;
        let model_name = llm_config.model_name.clone();
        let llm = build_llm_model(&llm_config)?;
        Self::new(RoleAgentConfig {
            agent: agent.clone(),
            model_name,
            llm,
            image_understand_llm: None,
            tools,
        })
    }

    /// Clone this agent with the given model bindings replacing the role's configured defaults.
    /// Cheap: only the model Arcs and the display name change; the tool provider is shared.
    pub fn with_llm_override(
        &self,
        llm: Arc<dyn LLMBase>,
        model_name: String,
        image_understand_llm: Option<Arc<dyn LLMBase>>,
    ) -> Self {
        Self {
            agent: self.agent.clone(),
            model_name,
            llm,
            image_understand_llm,
            tools: Arc::clone(&self.tools),
        }
    }

    pub fn agent(&self) -> &RoleServiceConfig {
        &self.agent
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Run one non-streaming turn against the instance's bound models.
    fn run_non_streaming(
        &self,
        messages: Vec<LLMMessage>,
        workspace_path: Option<String>,
        session_id: Option<String>,
    ) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)> {
        let context = build_inference_tool_context(
            &messages,
            workspace_path,
            session_id,
            Arc::clone(&self.llm),
            self.image_understand_llm.clone(),
        );

        let mut conversation = sanitize_messages_for_inference(messages);
        if conversation.is_empty() {
            return Err(Error::ValidationError(
                "messages must not be empty after sanitization".to_string(),
            ));
        }

        self.tools.augment_messages(&mut conversation, &context);
        let default_tools = self.tools.build_default_tools(&context);

        run_agent_tool_calling_with_reason(
            &self.agent,
            Arc::clone(&self.llm),
            default_tools,
            self.tools.tool_definitions(),
            conversation,
        )
    }

    /// Run one streamed turn against the instance's bound models.
    #[allow(clippy::too_many_arguments)]
    async fn run_streaming_core(
        &self,
        messages: Vec<LLMMessage>,
        token_tx: Option<mpsc::UnboundedSender<StreamToken>>,
        observer: Option<Arc<dyn ToolCallingObserver>>,
        compaction_observer: Option<ContextCompactionObserver>,
        workspace_path: Option<String>,
        session_id: Option<String>,
        cancellation: Option<Arc<dyn AgentCancellation>>,
    ) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)> {
        let context = build_inference_tool_context(
            &messages,
            workspace_path,
            session_id,
            Arc::clone(&self.llm),
            self.image_understand_llm.clone(),
        );

        let mut conversation = sanitize_messages_for_inference(messages);
        if conversation.is_empty() {
            return Err(Error::ValidationError(
                "messages must not be empty after sanitization".to_string(),
            ));
        }

        if super::service_type_ext::is_workspace_agent(&self.agent) {
            if let (Some(observer), Some(latest_user_index)) = (
                compaction_observer,
                conversation
                    .iter()
                    .rposition(|message| matches!(message.role, MessageRole::User)),
            ) {
                let latest_user_message = conversation.remove(latest_user_index);
                let estimated_tokens_before = estimate_messages_tokens(&conversation)
                    + estimate_messages_tokens(std::slice::from_ref(&latest_user_message));
                let threshold = compaction_threshold(
                    self.llm.context_length(),
                    current_context_compaction_percent(),
                );
                if estimated_tokens_before > threshold {
                    observer(ContextCompactionEvent::Started);
                    let started_at = Instant::now();
                    let compact_result = compact_message_history(
                        &self.llm,
                        conversation,
                        threshold,
                        &latest_user_message,
                    );
                    conversation = compact_result.messages;
                    if compact_result.did_compact {
                        observer(ContextCompactionEvent::Completed {
                            estimated_tokens_before,
                            estimated_tokens_after: compact_result.estimated_tokens_after
                                + estimate_messages_tokens(std::slice::from_ref(
                                    &latest_user_message,
                                )),
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
        let default_tools = self.tools.build_default_tools(&context);

        run_agent_tool_calling_streaming(
            &self.agent,
            Arc::clone(&self.llm),
            default_tools,
            self.tools.tool_definitions(),
            conversation,
            token_tx,
            observer,
            cancellation,
        )
        .await
    }
}

#[async_trait]
impl RoleService for RoleAgent {
    type Input = Vec<LLMMessage>;
    type Output = Vec<LLMMessage>;

    fn descriptor(&self) -> RoleServiceDescriptor {
        RoleServiceDescriptor {
            id: self.agent.id.clone(),
            name: self.agent.name.clone(),
            kind: super::service_type_ext::role_service_kind_of(&self.agent.role_service_type),
        }
    }

    async fn handle(
        &self,
        _context: RoleServiceContext,
        input: Self::Input,
    ) -> Result<Self::Output> {
        let (messages, _stop_reason) = self.run_non_streaming(input, None, None)?;
        Ok(messages)
    }
}

#[async_trait]
impl Agent for RoleAgent {
    type Config = RoleAgentConfig;
    type Input = Vec<LLMMessage>;
    type Output = (Vec<LLMMessage>, ToolCallingStopReason);

    fn name(&self) -> &str {
        &self.agent.name
    }

    fn llm(&self) -> Arc<dyn LLMBase> {
        Arc::clone(&self.llm)
    }

    fn new(config: Self::Config) -> Result<Self> {
        Self::from_config(config)
    }

    async fn run(&self, context: AgentContext, input: Self::Input) -> Result<Self::Output> {
        self.run_non_streaming(input, context.workspace_path, context.session_id)
    }

    async fn run_streaming(
        &self,
        context: AgentContext,
        input: Self::Input,
    ) -> Result<Self::Output> {
        self.run_streaming_core(
            input,
            context.token_tx,
            context.observer,
            context.compaction_observer,
            context.workspace_path,
            context.session_id,
            context.cancellation,
        )
        .await
    }
}

fn build_inference_tool_context(
    messages: &[LLMMessage],
    workspace_path: Option<String>,
    session_id: Option<String>,
    llm: Arc<dyn LLMBase>,
    image_understand_llm: Option<Arc<dyn LLMBase>>,
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
        image_understand_llm,
        image_media: messages
            .iter()
            .flat_map(|message| message.parts.iter())
            .filter_map(|part| match part {
                zihuan_core::message_part::MessagePart::Image { media } => Some(media.clone()),
                _ => None,
            })
            .collect(),
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

    let resources: Option<SharedAgentResourceProvider> =
        super::service_type_ext::optional_qq_chat(&agent.role_service_type)
            .map(|config| {
                zihuan_ims_service::qq_chat::resources::QqChatRoleServiceResources::new(config)
                    .into_shared()
            })
            .or_else(|| {
                super::service_type_ext::optional_workspace(&agent.role_service_type).map(|config| {
            zihuan_workspace_service::workspace_agent_service::WorkspaceRoleServiceResources::new(
                config,
            )
            .into_shared()
        })
            });

    for tool_def in tool_definitions {
        brain.add_tool(ServiceSubgraphTool {
            runner: ToolSubgraphRunner {
                node_id: format!("agent_inference_{}", agent.id),
                owner_node_type: "tool_calling".to_string(),
                shared_inputs: Vec::new(),
                definition: tool_def,
                shared_runtime_values: Arc::new(Mutex::new(HashMap::new())),
                resources: resources.clone(),
                result_mode: ToolResultMode::JsonObject,
                builtin_executor: Some(
                    zihuan_ims_service::qq_tool_subgraph_hooks::image_understand_executor(),
                ),
                progress_notifier: Some(
                    zihuan_ims_service::qq_tool_subgraph_hooks::qq_progress_notifier(),
                ),
            },
        });
    }

    brain
}

fn handle_tool_calling_result_with_reason(
    agent_name: &str,
    output_messages: Vec<LLMMessage>,
    stop_reason: ToolCallingStopReason,
) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)> {
    match &stop_reason {
        ToolCallingStopReason::Done
        | ToolCallingStopReason::AwaitUserInput(_)
        | ToolCallingStopReason::ToolCallLimitReached(_) => Ok((output_messages, stop_reason)),
        ToolCallingStopReason::TransportError(content) => Err(zihuan_core::string_error!(
            "chat stream LLM request failed for '{}': {}",
            agent_name,
            content
        )),
        ToolCallingStopReason::MaxIterationsReached => Err(zihuan_core::string_error!(
            "chat stream exceeded max tool iterations ({MAX_TOOL_ITERATIONS}) for '{}'",
            agent_name
        )),
    }
}

fn run_agent_tool_calling_with_reason(
    agent: &RoleServiceConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn Tool>>,
    tool_definitions: Vec<ToolDefinition>,
    messages: Vec<LLMMessage>,
) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)> {
    let brain = build_tool_calling_engine(agent, llm, default_tools, tool_definitions);
    let (output_messages, stop_reason) = brain.run(messages);
    handle_tool_calling_result_with_reason(&agent.name, output_messages, stop_reason)
}

async fn run_agent_tool_calling_streaming(
    agent: &RoleServiceConfig,
    llm: Arc<dyn LLMBase>,
    default_tools: Vec<Box<dyn Tool>>,
    tool_definitions: Vec<ToolDefinition>,
    messages: Vec<LLMMessage>,
    token_tx: Option<mpsc::UnboundedSender<StreamToken>>,
    observer: Option<Arc<dyn ToolCallingObserver>>,
    cancellation: Option<Arc<dyn AgentCancellation>>,
) -> Result<(Vec<LLMMessage>, ToolCallingStopReason)> {
    let mut brain = build_tool_calling_engine(agent, llm, default_tools, tool_definitions);
    if let Some(obs) = observer {
        brain.set_observer(obs);
    }
    if let Some(cancellation) = cancellation {
        brain.set_cancellation(cancellation);
    }
    // With no external token sink, stream into a dropped channel so the engine still runs its
    // streaming loop while observers keep receiving tool events.
    let token_tx = token_tx.unwrap_or_else(|| {
        let (tx, _rx) = mpsc::unbounded_channel();
        tx
    });
    let (output_messages, stop_reason) = brain.run_streaming(messages, token_tx).await;
    handle_tool_calling_result_with_reason(&agent.name, output_messages, stop_reason)
}
