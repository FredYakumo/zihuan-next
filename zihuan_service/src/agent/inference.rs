use std::collections::HashMap;
use std::sync::Arc;

use log::warn;
use storage_handler::{
    build_mysql_ref, build_tavily_ref, build_weaviate_ref, load_connections, ConnectionConfig,
};
use tokio::sync::mpsc;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{MessageRole, OpenAIMessage};
use zihuan_core::runtime::block_async;
use zihuan_graph_engine::data_value::{MySqlConfig, TavilyRef};
use zihuan_graph_engine::database::weaviate::WeaviateRef;
use zihuan_llm::agent::brain::{
    sanitize_messages_for_inference, Brain, BrainStopReason, BrainTool, MAX_TOOL_ITERATIONS,
};
use zihuan_llm::agent::qq_chat_agent::build_info_brain_tools;
use zihuan_llm::brain_tool::BrainToolDefinition;
use zihuan_llm::system_config::{AgentConfig, AgentType, LlmRefConfig};
use zihuan_llm::tool_subgraph::{ToolResultMode, ToolSubgraphRunner};

use super::qq_chat_agent::build_enabled_tool_definitions;
use crate::resource_resolver::{build_embedding_model, build_llm_model, resolve_llm_service_config, resolve_local_embedding_model_name};
use zihuan_llm::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;

#[derive(Clone)]
struct QqLoadedInferenceResources {
    bot_name: String,
    default_tools_enabled: HashMap<String, bool>,
    tavily_ref: Option<Arc<TavilyRef>>,
    mysql_ref: Option<Arc<MySqlConfig>>,
    weaviate_image_ref: Option<Arc<WeaviateRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
}

#[derive(Clone)]
pub struct LoadedInferenceAgent {
    agent: AgentConfig,
    model_name: String,
    llm: Arc<dyn LLMBase>,
    tool_definitions: Vec<BrainToolDefinition>,
    qq_resources: Option<QqLoadedInferenceResources>,
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
        let llm_refs = zihuan_llm::system_config::load_llm_refs()?;
        Self::load_with_refs(agent, &llm_refs, connections)
    }

    pub fn load_with_refs(
        agent: &AgentConfig,
        llm_refs: &[LlmRefConfig],
        connections: &[ConnectionConfig],
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
        let tool_definitions = build_enabled_tool_definitions(&agent.tools)?;
        let qq_resources = load_qq_resources(agent, connections);

        Ok(Self {
            agent: agent.clone(),
            model_name,
            llm,
            tool_definitions,
            qq_resources,
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
        let last_user_text = messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, MessageRole::User))
            .and_then(|m| m.content_text())
            .map(ToOwned::to_owned)
            .unwrap_or_default();

        let mut conversation = sanitize_messages_for_inference(messages);
        if conversation.is_empty() {
            return Err(Error::ValidationError(
                "messages must not be empty after sanitization".to_string(),
            ));
        }

        let mut default_brain_tools: Vec<Box<dyn BrainTool>> = Vec::new();
        if let Some(resources) = &self.qq_resources {
            conversation.insert(
                0,
                OpenAIMessage::system(format!(
                    "你是 {}。请保持回答简洁、友好、准确；当可调用工具时优先使用工具获取事实。",
                    resources.bot_name
                )),
            );
            default_brain_tools = build_info_brain_tools(
                &resources.default_tools_enabled,
                resources.tavily_ref.clone(),
                resources.mysql_ref.clone(),
                resources.weaviate_image_ref.clone(),
                resources.embedding_model.clone(),
                last_user_text,
            );
        }

        run_agent_brain(
            &self.agent,
            Arc::clone(&self.llm),
            default_brain_tools,
            self.tool_definitions.clone(),
            conversation,
        )
    }

    pub async fn infer_response_streaming_with_trace(
        &self,
        messages: Vec<OpenAIMessage>,
        token_tx: mpsc::UnboundedSender<String>,
    ) -> Result<Vec<OpenAIMessage>> {
        let last_user_text = messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, MessageRole::User))
            .and_then(|m| m.content_text())
            .map(ToOwned::to_owned)
            .unwrap_or_default();

        let mut conversation = sanitize_messages_for_inference(messages);
        if conversation.is_empty() {
            return Err(Error::ValidationError(
                "messages must not be empty after sanitization".to_string(),
            ));
        }

        let mut default_brain_tools: Vec<Box<dyn BrainTool>> = Vec::new();
        if let Some(resources) = &self.qq_resources {
            conversation.insert(
                0,
                OpenAIMessage::system(format!(
                    "你是 {}。请保持回答简洁、友好、准确；当可调用工具时优先使用工具获取事实。",
                    resources.bot_name
                )),
            );
            default_brain_tools = build_info_brain_tools(
                &resources.default_tools_enabled,
                resources.tavily_ref.clone(),
                resources.mysql_ref.clone(),
                resources.weaviate_image_ref.clone(),
                resources.embedding_model.clone(),
                last_user_text,
            );
        }

        run_agent_brain_streaming(
            &self.agent,
            Arc::clone(&self.llm),
            default_brain_tools,
            self.tool_definitions.clone(),
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

fn load_qq_resources(
    agent: &AgentConfig,
    connections: &[ConnectionConfig],
) -> Option<QqLoadedInferenceResources> {
    let AgentType::QqChat(config) = &agent.agent_type else {
        return None;
    };

    let tavily_ref = build_tavily_ref(
        if config.tavily_connection_id.trim().is_empty() {
            None
        } else {
            Some(config.tavily_connection_id.as_str())
        },
        connections,
    )
    .unwrap_or_else(|e| {
        warn!("[inference] tavily connection unavailable: {e}");
        None
    });

    let mysql_ref = block_async(build_mysql_ref(
        if config
            .mysql_connection_id
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            None
        } else {
            config.mysql_connection_id.as_deref()
        },
        connections,
    ))
    .unwrap_or_else(|e| {
        warn!("[inference] mysql connection unavailable: {e}");
        None
    });

    let weaviate_image_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(
            if config
                .weaviate_image_connection_id
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                None
            } else {
                config.weaviate_image_connection_id.as_deref()
            },
            connections,
            true,
        )
    })
    .unwrap_or_else(|e| {
        warn!("[inference] weaviate image connection unavailable: {e}");
        None
    });

    let embedding_model = if let Some(model_ref_id) = config.embedding_model_ref_id.as_deref() {
        let llm_refs = zihuan_llm::system_config::load_llm_refs().unwrap_or_default();
        match resolve_local_embedding_model_name(Some(model_ref_id), &llm_refs, &agent.name) {
            Ok(Some(_)) => block_async(
                RuntimeEmbeddingModelManager::shared().get_or_create_embedding_model(model_ref_id),
            )
            .ok(),
            Ok(None) => None,
            Err(err) => {
                warn!("[inference] embedding model ref unavailable: {err}");
                None
            }
        }
    } else {
        config.embedding.as_ref().map(build_embedding_model)
    };

    Some(QqLoadedInferenceResources {
        bot_name: if config.bot_name.trim().is_empty() {
            agent.name.clone()
        } else {
            config.bot_name.clone()
        },
        default_tools_enabled: config.default_tools_enabled.clone(),
        tavily_ref,
        mysql_ref,
        weaviate_image_ref,
        embedding_model,
    })
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
