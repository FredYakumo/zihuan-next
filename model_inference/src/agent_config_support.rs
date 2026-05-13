use std::sync::Arc;
use std::time::Duration;

use zihuan_core::error::Result;
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;

use crate::llm_api::LLMAPI;
use crate::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use crate::system_config::{load_llm_refs, ModelRefSpec};

pub const LLM_KIND_FIELD: &str = "llm_kind";

pub fn build_llm_from_ref_id(llm_ref_id: Option<&str>) -> Result<Arc<dyn LLMBase>> {
    let llm_ref_id = llm_ref_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            zihuan_core::error::Error::ValidationError("llm_ref_id is required".to_string())
        })?;

    let llm_ref = load_llm_refs()?
        .into_iter()
        .find(|item| item.id == llm_ref_id || item.config_id == llm_ref_id)
        .ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!(
                "llm_ref '{}' not found",
                llm_ref_id
            ))
        })?;

    if !llm_ref.enabled {
        return Err(zihuan_core::error::Error::ValidationError(format!(
            "llm_ref '{}' is disabled",
            llm_ref.name
        )));
    }

    let ModelRefSpec::ChatLlm { llm } = llm_ref.model else {
        return Err(zihuan_core::error::Error::ValidationError(format!(
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
        .ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(
                "embedding_model_ref_id is required".to_string(),
            )
        })?;

    zihuan_core::runtime::block_async(
        RuntimeEmbeddingModelManager::shared()
            .get_or_create_embedding_model(embedding_model_ref_id),
    )
}
