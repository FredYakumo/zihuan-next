use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use storage_handler::{
    load_connections, resource_resolver, RuntimeStorageConnectionManager, WeaviateCollectionSchema,
};
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::rag::TavilyRef;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::{DataType, DataValue, NodeConfigField, NodeConfigWidget};

use crate::llm_api::LLMAPI;
use crate::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use crate::system_config::{load_llm_refs, ModelRefSpec, QqChatAgentConfig};

pub const LLM_KIND_FIELD: &str = "llm_kind";
pub const LLM_KIND_MAIN: &str = "main";
pub const LLM_KIND_INTENT: &str = "intent";
pub const LLM_KIND_MATH_PROGRAMMING: &str = "math_programming";

thread_local! {
    static CURRENT_QQ_CHAT_AGENT_CONFIG: RefCell<Vec<QqChatAgentConfig>> = const { RefCell::new(Vec::new()) };
}

pub fn agent_llm_kind_select_field(description: &str) -> NodeConfigField {
    NodeConfigField::new(
        LLM_KIND_FIELD,
        DataType::String,
        NodeConfigWidget::AgentLlmKindSelect,
    )
    .with_description(description)
}

pub fn read_llm_kind(inline_values: &HashMap<String, DataValue>) -> Option<String> {
    inline_values
        .get(LLM_KIND_FIELD)
        .and_then(|value| match value {
            DataValue::String(value) => Some(value.clone()),
            _ => None,
        })
}

pub fn with_current_qq_chat_agent_config<T>(config: QqChatAgentConfig, f: impl FnOnce() -> T) -> T {
    CURRENT_QQ_CHAT_AGENT_CONFIG.with(|slot| {
        slot.borrow_mut().push(config);
    });
    let result = f();
    CURRENT_QQ_CHAT_AGENT_CONFIG.with(|slot| {
        slot.borrow_mut().pop();
    });
    result
}

pub fn current_qq_chat_agent_config() -> Result<QqChatAgentConfig> {
    CURRENT_QQ_CHAT_AGENT_CONFIG.with(|slot| {
        slot.borrow().last().cloned().ok_or_else(|| {
            Error::ValidationError(
                "当前节点不在 Agent 工具调用上下文中，无法读取 Agent 配置".to_string(),
            )
        })
    })
}

pub fn normalize_llm_kind(llm_kind: Option<&str>) -> Result<&'static str> {
    match llm_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(LLM_KIND_MAIN)
    {
        LLM_KIND_MAIN => Ok(LLM_KIND_MAIN),
        LLM_KIND_INTENT => Ok(LLM_KIND_INTENT),
        LLM_KIND_MATH_PROGRAMMING => Ok(LLM_KIND_MATH_PROGRAMMING),
        other => Err(Error::ValidationError(format!(
            "unsupported llm_kind '{}', expected one of: {}, {}, {}",
            other, LLM_KIND_MAIN, LLM_KIND_INTENT, LLM_KIND_MATH_PROGRAMMING
        ))),
    }
}

pub fn llm_ref_id_for_kind<'a>(config: &'a QqChatAgentConfig, llm_kind: &str) -> Option<&'a str> {
    match llm_kind {
        LLM_KIND_MAIN => config.llm_ref_id.as_deref(),
        LLM_KIND_INTENT => config
            .intent_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        LLM_KIND_MATH_PROGRAMMING => config
            .math_programming_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        _ => None,
    }
}

pub fn build_llm_from_ref_id(llm_ref_id: Option<&str>) -> Result<Arc<dyn LLMBase>> {
    let llm_ref_id = llm_ref_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::ValidationError("llm_ref_id is required".to_string()))?;

    let llm_ref = load_llm_refs()?
        .into_iter()
        .find(|item| item.id == llm_ref_id || item.config_id == llm_ref_id)
        .ok_or_else(|| Error::ValidationError(format!("llm_ref '{}' not found", llm_ref_id)))?;

    if !llm_ref.enabled {
        return Err(Error::ValidationError(format!(
            "llm_ref '{}' is disabled",
            llm_ref.name
        )));
    }

    let ModelRefSpec::ChatLlm { llm } = llm_ref.model else {
        return Err(Error::ValidationError(format!(
            "llm_ref '{}' is not a chat LLM config",
            llm_ref.name
        )));
    };

    Ok(Arc::new(
        LLMAPI::new(
            llm.model_name,
            llm.api_endpoint,
            llm.api_key,
            llm.api_style,
            llm.stream,
            llm.supports_multimodal_input,
            Duration::from_secs(llm.timeout_secs),
        )
        .with_retry_count(llm.retry_count),
    ))
}

pub fn build_embedding_from_ref_id(
    embedding_model_ref_id: Option<&str>,
) -> Result<Arc<dyn EmbeddingBase>> {
    let embedding_model_ref_id = embedding_model_ref_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::ValidationError("embedding_model_ref_id is required".to_string()))?;

    zihuan_core::runtime::block_async(
        RuntimeEmbeddingModelManager::shared()
            .get_or_create_embedding_model(embedding_model_ref_id),
    )
}

pub fn build_mysql_ref(mysql_connection_id: Option<&str>) -> Result<Arc<MySqlConfig>> {
    let mysql_connection_id = mysql_connection_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::ValidationError("mysql_connection_id is required".to_string()))?;
    zihuan_core::runtime::block_async(
        RuntimeStorageConnectionManager::shared().get_or_create_mysql_ref(mysql_connection_id),
    )
}

pub fn build_rustfs_ref(rustfs_connection_id: Option<&str>) -> Result<Arc<S3Ref>> {
    let rustfs_connection_id = rustfs_connection_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::ValidationError("rustfs_connection_id is required".to_string()))?;
    zihuan_core::runtime::block_async(
        RuntimeStorageConnectionManager::shared().get_or_create_s3_ref(rustfs_connection_id),
    )
}

pub fn build_tavily_ref(tavily_connection_id: Option<&str>) -> Result<Arc<TavilyRef>> {
    let tavily_connection_id = tavily_connection_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::ValidationError("tavily_connection_id is required".to_string()))?;
    let connections = load_connections()?;
    resource_resolver::build_tavily_ref(Some(tavily_connection_id), &connections)?
        .ok_or_else(|| Error::ValidationError("tavily_connection_id is required".to_string()))
}

pub fn build_image_db_ref(weaviate_image_connection_id: Option<&str>) -> Result<Arc<WeaviateRef>> {
    let weaviate_image_connection_id = weaviate_image_connection_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Error::ValidationError("weaviate_image_connection_id is required".to_string())
        })?;
    let connections = load_connections()?;
    resource_resolver::build_weaviate_ref(Some(weaviate_image_connection_id), &connections, true)?
        .ok_or_else(|| {
            Error::ValidationError("weaviate_image_connection_id is required".to_string())
        })
}

pub fn ensure_image_schema(ref_value: &WeaviateRef) -> Result<()> {
    ref_value.ensure_collection_schema(WeaviateCollectionSchema::ImageSemantic, false)?;
    Ok(())
}
