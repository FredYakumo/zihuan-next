use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::{AgentManager, AgentRuntimeState, AgentRuntimeStatus};
use crate::resource_resolver::{
    build_embedding_model, build_llm_model, resolve_llm_service_config,
};
use chrono::Local;
use ims_bot_adapter::adapter::BotAdapter;
use ims_bot_adapter::event::EventHandler;
use ims_bot_adapter::{build_ims_bot_adapter, parse_ims_bot_adapter_connection};
use log::{error, info};
use storage_handler::{
    build_mysql_ref, build_s3_ref, build_tavily_ref, build_weaviate_ref, find_connection,
    ConnectionConfig, ConnectionKind,
};
use tokio::task::JoinHandle;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::data_value::EXECUTION_TASK_ID;
use zihuan_graph_engine::data_value::{OpenAIMessageSessionCacheRef, SessionStateRef};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::graph_boundary::{root_graph_to_tool_subgraph, sync_root_graph_io};
use zihuan_graph_engine::message_restore::register_mysql_ref;
use zihuan_graph_engine::DataType;
use zihuan_llm::agent::qq_chat_agent::{QqChatAgentService, QqChatAgentServiceConfig};
use zihuan_llm::brain_tool::BrainToolDefinition;
use zihuan_llm::system_config::{
    load_llm_refs, AgentConfig, AgentToolConfig, AgentToolType, LlmRefConfig, NodeGraphToolConfig,
    QqChatAgentConfig,
};

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
    let intent_llm_config = resolve_llm_service_config(
        config
            .intent_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        config.llm.as_ref(),
        &llm_refs,
        &agent.name,
    )?;
    let intent_llm = build_llm_model(&intent_llm_config);
    let math_programming_llm_config = resolve_llm_service_config(
        config
            .math_programming_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        config.llm.as_ref(),
        &llm_refs,
        &agent.name,
    )?;
    let math_programming_llm = build_llm_model(&math_programming_llm_config);
    let embedding_model = config.embedding.as_ref().map(build_embedding_model);
    let tavily = build_tavily_ref(Some(&config.tavily_connection_id), &connections)?
        .ok_or_else(|| Error::ValidationError("missing tavily connection".to_string()))?;
    let object_storage = build_s3_ref(config.rustfs_connection_id.as_deref(), &connections).await?;
    let mysql_ref = build_mysql_ref(config.mysql_connection_id.as_deref(), &connections).await?;
    let weaviate_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(
            config.weaviate_connection_id.as_deref(),
            &connections,
            false,
        )
    })?;
    let weaviate_image_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(
            config.weaviate_image_connection_id.as_deref(),
            &connections,
            true,
        )
    })?;
    let tool_definitions = build_enabled_tool_definitions(&agent.tools)?;

    if let Some(ref mysql) = mysql_ref {
        register_mysql_ref(mysql.clone());
    }

    let service = Arc::new(QqChatAgentService::new(QqChatAgentServiceConfig {
        node_id: format!("service_agent_{}", agent.id),
        bot_name: if config.bot_name.trim().is_empty() {
            agent.name.clone()
        } else {
            config.bot_name.clone()
        },
        system_prompt: config.system_prompt.clone(),
        cache: Arc::new(OpenAIMessageSessionCacheRef::new(format!(
            "service_agent_cache_{}",
            agent.id
        ))),
        session: Arc::new(SessionStateRef::new(format!(
            "service_agent_session_{}",
            agent.id
        ))),
        llm,
        intent_llm,
        math_programming_llm,
        main_llm_display_name: resolve_llm_ref_display_name(
            config.llm_ref_id.as_deref(),
            &llm_refs,
            &llm_config.model_name,
        ),
        intent_llm_display_name: resolve_llm_ref_display_name(
            config
                .intent_llm_ref_id
                .as_deref()
                .or(config.llm_ref_id.as_deref()),
            &llm_refs,
            &intent_llm_config.model_name,
        ),
        math_programming_llm_display_name: resolve_llm_ref_display_name(
            config
                .math_programming_llm_ref_id
                .as_deref()
                .or(config.llm_ref_id.as_deref()),
            &llm_refs,
            &math_programming_llm_config.model_name,
        ),
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
                        instance_id: None,
                        status: AgentRuntimeStatus::Stopped,
                        started_at: None,
                        last_error: None,
                    },
                );
                (true, None)
            }
            Err(err) => {
                error!(
                    "[service] QQ chat agent '{}' exited with error: {}",
                    agent_name, err
                );
                let msg = err.to_string();
                manager.update_state(
                    &agent_id,
                    AgentRuntimeState {
                        instance_id: None,
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

fn resolve_llm_ref_display_name(
    llm_ref_id: Option<&str>,
    llm_refs: &[LlmRefConfig],
    fallback_model_name: &str,
) -> String {
    llm_ref_id
        .and_then(|id| llm_refs.iter().find(|item| item.id == id))
        .map(|item| item.name.clone())
        .unwrap_or_else(|| fallback_model_name.to_string())
}

pub fn build_enabled_tool_definitions(
    tools: &[AgentToolConfig],
) -> Result<Vec<BrainToolDefinition>> {
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
    let (mut graph, parameters, outputs) = match config {
        NodeGraphToolConfig::FilePath {
            path,
            parameters,
            outputs,
        } => (
            load_graph_from_path(PathBuf::from(path))?,
            parameters.clone(),
            outputs.clone(),
        ),
        NodeGraphToolConfig::WorkflowSet {
            name,
            parameters,
            outputs,
        } => (
            load_graph_from_path(PathBuf::from("workflow_set").join(format!("{name}.json")))?,
            parameters.clone(),
            outputs.clone(),
        ),
        NodeGraphToolConfig::InlineGraph {
            graph,
            parameters,
            outputs,
        } => (graph.clone(), parameters.clone(), outputs.clone()),
    };

    sync_root_graph_io(&mut graph);
    validate_tool_graph_contract(tool, &graph, &parameters, &outputs)?;
    let subgraph = root_graph_to_tool_subgraph(&graph);

    Ok(BrainToolDefinition {
        id: tool.id.clone(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters,
        outputs,
        subgraph,
    })
}

fn load_graph_from_path(
    path: PathBuf,
) -> Result<zihuan_graph_engine::graph_io::NodeGraphDefinition> {
    if !path.exists() {
        return Err(Error::ValidationError(format!(
            "tool graph file not found: {}",
            path.display()
        )));
    }
    let loaded = zihuan_graph_engine::load_graph_definition_from_json_with_migration(&path)?;
    Ok(loaded.graph)
}

fn validate_tool_graph_contract(
    tool: &AgentToolConfig,
    graph: &zihuan_graph_engine::graph_io::NodeGraphDefinition,
    parameters: &[zihuan_llm::brain_tool::ToolParamDef],
    outputs: &[FunctionPortDef],
) -> Result<()> {
    if graph.graph_inputs.is_empty() {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 引用的节点图未定义输入列表",
            tool.name
        )));
    }
    if graph.graph_outputs.is_empty() {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 引用的节点图未定义输出列表",
            tool.name
        )));
    }
    if outputs.is_empty() {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 未定义 outputs，必须与节点图输出匹配",
            tool.name
        )));
    }

    for port in &graph.graph_inputs {
        if !matches!(
            port.data_type,
            DataType::Integer | DataType::Float | DataType::String | DataType::Boolean
        ) {
            return Err(Error::ValidationError(format!(
                "agent tool '{}' 的节点图输入 '{}' 类型必须是基础类型 int/float/string/boolean，实际为 {}",
                tool.name, port.name, port.data_type
            )));
        }
    }

    if !same_param_signature(parameters, &graph.graph_inputs) {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 的 parameters 与节点图输入定义不匹配",
            tool.name
        )));
    }
    if !same_port_signature(outputs, &graph.graph_outputs) {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 的 outputs 与节点图输出定义不匹配",
            tool.name
        )));
    }

    Ok(())
}

fn same_param_signature(
    parameters: &[zihuan_llm::brain_tool::ToolParamDef],
    inputs: &[FunctionPortDef],
) -> bool {
    parameters.len() == inputs.len()
        && parameters.iter().zip(inputs).all(|(param, input)| {
            param.name.trim() == input.name.trim() && param.data_type == input.data_type
        })
}

fn same_port_signature(left: &[FunctionPortDef], right: &[FunctionPortDef]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| {
            a.name.trim() == b.name.trim() && a.data_type == b.data_type
        })
}
