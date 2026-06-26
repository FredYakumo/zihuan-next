use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zihuan_agent::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::qq_chat::QqChatAgentServiceConfig;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::rag::WebSearchEngineRef;
use zihuan_core::steer::PendingSteerStore;
use zihuan_core::task_context::AgentTaskRuntime;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;
use zihuan_graph_engine::data_value::{LLMMessageSessionCacheRef, SessionStateRef};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::DataValue;

use crate::agent::qq_chat::language_style_store::QqChatAgentServiceLanguageStyle;
use crate::agent::qq_chat::model::reply::QqChatServiceReplyBatchBuilder;
use crate::agent::qq_chat::tool_quota::{QqChatToolQuotaContext, SessionToolQuotaState};

/// Runtime context assembled per-turn for the QQ chat agent service.
pub(crate) struct QqChatAgentServiceContext<'a> {
    pub(crate) adapter: &'a ims_bot_adapter::adapter::SharedBotAdapter,
    pub(crate) bot_name: &'a str,
    pub(crate) agent_system_prompt: Option<&'a str>,
    pub(crate) cache: &'a Arc<LLMMessageSessionCacheRef>,
    pub(crate) llm: &'a Arc<dyn LLMBase>,
    pub(crate) intent_classification_llm: &'a Arc<dyn LLMBase>,
    pub(crate) math_programming_llm: &'a Arc<dyn LLMBase>,
    pub(crate) natural_language_reply_llm: &'a Arc<dyn LLMBase>,
    pub(crate) natural_language_reply_system_prompt: Option<&'a str>,
    pub(crate) rdb_pool: Option<&'a RelationalDbConnection>,
    pub(crate) weaviate_image_ref: Option<&'a Arc<WeaviateRef>>,
    pub(crate) weaviate_memory_ref: Option<&'a Arc<WeaviateRef>>,
    pub(crate) embedding_model: Option<&'a Arc<dyn EmbeddingBase>>,
    pub(crate) web_search_engine: &'a Arc<WebSearchEngineRef>,
    pub(crate) s3_ref: Option<&'a Arc<S3Ref>>,
    pub(crate) max_message_length: usize,
    pub(crate) compact_context_length: usize,
    pub(crate) max_steer_count: usize,
    pub(crate) reply_batch_builder: Option<&'a QqChatServiceReplyBatchBuilder>,
    pub(crate) shared_runtime_values: HashMap<String, DataValue>,
    pub(crate) session_state_store: &'a Arc<Mutex<QqChatAgentServiceSessionState>>,
    pub(crate) pending_steer: &'a Arc<PendingSteerStore>,
    pub(crate) task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
    pub(crate) task_db_connection_id: Option<String>,
    pub(crate) tool_quota: Option<QqChatToolQuotaContext>,
    pub(crate) resolved_language_style: Option<QqChatAgentServiceLanguageStyle>,
}

/// Persistent runtime configuration for a QQ chat agent service instance.
#[derive(Clone)]
pub struct QqChatAgentServiceRuntimeConfig {
    pub agent_id: String,
    pub qq_chat_config: QqChatAgentServiceConfig,
    pub node_id: String,
    pub bot_name: String,
    pub system_prompt: Option<String>,
    pub cache: Arc<LLMMessageSessionCacheRef>,
    pub session: Arc<SessionStateRef>,
    pub llm: Arc<dyn LLMBase>,
    pub intent_classification_llm: Arc<dyn LLMBase>,
    pub math_programming_llm: Arc<dyn LLMBase>,
    pub natural_language_reply_llm: Arc<dyn LLMBase>,
    pub main_llm_display_name: String,
    pub intent_classification_llm_display_name: String,
    pub math_programming_llm_display_name: String,
    pub natural_language_reply_llm_display_name: String,
    pub rdb_pool: Option<RelationalDbConnection>,
    pub weaviate_image_ref: Option<Arc<WeaviateRef>>,
    pub weaviate_memory_ref: Option<Arc<WeaviateRef>>,
    pub embedding_model: Option<Arc<dyn EmbeddingBase>>,
    pub web_search_engine: Arc<WebSearchEngineRef>,
    pub s3_ref: Option<Arc<S3Ref>>,
    pub max_message_length: usize,
    pub compact_context_length: usize,
    pub max_steer_count: usize,
    pub reply_batch_builder: Option<QqChatServiceReplyBatchBuilder>,
    pub default_tools_enabled: HashMap<String, bool>,
    pub shared_inputs: Vec<FunctionPortDef>,
    pub tool_definitions: Vec<BrainToolDefinition>,
    pub shared_runtime_values: HashMap<String, DataValue>,
    pub session_state_store: Arc<Mutex<QqChatAgentServiceSessionState>>,
    pub task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
    pub tool_quota_session_state: Arc<Mutex<SessionToolQuotaState>>,
}
