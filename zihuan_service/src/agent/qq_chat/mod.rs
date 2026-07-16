mod core;
mod emotion;
pub mod ignore_store;
mod inbox;
pub mod language_style_store;
pub(crate) mod logging;
pub mod message_rate_limit_store;
pub(crate) mod model;
pub(crate) mod msg_send;
pub mod privilege_gate;
pub mod privilege_store;
mod steer;
pub mod style_learner;
pub(crate) mod tool_quota;
pub mod tool_quota_store;
mod user_input;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use self::core::{
    build_info_brain_tools, expand_messages_for_inference, prepare_current_turn_user_input_from_event, QqChatTaskTrace,
    LOG_PREFIX, LOG_TEXT_PREVIEW_CHARS,
};
use self::ignore_store::should_ignore_message_blocking;
use self::inbox::QqChatAgentServiceInbox;
use self::language_style_store::get_applicable_language_style_blocking;
use self::message_rate_limit_store::{consume_message_rate_limit_blocking, MessageRateLimitBlockAction};
use self::model::{
    QqChatAgentService, QqChatAgentServiceContext, QqChatAgentServiceInner, QqChatAgentServiceRuntimeConfig,
    QqChatServiceReplyBatchBuilder, QqInferenceToolProvider, QqLoadedInferenceResources,
};
use self::msg_send::{
    build_reply_batch_builder as build_unified_reply_batch_builder, send_direct_notification_text_reply,
};
use super::inference::{InferenceToolContext, InferenceToolProvider};
use super::{AgentManager, AgentRuntimeState, AgentRuntimeStatus};
use crate::agent::tool_definitions::build_enabled_tool_definitions;
use crate::resource_resolver::{
    build_embedding_model, build_llm_model, resolve_llm_service_config, resolve_local_embedding_model_name,
};
use crate::storage::qq_chat_session_store::{release_session, try_claim_session};
use chrono::Local;
use ims_bot_adapter::active_adapter_manager::ActiveAdapterManager;
use ims_bot_adapter::event::EventHandler;
use ims_bot_adapter::message_helpers::get_bot_id;
use ims_bot_adapter::models::event_model::MessageType;
use ims_bot_adapter::models::message::MessageProp;
use log::{error, info, warn};
use model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use model_inference::system_config::{load_llm_refs, AgentConfig};
use storage_handler::{
    build_elasticsearch_ref, build_relational_db_connection_for_connection, build_s3_ref, build_weaviate_ref,
    build_web_search_engine_ref, find_connection, ConnectionConfig, ConnectionKind, WeaviateCollectionSchema,
};
use tokio::task::JoinHandle;
use zihuan_agent::brain::BrainTool;
use zihuan_agent::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::qq_chat::{current_qq_chat_agent_service_config, QqChatAgentServiceConfig};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::LLMMessage;
use zihuan_core::rag::WebSearchEngineRef;
use zihuan_core::runtime::block_async;
use zihuan_core::steer::PendingSteerEvent;
use zihuan_core::task_context::{
    scope_task_id, scope_task_runtime, AgentTaskRequest, AgentTaskResult, AgentTaskRuntime, AgentTaskStatus,
};
use zihuan_core::utils::string_utils::shorten_text;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;
use zihuan_graph_engine::data_value::{LLMMessageSessionCacheRef, SessionStateRef};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::message_persistence::persist_message_event;
use zihuan_graph_engine::message_restore::register_rdb_pool;
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_nlp::{build_segmenter, TextSegmenter};

use self::tool_quota::SessionToolQuotaState;

fn build_reply_batch_builder(segmenter: Arc<dyn TextSegmenter>) -> QqChatServiceReplyBatchBuilder {
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
pub use self::user_input::PreparedCurrentTurnUserInput;

#[doc(hidden)]
pub fn prepare_message_event_user_input_for_test(
    event: &ims_bot_adapter::models::event_model::MessageEvent,
    bot_id: &str,
    bot_name: &str,
) -> PreparedCurrentTurnUserInput {
    prepare_current_turn_user_input_from_event(event, bot_id, bot_name, None)
}

impl InferenceToolProvider for QqInferenceToolProvider {
    fn augment_messages(&self, messages: &mut Vec<LLMMessage>, _context: &InferenceToolContext) {
        messages.insert(
            0,
            LLMMessage::system(format!(
                "你是 {}。请保持回答简洁、友好、准确；当可调用工具时优先使用工具获取事实。bot 先前的可见回复属于对话内容，不天然是真实世界事实；当用户追问你上一句里的模糊指代时，先判断那是不是玩笑、修辞或口嗨，再决定是否需要澄清。",
                self.resources.bot_name
            )),
        );
    }

    fn build_default_tools(&self, context: &InferenceToolContext) -> Vec<Box<dyn BrainTool>> {
        build_info_brain_tools(
            &self.resources.default_tools_enabled,
            self.resources.web_search_engine_ref.clone(),
            self.resources.rdb_pool.clone(),
            self.resources.s3_ref.clone(),
            self.resources.weaviate_image_ref.clone(),
            self.resources.elasticsearch_image_ref.clone(),
            self.resources.weaviate_memory_ref.clone(),
            self.resources.elasticsearch_memory_ref.clone(),
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
    config: &QqChatAgentServiceConfig,
    connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    Ok(Arc::new(QqInferenceToolProvider {
        resources: load_qq_resources(agent, config, connections)?,
        tool_definitions: build_enabled_tool_definitions(&agent.tools)?,
    }))
}

fn load_qq_resources(
    agent: &AgentConfig,
    config: &QqChatAgentServiceConfig,
    connections: &[ConnectionConfig],
) -> Result<QqLoadedInferenceResources> {
    if config
        .weaviate_memory_connection_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && config
            .elasticsearch_memory_connection_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(Error::ValidationError(
            "configure either Weaviate or Elasticsearch for agent memory, not both".to_string(),
        ));
    }
    if config
        .weaviate_image_connection_id
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && config
            .elasticsearch_image_connection_id
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(Error::ValidationError(
            "configure either Weaviate or Elasticsearch for image semantic storage, not both".to_string(),
        ));
    }
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

    let rdb_pool = match config.resolved_rdb_id() {
        Some(connection_id) => {
            block_async(build_relational_db_connection_for_connection(connection_id, connections)).ok()
        }
        None => None,
    };
    let s3_ref = block_async(build_s3_ref(config.rustfs_connection_id.as_deref(), connections)).unwrap_or_else(|e| {
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
    let elasticsearch_image_ref = build_elasticsearch_ref(
        config
            .elasticsearch_image_connection_id
            .as_deref()
            .filter(|value| !value.trim().is_empty()),
        connections,
        Some(WeaviateCollectionSchema::ImageSemantic),
    )
    .unwrap_or_else(|error| {
        warn!("[inference][qq_agent] elasticsearch image connection unavailable: {error}");
        None
    });
    let elasticsearch_memory_ref = build_elasticsearch_ref(
        config
            .elasticsearch_memory_connection_id
            .as_deref()
            .filter(|value| !value.trim().is_empty()),
        connections,
        Some(WeaviateCollectionSchema::AgentMemory),
    )
    .unwrap_or_else(|error| {
        warn!("[inference][qq_agent] elasticsearch memory connection unavailable: {error}");
        None
    });

    let embedding_model = if let Some(model_ref_id) = config.embedding_model_ref_id.as_deref() {
        let llm_refs = model_inference::system_config::load_llm_refs().unwrap_or_default();
        match resolve_local_embedding_model_name(Some(model_ref_id), &llm_refs, &agent.name) {
            Ok(Some(_)) => {
                block_async(RuntimeEmbeddingModelManager::shared().get_or_create_embedding_model(model_ref_id)).ok()
            }
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
            resolve_llm_service_config(Some(llm_ref_id), &load_llm_refs().unwrap_or_default(), &agent.name)
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
        rdb_pool,
        s3_ref,
        weaviate_image_ref,
        elasticsearch_image_ref,
        weaviate_memory_ref,
        elasticsearch_memory_ref,
        embedding_model,
        memory_llm,
    })
}

/// Purpose: Bootstrap and launch a long-running QQ Chat Agent Service instance.
///
/// Resolves all runtime dependencies (`llm`, `embedding_model`, `tavily`, `s3_ref`,
/// `rdb_pool`, `weaviate_image_ref`), wires the IMS bot adapter event handler
/// through an inbox queue, then spawns a background task that runs the
/// `BotAdapter::start` loop until exit.
///
/// Called when the service layer starts an agent whose type is QQ chat —
/// typically from `AgentManager::start_agent` after validating the agent config.
///
/// Call chain:
///   `AgentManager::start_agent` → `QqChatAgentService::spawn`
///     → build deps → register `EventHandler` on bot adapter
///     → `tokio::spawn`(`BotAdapter::start`) → `handle_event` per incoming message
///     → `on_finish` callback on exit
pub async fn spawn(
    manager: &AgentManager,
    agent: AgentConfig,
    config: QqChatAgentServiceConfig,
    connections: Vec<ConnectionConfig>,
    on_finish: super::OnFinishShared,
    task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
) -> Result<JoinHandle<()>> {
    let llm_refs = load_llm_refs()?;
    let bot_connection = find_connection(&connections, &config.ims_bot_adapter_connection_id)?;
    let ConnectionKind::BotAdapter(_) = &bot_connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a bot adapter connection",
            bot_connection.name
        )));
    };

    let llm_config = resolve_llm_service_config(config.llm_ref_id.as_deref(), &llm_refs, &agent.name)?;
    let llm = build_llm_model(&llm_config)?;
    let intent_classification_llm_config = resolve_llm_service_config(
        config
            .intent_classification_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        &llm_refs,
        &agent.name,
    )?;
    let intent_classification_llm = build_llm_model(&intent_classification_llm_config)?;
    let math_programming_llm_config = resolve_llm_service_config(
        config.math_programming_llm_ref_id.as_deref().or(config.llm_ref_id.as_deref()),
        &llm_refs,
        &agent.name,
    )?;
    let math_programming_llm = build_llm_model(&math_programming_llm_config)?;
    let natural_language_reply_llm_config =
        resolve_llm_service_config(config.natural_language_reply_llm_ref_id.as_deref(), &llm_refs, &agent.name)?;
    let natural_language_reply_llm = build_llm_model(&natural_language_reply_llm_config)?;
    let embedding_model = if let Some(model_ref_id) = config.embedding_model_ref_id.as_deref() {
        let model_name = resolve_local_embedding_model_name(Some(model_ref_id), &llm_refs, &agent.name)?;
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
            .ok_or_else(|| Error::ValidationError("missing web search engine connection".to_string()))?;
    let object_storage = build_s3_ref(config.rustfs_connection_id.as_deref(), &connections).await?;
    let rdb_pool = match config.resolved_rdb_id() {
        Some(connection_id) => Some(build_relational_db_connection_for_connection(connection_id, &connections).await?),
        None => None,
    };
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

    if let Some(ref rdb_pool) = rdb_pool {
        register_rdb_pool(rdb_pool.clone());
    }

    let service = Arc::new(QqChatAgentService::new(QqChatAgentServiceRuntimeConfig {
        agent_id: agent.id.clone(),
        qq_chat_config: config.clone(),
        node_id: format!("service_agent_{}", agent.id),
        bot_name: if config.bot_name.trim().is_empty() {
            agent.name.clone()
        } else {
            config.bot_name.clone()
        },
        system_prompt: config.system_prompt.clone(),
        cache: Arc::new(LLMMessageSessionCacheRef::new(format!("service_agent_cache_{}", agent.id))),
        session: Arc::new(SessionStateRef::new(format!("service_agent_session_{}", agent.id))),
        llm,
        intent_classification_llm,
        math_programming_llm,
        natural_language_reply_llm,
        rdb_pool,
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
        session_state_store: Arc::new(Mutex::new(QqChatAgentServiceSessionState::default())),
        task_runtime,
        tool_quota_session_state: Arc::new(Mutex::new(SessionToolQuotaState::default())),
    })?);

    let adapter = ActiveAdapterManager::shared()
        .get_or_create_with_object_storage(&config.ims_bot_adapter_connection_id, object_storage)
        .await?;

    let inbox = QqChatAgentServiceInbox::new(
        Arc::clone(&service),
        adapter.clone(),
        redis_ref,
        &agent.id,
        config.event_handler_threads,
    );

    let handler_id = format!("qq_chat_service:{}", agent.id);
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
        adapter.lock().await.register_event_handler_with_id(handler_id.clone(), handler);
    }

    let manager = manager.clone();
    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();
    let adapter_for_cleanup = adapter.clone();
    let handler_id_for_cleanup = handler_id.clone();
    let user_on_finish = {
        let mut guard = on_finish.lock().unwrap();
        guard.take()
    };
    {
        let adapter = adapter_for_cleanup.clone();
        let handler_id = handler_id_for_cleanup.clone();
        let mut guard = on_finish.lock().unwrap();
        *guard = Some(Box::new(move |success, error_msg| {
            let adapter = adapter.clone();
            let handler_id = handler_id.clone();
            tokio::spawn(async move {
                adapter.lock().await.unregister_event_handler(&handler_id);
            });
            if let Some(cb) = user_on_finish {
                cb(success, error_msg);
            }
        }));
    }

    Ok(tokio::spawn(async move {
        info!("[service] starting QQ Chat Agent Service '{}'", agent_name);
        let mut tasks = tokio::task::JoinSet::new();
        inbox.spawn_consumers(&mut tasks);
        std::future::pending::<()>().await;
        inbox.request_shutdown();
        adapter_for_cleanup
            .lock()
            .await
            .unregister_event_handler(&handler_id_for_cleanup);

        let success = true;
        let error_msg = None;

        if success {
            info!("[service] QQ Chat Agent Service '{}' stopped", agent_name);
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
                .unwrap_or_else(|| "QQ Chat Agent Service exited unexpectedly".to_string());
            error!("[service] QQ Chat Agent Service '{}' exited with error: {}", agent_name, msg);
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
    config: &QqChatAgentServiceConfig,
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

impl QqChatAgentServiceInner {
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
        ctx: &QqChatAgentServiceContext<'_>,
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

        if let Err(err) = persist_message_event(event, ctx.rdb_pool, None) {
            warn!("{LOG_PREFIX} Message persistence failed: {err}");
        }

        if let Some(rdb_pool) = ctx.rdb_pool {
            let group_id_text = event.group_id.map(|value| value.to_string());
            if should_ignore_message_blocking(rdb_pool, agent_id, &sender_id, group_id_text.as_deref())? {
                info!(
                    "{LOG_PREFIX} Ignored inbound message: message_id={} sender={} group={:?}",
                    event.message_id, sender_id, event.group_id
                );
                return Ok(());
            }
        }

        if is_group {
            let bot_id = get_bot_id(ctx.adapter);
            let msg_prop =
                MessageProp::from_messages_with_bot_name(&event.message_list, Some(&bot_id), Some(ctx.bot_name));
            if !msg_prop.is_at_me {
                return Ok(());
            }
        }

        let mut message_rate_limit_warning = None;
        if let Some(rdb_pool) = ctx.rdb_pool {
            let group_id_text = event.group_id.map(|value| value.to_string());
            let config = current_qq_chat_agent_service_config()?;
            let rate_limit_result =
                consume_message_rate_limit_blocking(rdb_pool, agent_id, &sender_id, group_id_text.as_deref(), &config)?;
            if !rate_limit_result.allowed {
                match rate_limit_result.block_action {
                    MessageRateLimitBlockAction::ReplyOnce => {
                        let bot_id = get_bot_id(ctx.adapter);
                        let blocked_reply = rate_limit_result
                            .blocked_reply
                            .unwrap_or_else(|| "你已经达到 rate limit 了，请待会再找我。".to_string());
                        let mention_target_id =
                            (is_group && rate_limit_result.mention_sender_on_block).then_some(sender_id.as_str());
                        send_direct_notification_text_reply(
                            &QqChatTaskTrace::new(Local::now()),
                            ctx.adapter,
                            &target_id,
                            ctx.rdb_pool,
                            event.group_name.as_deref(),
                            ctx.bot_name,
                            &bot_id,
                            &blocked_reply,
                            is_group,
                            mention_target_id,
                            ctx.max_message_length,
                        )?;
                        info!(
                            "{LOG_PREFIX} Message rate-limited with reply: message_id={} sender={} group={:?}",
                            event.message_id, sender_id, event.group_id
                        );
                    }
                    MessageRateLimitBlockAction::Silent => {
                        info!(
                            "{LOG_PREFIX} Message rate-limited silently: message_id={} sender={} group={:?}",
                            event.message_id, sender_id, event.group_id
                        );
                    }
                    MessageRateLimitBlockAction::None => {
                        info!(
                            "{LOG_PREFIX} Message rate-limited without block action: message_id={} sender={} group={:?}",
                            event.message_id, sender_id, event.group_id
                        );
                    }
                }
                return Ok(());
            }
            message_rate_limit_warning = rate_limit_result.warning_prompt;
        }

        let (claimed, claim_token) = try_claim_session(session, &sender_id);
        if !claimed {
            return self.try_handle_busy_session_steer(event, ctx, &sender_id, &target_id, is_group, time);
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
                            &trace,
                            event,
                            time,
                            &sender_id,
                            &target_id,
                            is_group,
                            message_rate_limit_warning.as_deref(),
                            ctx,
                        )
                    })
                })
            } else {
                scope_task_id(task_handle.task_id.clone(), || {
                    self.handle_claimed(
                        &trace,
                        event,
                        time,
                        &sender_id,
                        &target_id,
                        is_group,
                        message_rate_limit_warning.as_deref(),
                        ctx,
                    )
                })
            }
        } else {
            self.handle_claimed(
                &trace,
                event,
                time,
                &sender_id,
                &target_id,
                is_group,
                message_rate_limit_warning.as_deref(),
                ctx,
            )
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
