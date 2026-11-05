pub(crate) mod command;
mod core;
mod event_processor;
pub mod ignore_store;
mod inbox;
pub mod language_style_store;
pub(crate) mod logging;
pub(crate) mod model;
pub(crate) mod msg_send;
pub mod privilege_gate;
pub mod privilege_store;
pub mod resources;
pub(crate) mod steer;
pub mod style_learner;
pub(crate) mod tool_quota;
pub mod tool_quota_store;
mod turn_workflow;
pub(crate) mod user_input;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

pub(crate) use self::core::{
    build_group_system_prompt, build_private_system_prompt, build_user_message,
    collect_available_media_from_brain_output, compact_turn_history, split_current_user_body,
    DEFAULT_REPLY_REQUEST,
};
use self::core::{
    build_info_brain_tools, expand_messages_for_inference,
    prepare_current_turn_user_input_from_event,
};
pub(crate) use self::core::{QqChatTaskTrace, LOG_PREFIX};
use self::inbox::QqChatAgentServiceInbox;
pub(crate) use self::model::{QqChatAgentService, QqChatAgentServiceContext};
use self::model::{
    QqChatAgentServiceRuntimeConfig, QqChatServiceReplyBatchBuilder, QqInferenceToolProvider,
    QqLoadedInferenceResources,
};
use crate::procedure::utils::reply_plan::build_reply_batch_builder as build_unified_reply_batch_builder;
use crate::qq_session_state::QqChatSessionState;
use crate::role_config::{public_model_display_names, QqChatRoleServiceConfig};
pub use crate::storage::message_rate_limit_store::{
    list_message_rate_limit_usage, reset_message_rate_limit_usage,
};
pub use crate::storage::qq_chat_context_store::{
    delete_turn as delete_archived_context_turn, load_turns as load_archived_context_turns,
    persist_turn as persist_archived_context_turn, query_turns as query_archived_context_turns,
    ArchivedTurn, TurnSummary,
};
pub use crate::storage::qq_chat_history_store::clear_history;
use crate::storage::qq_chat_history_store::load_history;
use crate::tools::build_job_memory_resources;
use chrono::Local;
use log::{error, info, warn};
use tokio::task::JoinHandle;
use zihuan_core::agent::inference_provider::{InferenceToolContext, InferenceToolProvider};
use zihuan_core::agent::resource_resolver::{
    build_embedding_model, build_llm_model, resolve_llm_service_config,
    resolve_local_embedding_model_name,
};
use zihuan_core::agent::tool_definitions::{
    build_enabled_tool_definitions, selected_plugin_names, selected_sub_agent_ids,
};
use zihuan_core::agent::tools::Tool;
use zihuan_core::config::llm_refs::load_llm_refs;
use zihuan_core::error::{Error, Result};
use zihuan_core::graph::function_graph::FunctionPortDef;
use zihuan_core::graph::tool_spec::ToolDefinition;
use zihuan_core::graph::SessionStateRef;
use zihuan_core::ims_bot_adapter::active_adapter_manager::ActiveAdapterManager;
use zihuan_core::ims_bot_adapter::event::EventHandler;
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use zihuan_core::nlp::{build_segmenter, TextSegmenter};
use zihuan_core::role::service_config::RoleServiceConfig;
use zihuan_core::runtime::block_async;
use zihuan_core::storage::message::record_restore::register_rdb_pool;
use zihuan_core::storage::LLMMessageSessionCacheRef;
use zihuan_core::storage::{
    build_relational_db_connection_for_connection, build_retrieval_store_ref, build_s3_ref,
    build_web_search_engine_ref, find_connection, ConnectionConfig, ConnectionKind,
    LocalMemoryStore, RetrievalStoreRef,
};
use zihuan_core::task_context::AgentTaskRuntime;
use zihuan_core::validation_error;

use self::tool_quota::SessionToolQuotaState;

fn build_reply_batch_builder(segmenter: Arc<dyn TextSegmenter>) -> QqChatServiceReplyBatchBuilder {
    build_unified_reply_batch_builder(segmenter)
}

#[doc(hidden)]
pub fn expand_message_event_for_tool_input(
    event: &zihuan_core::ims_bot_adapter::models::event_model::MessageEvent,
) -> zihuan_core::ims_bot_adapter::models::event_model::MessageEvent {
    let mut expanded = event.clone();
    expanded.message_list = expand_messages_for_inference(&event.message_list);
    expanded
}

#[doc(hidden)]
pub use self::user_input::PreparedCurrentTurnUserInput;

#[doc(hidden)]
pub fn prepare_message_event_user_input_for_test(
    event: &zihuan_core::ims_bot_adapter::models::event_model::MessageEvent,
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

    fn build_default_tools(&self, _context: &InferenceToolContext) -> Vec<Box<dyn Tool>> {
        build_info_brain_tools(
            &self.resources.default_tools_enabled,
            self.resources.web_search_engine_ref.clone(),
            self.resources.rdb_pool.clone(),
            self.resources.s3_ref.clone(),
            self.resources.retrieval_store.clone(),
            self.resources.local_memory_store.clone(),
            self.resources.embedding_model.clone(),
            self.resources.memory_llm.clone(),
            zihuan_core::storage::AgentMemoryAccessContext::default(),
            self.resources.model_names.clone(),
            &self.resources.plugin_tool_names,
        )
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tool_definitions.clone()
    }
}

pub fn load_inference_tool_provider(
    agent: &RoleServiceConfig,
    config: &QqChatRoleServiceConfig,
    connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    Ok(Arc::new(QqInferenceToolProvider {
        resources: load_qq_resources(agent, config, connections)?,
        tool_definitions: build_enabled_tool_definitions(&agent.tools)?,
    }))
}

fn load_qq_resources(
    agent: &RoleServiceConfig,
    config: &QqChatRoleServiceConfig,
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

    let rdb_pool = match config.resolved_rdb_id() {
        Some(connection_id) => {
            block_async(build_relational_db_connection_for_connection(connection_id, connections))
                .ok()
        }
        None => None,
    };
    let s3_ref = block_async(build_s3_ref(config.rustfs_connection_id.as_deref(), connections))
        .unwrap_or_else(|e| {
            warn!("[inference][qq_agent] rustfs connection unavailable: {e}");
            None
        });

    let retrieval_store = resolve_retrieval_store(config, connections);
    let local_memory_store = config
        .retrieval_store_is_local()
        .then(|| Arc::new(LocalMemoryStore::in_app_data_dir()));

    let llm_refs = load_llm_refs().unwrap_or_default();
    let embedding_model = if local_memory_store.is_some() {
        None
    } else if let Some(model_ref_id) = config.embedding_model_ref_id.as_deref() {
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
            resolve_llm_service_config(Some(llm_ref_id), &llm_refs).map_err(|err| {
                validation_error!("[{}] Failed to load llm config: {err}", &agent.name)
            })
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
        retrieval_store,
        local_memory_store,
        embedding_model,
        memory_llm,
        model_names: public_model_display_names(config, &llm_refs),
        plugin_tool_names: selected_plugin_names(&agent.tools),
    })
}

/// Resolves the configured retrieval store, warning instead of failing so a
/// misconfigured store does not prevent the agent from starting.
fn resolve_retrieval_store(
    config: &QqChatRoleServiceConfig,
    connections: &[ConnectionConfig],
) -> Option<Arc<RetrievalStoreRef>> {
    let connection_id = config.retrieval_store_connection_id()?;
    match build_retrieval_store_ref(connection_id, connections) {
        Ok(store) => Some(Arc::new(store)),
        Err(error) => {
            warn!("[inference][qq_agent] retrieval store unavailable: {error}");
            None
        }
    }
}

/// Purpose: Bootstrap and launch a long-running QQ Chat Agent Service instance.
///
/// Resolves all runtime dependencies (llm, embedding_model, tavily, s3_ref,
/// rdb_pool, retrieval_store), wires the IMS bot adapter event handler
/// through an inbox queue, then spawns a background task that runs the
/// BotAdapter::start loop until exit.
///
/// Called when the service layer starts an agent whose type is QQ chat —
/// typically from AgentManager::start_agent after validating the agent config.
///
/// Call chain:
///   AgentManager::start_agent -> QqChatAgentService::spawn
///     -> build deps -> register EventHandler on bot adapter
///     -> tokio::spawn(BotAdapter::start) -> handle_event per incoming message
///     -> on_finish callback on exit
pub type RuntimeFinishedCallback = Arc<dyn Fn(bool, Option<String>) + Send + Sync>;

pub async fn spawn(
    agent: RoleServiceConfig,
    config: QqChatRoleServiceConfig,
    connections: Vec<ConnectionConfig>,
    on_finish: RuntimeFinishedCallback,
    task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
) -> Result<JoinHandle<()>> {
    let llm_refs = load_llm_refs()?;
    let public_model_names = public_model_display_names(&config, &llm_refs);
    let bot_connection = find_connection(&connections, &config.ims_bot_adapter_connection_id)?;
    let ConnectionKind::BotAdapter(_) = &bot_connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a bot adapter connection",
            bot_connection.name
        )));
    };

    let llm_config = resolve_llm_service_config(config.llm_ref_id.as_deref(), &llm_refs)
        .map_err(|err| validation_error!("[{}] Failed to load llm config: {err}", &agent.name))?;
    let llm = build_llm_model(&llm_config)?;
    let intent_classification_llm_config = resolve_llm_service_config(
        config
            .intent_classification_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        &llm_refs,
    )
    .map_err(|err| validation_error!("[{}] Failed to load llm config: {err}", &agent.name))?;
    let intent_classification_llm = build_llm_model(&intent_classification_llm_config)?;
    let math_programming_llm_config = resolve_llm_service_config(
        config.math_programming_llm_ref_id.as_deref().or(config.llm_ref_id.as_deref()),
        &llm_refs,
    )
    .map_err(|err| validation_error!("[{}] Failed to load llm config: {err}", &agent.name))?;
    let math_programming_llm = build_llm_model(&math_programming_llm_config)?;
    let natural_language_reply_llm_config =
        resolve_llm_service_config(config.natural_language_reply_llm_ref_id.as_deref(), &llm_refs)
            .map_err(|err| {
                validation_error!("[{}] Failed to load llm config: {err}", &agent.name)
            })?;
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
    let redis_ref = resolve_inbox_redis_ref(&connections)?;
    let retrieval_store = resolve_retrieval_store(&config, &connections);
    let tool_definitions = build_enabled_tool_definitions(&agent.tools)?;
    let tokenizer_segmenter = resolve_tokenizer_segmenter(&config, &connections);

    if let Some(ref rdb_pool) = rdb_pool {
        register_rdb_pool(rdb_pool.clone());
    }

    let cache =
        Arc::new(LLMMessageSessionCacheRef::new(format!("service_agent_cache_{}", agent.id)));

    // Back the conversation context with the same Redis the inbox uses, when one is configured,
    // so the live context (and the Data Explorer view over it) survives a process restart.
    // Without a Redis connection the cache stays process-memory only.
    if let Some(redis_ref) = redis_ref.as_ref() {
        if let Some(url) = redis_ref.url.clone() {
            cache.configure_redis(Some(url)).await;
        }
    }
    zihuan_core::qq_context::register_qq_chat_history_cache(&agent.id, Arc::clone(&cache));

    // Register the service's job resources with the scheduler kernel so script-defined
    // jobs can reach this service's LLM, tools, history cache, and memory backend. The
    // guard lives in the service task below; stopping the service unregisters the resources.
    let job_memory = build_job_memory_resources(
        config
            .retrieval_store_is_local()
            .then(|| Arc::new(LocalMemoryStore::in_app_data_dir())),
        retrieval_store.clone(),
        embedding_model.clone(),
    );
    let scheduled_jobs = config.resolved_scheduled_jobs();
    let scheduler_guard = if !scheduled_jobs.is_empty() {
        rdb_pool
            .as_ref()
            .zip(job_memory.clone())
            .map(|(connection, memory)| {
                let history_cache = Arc::clone(&cache);
                let clear_cache = Arc::clone(&cache);
                zihuan_core::scheduler::register_service(zihuan_core::scheduler::JobResources {
                    agent_id: agent.id.clone(),
                    connection: connection.clone(),
                    llm: Arc::clone(&llm),
                    tool_definitions: tool_definitions.clone(),
                    history_loader: Arc::new(move |sender_id: &str| {
                        load_history(&history_cache, sender_id)
                    }),
                    history_clearer: Arc::new(move |sender_id: &str| {
                        clear_history(&clear_cache, sender_id)
                    }),
                    memory: Some(memory),
                    scheduled_jobs,
                })
            })
            .transpose()?
    } else {
        None
    };

    // One-time migration of the retired dream_memory table into the agent's memory store.
    // Best-effort: when the backend is unavailable the rows stay for the next start.
    if let (Some(connection), Some(memory)) = (rdb_pool.as_ref(), job_memory) {
        match zihuan_core::database::migrate_legacy_dream_memory(
            connection,
            &agent.id,
            |sender_id, content| {
                zihuan_core::scheduler::store_agent_memory(
                    &memory,
                    &zihuan_core::storage::AgentMemoryUpsert {
                        attributes: zihuan_core::storage::MemoryAttributes {
                            category: zihuan_core::storage::MemoryCategory::Person,
                            person: zihuan_core::storage::PersonMemory {
                                qq_id: sender_id.to_string(),
                                impression: content.to_string(),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        key: format!("person-{sender_id}"),
                        value: content.to_string(),
                        expires_at: None,
                        sender_id_list: Vec::new(),
                        group_id_list: Vec::new(),
                    },
                )
                .map(|_| ())
            },
        )
        .await
        {
            Ok(0) => {}
            Ok(count) => {
                info!("[Scheduler] migrated {count} legacy dream memory row(s) into the agent memory store");
            }
            Err(error) => {
                warn!("[Scheduler] legacy dream memory migration deferred: {error}");
            }
        }
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
        cache: Arc::clone(&cache),
        session: Arc::new(SessionStateRef::new(format!("service_agent_session_{}", agent.id))),
        llm: Arc::clone(&llm),
        intent_classification_llm,
        math_programming_llm,
        natural_language_reply_llm,
        rdb_pool,
        retrieval_store,
        local_memory_store: config
            .retrieval_store_is_local()
            .then(|| Arc::new(LocalMemoryStore::in_app_data_dir())),
        embedding_model,
        web_search_engine,
        s3_ref: object_storage.clone(),
        max_message_length: config.max_message_length,
        max_steer_count: config.max_steer_count,
        reply_batch_builder: Some(build_reply_batch_builder(tokenizer_segmenter)),
        default_tools_enabled: config.default_tools_enabled.clone(),
        shared_inputs: Vec::<FunctionPortDef>::new(),
        tool_definitions: tool_definitions.clone(),
        sub_agent_ids: selected_sub_agent_ids(&agent.tools),
        plugin_tool_names: selected_plugin_names(&agent.tools),
        shared_runtime_values: HashMap::new(),
        session_state_store: Arc::new(Mutex::new(QqChatSessionState::default())),
        task_runtime,
        tool_quota_session_state: Arc::new(Mutex::new(SessionToolQuotaState::default())),
        public_model_names,
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

    let agent_name = agent.name.clone();
    let adapter_for_cleanup = adapter.clone();
    let handler_id_for_cleanup = handler_id.clone();

    Ok(tokio::spawn(async move {
        let _scheduler_guard = scheduler_guard;
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
        } else {
            let msg = error_msg
                .clone()
                .unwrap_or_else(|| "QQ Chat Agent Service exited unexpectedly".to_string());
            error!("[service] QQ Chat Agent Service '{}' exited with error: {}", agent_name, msg);
        }
        on_finish(success, error_msg);
    }))
}

fn resolve_tokenizer_segmenter(
    config: &QqChatRoleServiceConfig,
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
) -> Result<Option<Arc<zihuan_core::storage::RedisConfig>>> {
    let redis_connection_id = connections.iter().find_map(|connection| {
        if connection.enabled && matches!(connection.kind, ConnectionKind::Redis(_)) {
            Some(connection.id.as_str())
        } else {
            None
        }
    });
    zihuan_core::storage::build_redis_ref(redis_connection_id, connections)
}
