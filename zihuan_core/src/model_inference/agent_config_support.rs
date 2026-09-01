use std::sync::Arc;

use crate::error::Result;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::model_inference::llm::llm_base::LLMBase;

use crate::config::llm_refs::load_llm_refs;
use crate::model_inference::model_config::ModelRefSpec;
use crate::model_inference::model_factory::build_llm;
use crate::model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;

pub const LLM_KIND_FIELD: &str = "llm_kind";

pub fn build_llm_from_ref_id(llm_ref_id: Option<&str>) -> Result<Arc<dyn LLMBase>> {
    let llm_ref_id =
        llm_ref_id.map(str::trim).filter(|value| !value.is_empty()).ok_or_else(|| {
            crate::error::Error::ValidationError("llm_ref_id is required".to_string())
        })?;

    let llm_ref = load_llm_refs()?
        .into_iter()
        .find(|item| item.id == llm_ref_id || item.config_id == llm_ref_id)
        .ok_or_else(|| {
            crate::error::Error::ValidationError(format!("llm_ref '{}' not found", llm_ref_id))
        })?;

    if !llm_ref.enabled {
        return Err(crate::error::Error::ValidationError(format!(
            "llm_ref '{}' is disabled",
            llm_ref.name
        )));
    }

    let ModelRefSpec::ChatLlm { llm } = llm_ref.model else {
        return Err(crate::error::Error::ValidationError(format!(
            "llm_ref '{}' is not a chat LLM config",
            llm_ref.name
        )));
    };

    build_llm(llm)
}

pub fn build_embedding_from_ref_id(
    embedding_model_ref_id: Option<&str>,
) -> Result<Arc<dyn EmbeddingBase>> {
    let embedding_model_ref_id = embedding_model_ref_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            crate::error::Error::ValidationError("embedding_model_ref_id is required".to_string())
        })?;

    crate::runtime::block_async(
        RuntimeEmbeddingModelManager::shared()
            .get_or_create_embedding_model(embedding_model_ref_id),
    )
}
