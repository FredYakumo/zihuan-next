use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::inference::{InferenceToolContext, InferenceToolProvider};
use super::qq_chat_agent_core::{
    build_info_brain_tools, expand_messages_for_inference, prepare_current_turn_user_input,
    prepare_current_turn_user_input_from_event, QqAgentReplyBatchBuilder, QqChatAgent,
    QqChatAgentContext, QqChatAgentService, QqChatAgentServiceConfig, QqChatTaskTrace, LOG_PREFIX,
    LOG_TEXT_PREVIEW_CHARS,
};
use super::qq_chat_agent_ignore_store::should_ignore_message_blocking;
use super::qq_chat_agent_msg_send::build_reply_batch_builder as build_unified_reply_batch_builder;
use super::{AgentManager, AgentRuntimeState, AgentRuntimeStatus};
use crate::agent::qq_chat_agent_inbox::{QqChatAgentInbox, QqChatAgentSupervisorEvent};
use crate::agent::tool_definitions::build_enabled_tool_definitions;
use crate::resource_resolver::{
    build_embedding_model, build_llm_model, resolve_llm_service_config,
    resolve_local_embedding_model_name,
};
use crate::storage::qq_chat_history_store::{conversation_history_key, load_history};
use crate::storage::qq_chat_session_store::{release_session, try_claim_session};
use chrono::Local;
use ims_bot_adapter::adapter::BotAdapter;
use ims_bot_adapter::event::EventHandler;
use ims_bot_adapter::message_helpers::get_bot_id;
use ims_bot_adapter::models::event_model::MessageType;
use ims_bot_adapter::models::message::MessageProp;
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
use zihuan_agent::session_state::QqChatAgentSessionState;
use zihuan_core::agent_config::QqChatAgentConfig;
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::LLMMessage;
use zihuan_core::rag::WebSearchEngineRef;
use zihuan_core::runtime::block_async;
use zihuan_core::steer::PendingSteerEvent;
use zihuan_core::task_context::{
    scope_task_id, scope_task_runtime, AgentTaskRequest, AgentTaskResult, AgentTaskRuntime,
    AgentTaskStatus,
};
use zihuan_core::utils::string_utils::shorten_text;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;
use zihuan_graph_engine::data_value::{LLMMessageSessionCacheRef, SessionStateRef};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::message_persistence::persist_message_event;
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
    let mut expanded = event.clone();
    expanded.message_list = expand_messages_for_inference(&event.message_list);
    expanded
}

#[doc(hidden)]
pub use crate::qq_chat_user_input::PreparedCurrentTurnUserInput;

#[doc(hidden)]
pub fn prepare_message_event_user_input_for_test(
    event: &ims_bot_adapter::models::event_model::MessageEvent,
    bot_id: &str,
    bot_name: &str,
) -> PreparedCurrentTurnUserInput {
    prepare_current_turn_user_input_from_event(event, bot_id, bot_name, None)
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
    fn augment_messages(&self, messages: &mut Vec<LLMMessage>, _context: &InferenceToolContext) {
        messages.insert(
            0,
            LLMMessage::system(format!(
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
        .map(|llm_ref_id| {
            resolve_llm_service_config(
                Some(llm_ref_id),
                &load_llm_refs().unwrap_or_default(),
                &agent.name,
            )
        })
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
        config.natural_language_reply_llm_ref_id.as_deref(),
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
        cache: Arc::new(LLMMessageSessionCacheRef::new(format!(
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
        session_state_store: Arc::new(Mutex::new(QqChatAgentSessionState::default())),
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

impl QqChatAgent {
    /// Entry point for handling a single inbound QQ message event.
    ///
    /// The flow is:
    /// - **Validation** — persists the message and checks ignore rules.
    /// - **Group mention filter** — silently drops group messages that do not `@` the bot.
    /// - **Session claim** — tries to acquire a per-sender session lock. If the session is busy,
    ///   the message is enqueued as a steer event instead.
    /// - **Task tracking** — starts a runtime task (if available) and builds a [`QqChatTaskTrace`].
    /// - **Delegation** — forwards to [`handle_claimed`] for the actual brain loop and reply.
    /// - **Cleanup** — releases the session lock, finalizes steer state, and marks the task
    ///   as completed or failed.
    pub(crate) fn handle(
        &self,
        event: &ims_bot_adapter::models::MessageEvent,
        time: &str,
        agent_id: &str,
        session: &Arc<SessionStateRef>,
        user_ip: Option<String>,
        ctx: &QqChatAgentContext<'_>,
    ) -> Result<()> {
        let is_group = event.message_type == MessageType::Group;
        let sender_id = event.sender.user_id.to_string();
        let target_id = if is_group {
            event
                .group_id
                .ok_or_else(|| self.wrap_err("group_id missing on group message"))?
                .to_string()
        } else {
            sender_id.clone()
        };

        info!(
            "{LOG_PREFIX} Handling {} message: message_id={} sender={} target={}",
            if is_group { "group" } else { "private" },
            event.message_id,
            sender_id,
            target_id
        );

        if let Err(err) = persist_message_event(event, ctx.rdb_pool, ctx.mysql_ref, None) {
            warn!("{LOG_PREFIX} Message persistence failed: {err}");
        }

        if let Some(rdb_pool) = ctx.rdb_pool {
            let group_id_text = event.group_id.map(|value| value.to_string());
            if should_ignore_message_blocking(
                rdb_pool,
                agent_id,
                &sender_id,
                group_id_text.as_deref(),
            )? {
                info!(
                    "{LOG_PREFIX} Ignored inbound message: message_id={} sender={} group={:?}",
                    event.message_id, sender_id, event.group_id
                );
                return Ok(());
            }
        }

        if is_group {
            let bot_id = get_bot_id(ctx.adapter);
            let msg_prop = MessageProp::from_messages_with_bot_name(
                &event.message_list,
                Some(&bot_id),
                Some(ctx.bot_name),
            );
            if !msg_prop.is_at_me {
                return Ok(());
            }
        }

        let (claimed, claim_token) = try_claim_session(session, &sender_id);
        if !claimed {
            let bot_id = get_bot_id(ctx.adapter);
            let prepared_input = prepare_current_turn_user_input(
                event,
                ctx.adapter,
                &bot_id,
                ctx.bot_name,
                ctx.s3_ref,
            );
            let mut inference_event = prepared_input.event.clone();
            inference_event.message_list =
                expand_messages_for_inference(&prepared_input.event.message_list);
            let current_message = prepare_current_turn_user_input_from_event(
                &inference_event,
                &bot_id,
                ctx.bot_name,
                ctx.s3_ref,
            )
            .text;
            if let Some(command_registry) = crate::command::global_command_registry() {
                let cmd_ctx = self.build_command_context(
                    &sender_id,
                    &target_id,
                    is_group,
                    inference_event.group_id,
                );
                if let Some(preview) = command_registry.preview(&cmd_ctx, &current_message) {
                    if preview.definition.allow_steer_bypass && preview.passthrough_text.is_none() {
                        info!(
                            "{LOG_PREFIX} Session busy for {sender_id}, executing command via steer bypass: message_id={} command=/{}",
                            event.message_id,
                            preview.definition.name
                        );
                        if let Some(dispatch_result) =
                            command_registry.dispatch(&cmd_ctx, &current_message)
                        {
                            let history_key = conversation_history_key(
                                &bot_id,
                                &sender_id,
                                is_group,
                                inference_event.group_id,
                            );
                            let legacy_history_key = sender_id.to_string();
                            let mut history =
                                load_history(ctx.cache, &history_key, &legacy_history_key);
                            let trace = QqChatTaskTrace::new(Local::now());
                            self.execute_command_dispatch(
                                &trace,
                                &cmd_ctx,
                                dispatch_result,
                                &prepared_input.event,
                                &inference_event,
                                &sender_id,
                                &target_id,
                                &bot_id,
                                &mut history,
                                ctx,
                            )?;
                            trace.finish_with_summary();
                            return Ok(());
                        }
                    } else {
                        info!(
                            "{LOG_PREFIX} Session busy for {sender_id}, command falls back to steer: message_id={} command=/{} allow_steer_bypass={} has_passthrough={}",
                            event.message_id,
                            preview.definition.name,
                            preview.definition.allow_steer_bypass,
                            preview.passthrough_text.is_some()
                        );
                    }
                }
            }
            let (accepted, queue_len, accepted_steer_count) = ctx.pending_steer.enqueue_with_limit(
                &sender_id,
                PendingSteerEvent {
                    event: prepared_input.event,
                    time: time.to_string(),
                },
                ctx.max_steer_count,
            );
            if accepted {
                info!(
                    "{LOG_PREFIX} Session busy for {sender_id}, enqueueing steer: message_id={} queue_len={} accepted_steer_count={}/{} message={}",
                    event.message_id,
                    queue_len,
                    accepted_steer_count,
                    ctx.max_steer_count,
                    shorten_text(&current_message, LOG_TEXT_PREVIEW_CHARS)
                );
            } else {
                warn!(
                    "{LOG_PREFIX} steer dropped for sender={} message_id={} because max steer count reached: accepted_steer_count={}/{} message={}",
                    sender_id,
                    event.message_id,
                    accepted_steer_count,
                    ctx.max_steer_count,
                    shorten_text(&current_message, LOG_TEXT_PREVIEW_CHARS)
                );
            }
            return Ok(());
        }

        ctx.pending_steer.ensure_session_entry(&sender_id);

        let task_created_at = Local::now();
        let task_handle = ctx.task_runtime.as_ref().map(|runtime| {
            runtime.start_task(AgentTaskRequest {
                task_name: format!("回复[{sender_id}]的消息"),
                agent_id: agent_id.to_string(),
                agent_name: ctx.bot_name.to_string(),
                user_ip,
                owner_id: Some(sender_id.to_string()),
                task_db_connection_id: ctx.task_db_connection_id.clone(),
            })
        });
        let trace = QqChatTaskTrace::new(task_created_at);
        let result = if let Some(task_handle) = task_handle.as_ref() {
            if let Some(task_runtime) = ctx.task_runtime.as_ref() {
                scope_task_runtime(Arc::clone(task_runtime), || {
                    scope_task_id(task_handle.task_id.clone(), || {
                        self.handle_claimed(
                            &trace, event, time, &sender_id, &target_id, is_group, ctx,
                        )
                    })
                })
            } else {
                scope_task_id(task_handle.task_id.clone(), || {
                    self.handle_claimed(&trace, event, time, &sender_id, &target_id, is_group, ctx)
                })
            }
        } else {
            self.handle_claimed(&trace, event, time, &sender_id, &target_id, is_group, ctx)
        };
        trace.finish_with_summary();

        release_session(session, &sender_id, claim_token);
        ctx.pending_steer.finish_session(&sender_id);
        if let Some(task_handle) = task_handle {
            match &result {
                Ok(report) => task_handle.finish(AgentTaskResult {
                    status: Some(AgentTaskStatus::Success),
                    result_summary: Some(report.result_summary.clone()),
                    error_message: None,
                }),
                Err(err) => task_handle.finish(AgentTaskResult {
                    status: Some(AgentTaskStatus::Failed),
                    result_summary: Some(format!("回复[{sender_id}]失败: {err}")),
                    error_message: Some(err.to_string()),
                }),
            }
        }
        result.map(|_| ())
    }
}
