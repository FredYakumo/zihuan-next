use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::inference::{InferenceToolContext, InferenceToolProvider};
use super::qq_chat_agent_core::{
    build_info_brain_tools, QqAgentReplyBatchBuilder, QqChatAgentService, QqChatAgentServiceConfig,
};
use super::qq_chat_agent_msg_send::build_reply_batch_builder as build_unified_reply_batch_builder;
use super::{AgentManager, AgentRuntimeState, AgentRuntimeStatus};
use crate::agent::qq_chat_agent_inbox::{QqChatAgentInbox, QqChatAgentSupervisorEvent};
use crate::agent::tool_definitions::build_enabled_tool_definitions;
use crate::resource_resolver::{
    build_embedding_model, build_llm_model, resolve_llm_service_config,
    resolve_local_embedding_model_name,
};
use chrono::Local;
use ims_bot_adapter::adapter::BotAdapter;
use ims_bot_adapter::event::EventHandler;
use ims_bot_adapter::{build_ims_bot_adapter, parse_ims_bot_adapter_connection};
use log::{error, info, warn};
use model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use model_inference::system_config::{load_llm_refs, AgentConfig, LlmRefConfig};
use storage_handler::{
    build_mysql_ref, build_relational_db_connection_for_connection, build_s3_ref,
    build_weaviate_ref, build_web_search_engine_ref, find_connection, ConnectionConfig,
    ConnectionKind, WeaviateCollectionSchema,
};
use tokio::task::JoinHandle;
use zihuan_agent::brain::BrainTool;
use zihuan_core::agent_config::QqChatAgentConfig;
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::OpenAIMessage;
use zihuan_core::rag::WebSearchEngineRef;
use zihuan_core::runtime::block_async;
use zihuan_core::task_context::AgentTaskRuntime;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;
use zihuan_graph_engine::data_value::{OpenAIMessageSessionCacheRef, SessionStateRef};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::message_restore::{register_mysql_ref, register_rdb_pool};
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_nlp::{build_segmenter, TextSegmenter};

fn build_reply_batch_builder(segmenter: Arc<dyn TextSegmenter>) -> QqAgentReplyBatchBuilder {
    build_unified_reply_batch_builder(segmenter)
}

#[doc(hidden)]
pub fn expand_message_event_for_tool_input(
    event: &ims_bot_adapter::models::event_model::MessageEvent,
) -> ims_bot_adapter::models::event_model::MessageEvent {
    super::qq_chat_agent_core::expand_event_for_inference(event)
}

#[derive(Clone)]
struct QqLoadedInferenceResources {
    bot_name: String,
    default_tools_enabled: HashMap<String, bool>,
    web_search_engine_ref: Option<Arc<WebSearchEngineRef>>,
    mysql_ref: Option<Arc<MySqlConfig>>,
    s3_ref: Option<Arc<S3Ref>>,
    weaviate_image_ref: Option<Arc<WeaviateRef>>,
    weaviate_memory_ref: Option<Arc<WeaviateRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
    memory_llm: Option<Arc<dyn LLMBase>>,
}

pub struct QqInferenceToolProvider {
    resources: QqLoadedInferenceResources,
    tool_definitions: Vec<BrainToolDefinition>,
}

impl InferenceToolProvider for QqInferenceToolProvider {
    fn augment_messages(&self, messages: &mut Vec<OpenAIMessage>, _context: &InferenceToolContext) {
        messages.insert(
            0,
            OpenAIMessage::system(format!(
                "你是 {}。请保持回答简洁、友好、准确；当可调用工具时优先使用工具获取事实。",
                self.resources.bot_name
            )),
        );
    }

    fn build_default_tools(&self, context: &InferenceToolContext) -> Vec<Box<dyn BrainTool>> {
        build_info_brain_tools(
            &self.resources.default_tools_enabled,
            self.resources.web_search_engine_ref.clone(),
            self.resources.mysql_ref.clone(),
            self.resources.s3_ref.clone(),
            self.resources.weaviate_image_ref.clone(),
            self.resources.weaviate_memory_ref.clone(),
            self.resources.embedding_model.clone(),
            self.resources.memory_llm.clone(),
            storage_handler::AgentMemoryAccessContext::default(),
            context.last_user_text.clone(),
        )
    }

    fn tool_definitions(&self) -> Vec<BrainToolDefinition> {
        self.tool_definitions.clone()
    }
}

pub fn load_inference_tool_provider(
    agent: &AgentConfig,
    config: &QqChatAgentConfig,
    connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    Ok(Arc::new(QqInferenceToolProvider {
        resources: load_qq_resources(agent, config, connections)?,
        tool_definitions: build_enabled_tool_definitions(&agent.tools)?,
    }))
}

fn load_qq_resources(
    agent: &AgentConfig,
    config: &QqChatAgentConfig,
    connections: &[ConnectionConfig],
) -> Result<QqLoadedInferenceResources> {
    let web_search_engine_ref = build_web_search_engine_ref(
        if config.web_search_engine_connection_id.trim().is_empty() {
            None
        } else {
            Some(config.web_search_engine_connection_id.as_str())
        },
        connections,
    )
    .unwrap_or_else(|e| {
        warn!("[inference][qq_agent] web search engine connection unavailable: {e}");
        None
    });

    let mysql_ref = build_agent_mysql_ref(config, connections, &agent.name)?;
    let s3_ref = block_async(build_s3_ref(
        config.rustfs_connection_id.as_deref(),
        connections,
    ))
    .unwrap_or_else(|e| {
        warn!("[inference][qq_agent] rustfs connection unavailable: {e}");
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
            Some(WeaviateCollectionSchema::ImageSemantic),
        )
    })
    .unwrap_or_else(|e| {
        warn!("[inference][qq_agent] weaviate image connection unavailable: {e}");
        None
    });
    let weaviate_memory_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(
            config
                .weaviate_memory_connection_id
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
            connections,
            Some(WeaviateCollectionSchema::AgentMemory),
        )
    })
    .unwrap_or_else(|e| {
        warn!("[inference][qq_agent] weaviate memory connection unavailable: {e}");
        None
    });

    let embedding_model = if let Some(model_ref_id) = config.embedding_model_ref_id.as_deref() {
        let llm_refs = model_inference::system_config::load_llm_refs().unwrap_or_default();
        match resolve_local_embedding_model_name(Some(model_ref_id), &llm_refs, &agent.name) {
            Ok(Some(_)) => block_async(
                RuntimeEmbeddingModelManager::shared().get_or_create_embedding_model(model_ref_id),
            )
            .ok(),
            Ok(None) => None,
            Err(err) => {
                warn!("[inference][qq_agent] embedding model ref unavailable: {err}");
                None
            }
        }
    } else {
        config.embedding.as_ref().map(build_embedding_model)
    };
    let memory_llm = config
        .llm_ref_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|llm_ref_id| resolve_llm_service_config(Some(llm_ref_id), &load_llm_refs().unwrap_or_default(), &agent.name))
        .transpose()?
        .map(|llm_config| build_llm_model(&llm_config))
        .transpose()
        .unwrap_or_else(|err| {
            warn!("[inference][qq_agent] memory llm unavailable: {err}");
            None
        });

    Ok(QqLoadedInferenceResources {
        bot_name: if config.bot_name.trim().is_empty() {
            agent.name.clone()
        } else {
            config.bot_name.clone()
        },
        default_tools_enabled: config.default_tools_enabled.clone(),
        web_search_engine_ref,
        mysql_ref,
        s3_ref,
        weaviate_image_ref,
        weaviate_memory_ref,
        embedding_model,
        memory_llm,
    })
}

fn build_agent_mysql_ref(
    config: &QqChatAgentConfig,
    connections: &[ConnectionConfig],
    agent_name: &str,
) -> Result<Option<Arc<MySqlConfig>>> {
    let Some(connection_id) = config.resolved_rdb_id() else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    if !matches!(connection.kind, ConnectionKind::Mysql(_)) {
        return Ok(None);
    }

    block_async(build_mysql_ref(Some(connection_id), connections)).map_err(|err| {
        Error::ValidationError(format!(
            "agent '{}' failed to initialize mysql dependency from rdb_id='{}': {}",
            agent_name, connection_id, err
        ))
    })
}

/// Purpose: Bootstrap and launch a long-running QQ chat agent instance.
///
/// Resolves all runtime dependencies (`llm`, `embedding_model`, `tavily`, `s3_ref`,
/// `mysql_ref`, `weaviate_image_ref`), wires the IMS bot adapter event handler
/// through an inbox queue, then spawns a background task that runs the
/// `BotAdapter::start` loop until exit.
///
/// Called when the service layer starts an agent whose type is QQ chat —
/// typically from `AgentManager::start_agent` after validating the agent config.
///
/// Call chain:
///   `AgentManager::start_agent` → `QqChatAgent::spawn`
///     → build deps → register `EventHandler` on bot adapter
///     → `tokio::spawn`(`BotAdapter::start`) → `handle_event` per incoming message
///     → `on_finish` callback on exit
pub async fn spawn(
    manager: &AgentManager,
    agent: AgentConfig,
    config: QqChatAgentConfig,
    connections: Vec<ConnectionConfig>,
    on_finish: super::OnFinishShared,
    task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
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

    let llm_config =
        resolve_llm_service_config(config.llm_ref_id.as_deref(), &llm_refs, &agent.name)?;
    let llm = build_llm_model(&llm_config)?;
    let math_programming_llm_config = resolve_llm_service_config(
        config
            .math_programming_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        &llm_refs,
        &agent.name,
    )?;
    let math_programming_llm = build_llm_model(&math_programming_llm_config)?;
    let natural_language_reply_llm_config = resolve_llm_service_config(
        config
            .natural_language_reply_llm_ref_id
            .as_deref(),
        &llm_refs,
        &agent.name,
    )?;
    let natural_language_reply_llm = build_llm_model(&natural_language_reply_llm_config)?;
    let embedding_model = if let Some(model_ref_id) = config.embedding_model_ref_id.as_deref() {
        let model_name =
            resolve_local_embedding_model_name(Some(model_ref_id), &llm_refs, &agent.name)?;
        match model_name {
            Some(_) => Some(
                RuntimeEmbeddingModelManager::shared()
                    .get_or_create_embedding_model(model_ref_id)
                    .await?,
            ),
            None => None,
        }
    } else {
        config.embedding.as_ref().map(build_embedding_model)
    };
    let web_search_engine =
        build_web_search_engine_ref(Some(&config.web_search_engine_connection_id), &connections)?
            .ok_or_else(|| {
            Error::ValidationError("missing web search engine connection".to_string())
        })?;
    let object_storage = build_s3_ref(config.rustfs_connection_id.as_deref(), &connections).await?;
    let rdb_pool = match config.resolved_rdb_id() {
        Some(connection_id) => {
            Some(build_relational_db_connection_for_connection(connection_id, &connections).await?)
        }
        None => None,
    };
    let mysql_ref = build_agent_mysql_ref(&config, &connections, &agent.name)?;
    let redis_ref = resolve_inbox_redis_ref(&connections)?;
    let weaviate_image_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(
            config.weaviate_image_connection_id.as_deref(),
            &connections,
            Some(WeaviateCollectionSchema::ImageSemantic),
        )
    })?;
    let weaviate_memory_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(
            config
                .weaviate_memory_connection_id
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
            &connections,
            Some(WeaviateCollectionSchema::AgentMemory),
        )
    })?;
    let tool_definitions = build_enabled_tool_definitions(&agent.tools)?;
    let tokenizer_segmenter = resolve_tokenizer_segmenter(&config, &connections);

    if let Some(ref mysql) = mysql_ref {
        register_mysql_ref(mysql.clone());
    }
    if let Some(ref rdb_pool) = rdb_pool {
        register_rdb_pool(rdb_pool.clone());
    }

    let service = Arc::new(QqChatAgentService::new(QqChatAgentServiceConfig {
        agent_id: agent.id.clone(),
        qq_chat_config: config.clone(),
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
        math_programming_llm,
        natural_language_reply_llm,
        main_llm_display_name: resolve_llm_ref_display_name(
            config.llm_ref_id.as_deref(),
            &llm_refs,
            &llm_config.model_name,
        ),
        math_programming_llm_display_name: resolve_llm_ref_display_name(
            config
                .math_programming_llm_ref_id
                .as_deref()
                .or(config.llm_ref_id.as_deref()),
            &llm_refs,
            &math_programming_llm_config.model_name,
        ),
        natural_language_reply_llm_display_name: resolve_llm_ref_display_name(
            config.natural_language_reply_llm_ref_id.as_deref(),
            &llm_refs,
            &natural_language_reply_llm_config.model_name,
        ),
        rdb_pool,
        mysql_ref,
        weaviate_image_ref,
        weaviate_memory_ref,
        embedding_model,
        web_search_engine,
        s3_ref: object_storage.clone(),
        max_message_length: config.max_message_length,
        compact_context_length: config.compact_context_length,
        max_steer_count: config.max_steer_count,
        reply_batch_builder: Some(build_reply_batch_builder(tokenizer_segmenter)),
        default_tools_enabled: config.default_tools_enabled.clone(),
        shared_inputs: Vec::<FunctionPortDef>::new(),
        tool_definitions,
        shared_runtime_values: HashMap::new(),
        session_state_store: Arc::new(Mutex::new(HashMap::new())),
        task_runtime,
    })?);

    let adapter = build_ims_bot_adapter(&ims_bot_adapter_connection, object_storage).await;

    let inbox = QqChatAgentInbox::new(
        Arc::clone(&service),
        adapter.clone(),
        redis_ref,
        &agent.id,
        config.event_handler_threads,
    );

    {
        let inbox = inbox.clone();
        let handler: EventHandler = Arc::new(move |event| {
            let event = event.clone();
            let inbox = inbox.clone();
            Box::pin(async move {
                let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                inbox.enqueue(event, time).await?;
                Ok(())
            })
        });
        adapter.lock().await.register_event_handler(handler);
    }

    let manager = manager.clone();
    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();
    Ok(tokio::spawn(async move {
        info!("[service] starting QQ chat agent '{}'", agent_name);
        let mut tasks = tokio::task::JoinSet::new();
        inbox.spawn_consumers(&mut tasks);
        tasks.spawn(async move {
            match BotAdapter::start(adapter).await {
                Ok(()) => QqChatAgentSupervisorEvent::AdapterFinished {
                    success: true,
                    error_msg: None,
                },
                Err(err) => QqChatAgentSupervisorEvent::AdapterFinished {
                    success: false,
                    error_msg: Some(err.to_string()),
                },
            }
        });

        let mut adapter_result: Option<(bool, Option<String>)> = None;
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(QqChatAgentSupervisorEvent::AdapterFinished { success, error_msg }) => {
                    adapter_result = Some((success, error_msg));
                    inbox.request_shutdown();
                }
                Ok(QqChatAgentSupervisorEvent::RedisConsumerFinished) => {
                    if adapter_result.is_none() {
                        warn!("[service][qq_agent] a Redis inbox consumer exited unexpectedly");
                    }
                }
                Ok(QqChatAgentSupervisorEvent::MemoryConsumerFinished) => {
                    if adapter_result.is_none() {
                        warn!("[service][qq_agent] a memory inbox consumer exited unexpectedly");
                    }
                }
                Err(err) => {
                    error!("[service][qq_agent] inbox task join failed: {err}");
                }
            }
        }
        let (success, error_msg) = adapter_result.unwrap_or_else(|| {
            (
                false,
                Some("QQ chat agent task set ended unexpectedly".to_string()),
            )
        });

        if success {
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
        } else {
            let msg = error_msg
                .clone()
                .unwrap_or_else(|| "QQ chat agent exited unexpectedly".to_string());
            error!(
                "[service] QQ chat agent '{}' exited with error: {}",
                agent_name, msg
            );
            manager.update_state(
                &agent_id,
                AgentRuntimeState {
                    instance_id: None,
                    status: AgentRuntimeStatus::Error,
                    started_at: None,
                    last_error: Some(msg.clone()),
                },
            );
        }
        if let Some(cb) = on_finish.lock().unwrap().take() {
            cb(success, error_msg);
        }
    }))
}

fn resolve_tokenizer_segmenter(
    config: &QqChatAgentConfig,
    connections: &[ConnectionConfig],
) -> Arc<dyn TextSegmenter> {
    let tokenizer_path = config
        .tokenizer_connection_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|connection_id| match find_connection(connections, connection_id) {
            Ok(connection) => match &connection.kind {
                ConnectionKind::Tokenizer(tokenizer) => Some(Path::new("models/tokenizer").join(
                    tokenizer.model_name.trim(),
                )),
                _ => {
                    warn!(
                        "[service][qq_agent] tokenizer_connection_id='{}' points to non-tokenizer connection '{}', fallback to punctuation segmenter",
                        connection_id,
                        connection.name
                    );
                    None
                }
            },
            Err(err) => {
                warn!(
                    "[service][qq_agent] tokenizer connection not found for id='{}': {}, fallback to punctuation segmenter",
                    connection_id,
                    err
                );
                None
            }
        })
        .map(|model_dir| model_dir.join("tokenizer.json"));

    build_segmenter(tokenizer_path.as_deref())
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

fn resolve_inbox_redis_ref(
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<zihuan_graph_engine::data_value::RedisConfig>>> {
    let redis_connection_id = connections.iter().find_map(|connection| {
        if connection.enabled && matches!(connection.kind, ConnectionKind::Redis(_)) {
            Some(connection.id.as_str())
        } else {
            None
        }
    });
    storage_handler::build_redis_ref(redis_connection_id, connections)
}
