use std::collections::HashMap;
use std::sync::Arc;

use storage_handler::ElasticsearchRef;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::rag::WebSearchEngineRef;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;
use zihuan_graph_engine::object_storage::S3Ref;

/// Loaded inference resources for the QQ chat agent.
#[derive(Clone)]
pub(crate) struct QqLoadedInferenceResources {
    pub(crate) bot_name: String,
    pub(crate) default_tools_enabled: HashMap<String, bool>,
    pub(crate) web_search_engine_ref: Option<Arc<WebSearchEngineRef>>,
    pub(crate) rdb_pool: Option<RelationalDbConnection>,
    pub(crate) s3_ref: Option<Arc<S3Ref>>,
    pub(crate) weaviate_image_ref: Option<Arc<WeaviateRef>>,
    pub(crate) elasticsearch_image_ref: Option<Arc<ElasticsearchRef>>,
    pub(crate) weaviate_memory_ref: Option<Arc<WeaviateRef>>,
    pub(crate) elasticsearch_memory_ref: Option<Arc<ElasticsearchRef>>,
    pub(crate) embedding_model: Option<Arc<dyn EmbeddingBase>>,
    pub(crate) memory_llm: Option<Arc<dyn LLMBase>>,
}

/// Inference tool provider for the QQ chat agent.
pub struct QqInferenceToolProvider {
    pub(crate) resources: QqLoadedInferenceResources,
    pub(crate) tool_definitions: Vec<BrainToolDefinition>,
}
