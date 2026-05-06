use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Local;
use log::{error, info};
use tokio::task::JoinHandle;
use storage_handler::{
    build_mysql_ref, build_s3_ref, build_tavily_ref, build_weaviate_ref, find_connection,
    ConnectionConfig, ConnectionKind,
};
use ims_bot_adapter::adapter::BotAdapter;
use ims_bot_adapter::event::EventHandler;
use ims_bot_adapter::{build_ims_bot_adapter, parse_ims_bot_adapter_connection};
use zihuan_core::error::{Error, Result};
use zihuan_llm::agent::qq_chat_agent::{QqChatAgentService, QqChatAgentServiceConfig};
use zihuan_llm::brain_tool::BrainToolDefinition;
use zihuan_llm::system_config::{
    load_llm_refs, AgentConfig, AgentToolConfig, AgentToolType, NodeGraphToolConfig,
    QqChatAgentConfig,
};
use crate::resource_resolver::{build_embedding_model, build_llm_model, resolve_llm_service_config};
use zihuan_graph_engine::brain_tool_spec::QQ_AGENT_TOOL_OUTPUT_NAME;
use zihuan_graph_engine::data_value::{OpenAIMessageSessionCacheRef, SessionStateRef};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::message_restore::register_mysql_ref;
use zihuan_graph_engine::DataType;
use zihuan_graph_engine::data_value::EXECUTION_TASK_ID;
use super::{AgentManager, AgentRuntimeState, AgentRuntimeStatus};

pub async fn spawn(
    manager: &AgentManager,
    agent: AgentConfig,
    config: QqChatAgentConfig,
    connections: Vec<ConnectionConfig>,
    on_finish: super::OnFinishShared,
    task_id: String,
) -> Result<JoinHandle<()>> {
    let llm_refs = load_llm_refs()?;
    let bot_connection = find_connection(&connections, &config.ims_bot_adapter_connection_id)?;
    let ConnectionKind::BotAdapter(ims_bot_adapter_connection) = &bot_connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a bot adapter connection",
            bot_connection.name
        )));
    };
    let ims_bot_adapter_connection = parse_ims_bot_adapter_connection(ims_bot_adapter_connection)?;

    let llm_config = resolve_llm_service_config(
        config.llm_ref_id.as_deref(),
        config.llm.as_ref(),
        &llm_refs,
        &agent.name,
    )?;
    let llm = build_llm_model(&llm_config);
    let embedding_model = config.embedding.as_ref().map(build_embedding_model);
    let tavily = build_tavily_ref(Some(&config.tavily_connection_id), &connections)?
        .ok_or_else(|| Error::ValidationError("missing tavily connection".to_string()))?;
    let object_storage = build_s3_ref(config.rustfs_connection_id.as_deref(), &connections).await?;
    let mysql_ref = build_mysql_ref(config.mysql_connection_id.as_deref(), &connections).await?;
    let weaviate_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(config.weaviate_connection_id.as_deref(), &connections, false)
    })?;
    let weaviate_image_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(config.weaviate_image_connection_id.as_deref(), &connections, true)
    })?;
    let tool_definitions = build_enabled_tool_definitions(&agent.tools)?;

    if let Some(ref mysql) = mysql_ref {
        register_mysql_ref(mysql.clone());
    }

    let service = Arc::new(QqChatAgentService::new(QqChatAgentServiceConfig {
        node_id: format!("service_agent_{}", agent.id),
        node_name: agent.name.clone(),
        bot_name: if config.bot_name.trim().is_empty() {
            agent.name.clone()
        } else {
            config.bot_name.clone()
        },
        cache: Arc::new(OpenAIMessageSessionCacheRef::new(format!(
            "service_agent_cache_{}",
            agent.id
        ))),
        session: Arc::new(SessionStateRef::new(format!(
            "service_agent_session_{}",
            agent.id
        ))),
        llm,
        mysql_ref,
        weaviate_ref,
        weaviate_image_ref,
        embedding_model,
        tavily,
        max_message_length: config.max_message_length,
        compact_context_length: config.compact_context_length,
        default_tools_enabled: config.default_tools_enabled.clone(),
        shared_inputs: Vec::<FunctionPortDef>::new(),
        tool_definitions,
        shared_runtime_values: HashMap::new(),
    })?);

    let adapter = build_ims_bot_adapter(&ims_bot_adapter_connection, object_storage).await;

    {
        let service = Arc::clone(&service);
        let adapter_for_handler = adapter.clone();
        let handler: EventHandler = Arc::new(move |event| {
            let service = Arc::clone(&service);
            let adapter = adapter_for_handler.clone();
            let event = event.clone();
            Box::pin(async move {
                let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                // block_in_place allows blocking calls (reqwest::blocking::Client drop)
                // inside a tokio worker thread without panicking.
                if let Err(err) =
                    tokio::task::block_in_place(|| service.handle_event(&event, &adapter, &time))
                {
                    error!("[service][qq_agent] failed to handle message event: {err}");
                }
            })
        });
        adapter.lock().await.register_event_handler(handler);
    }

    let manager = manager.clone();
    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();
    Ok(tokio::spawn(EXECUTION_TASK_ID.scope(task_id, async move {
        info!("[service] starting QQ chat agent '{}'", agent_name);
        let (success, error_msg) = match BotAdapter::start(adapter).await {
            Ok(()) => {
                info!("[service] QQ chat agent '{}' stopped", agent_name);
                manager.update_state(
                    &agent_id,
                    AgentRuntimeState {
                        status: AgentRuntimeStatus::Stopped,
                        started_at: None,
                        last_error: None,
                    },
                );
                (true, None)
            }
            Err(err) => {
                error!("[service] QQ chat agent '{}' exited with error: {}", agent_name, err);
                let msg = err.to_string();
                manager.update_state(
                    &agent_id,
                    AgentRuntimeState {
                        status: AgentRuntimeStatus::Error,
                        started_at: None,
                        last_error: Some(msg.clone()),
                    },
                );
                (false, Some(msg))
            }
        };
        if let Some(cb) = on_finish.lock().unwrap().take() {
            cb(success, error_msg);
        }
    })))
}

pub fn build_enabled_tool_definitions(tools: &[AgentToolConfig]) -> Result<Vec<BrainToolDefinition>> {
    let mut definitions = Vec::new();
    for tool in tools.iter().filter(|tool| tool.enabled) {
        match &tool.tool_type {
            AgentToolType::NodeGraph(config) => {
                definitions.push(build_node_graph_tool_definition(tool, config)?);
            }
        }
    }
    Ok(definitions)
}

fn build_node_graph_tool_definition(
    tool: &AgentToolConfig,
    config: &NodeGraphToolConfig,
) -> Result<BrainToolDefinition> {
    let (subgraph, parameters, outputs) = match config {
        NodeGraphToolConfig::FilePath { path, parameters, outputs } => (
            load_graph_from_path(PathBuf::from(path))?,
            parameters.clone(),
            outputs.clone(),
        ),
        NodeGraphToolConfig::WorkflowSet { name, parameters, outputs } => (
            load_graph_from_path(PathBuf::from("workflow_set").join(format!("{name}.json")))?,
            parameters.clone(),
            outputs.clone(),
        ),
        NodeGraphToolConfig::InlineGraph { graph, parameters, outputs } => {
            (graph.clone(), parameters.clone(), outputs.clone())
        }
    };

    let outputs = if outputs.is_empty() {
        vec![FunctionPortDef {
            name: QQ_AGENT_TOOL_OUTPUT_NAME.to_string(),
            data_type: DataType::String,
        }]
    } else {
        outputs
    };

    Ok(BrainToolDefinition {
        id: tool.id.clone(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters,
        outputs,
        subgraph,
    })
}

fn load_graph_from_path(path: PathBuf) -> Result<zihuan_graph_engine::graph_io::NodeGraphDefinition> {
    if !path.exists() {
        return Err(Error::ValidationError(format!(
            "tool graph file not found: {}",
            path.display()
        )));
    }
    let loaded = zihuan_graph_engine::load_graph_definition_from_json_with_migration(&path)?;
    Ok(loaded.graph)
}

