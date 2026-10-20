use std::sync::Arc;
use std::time::Duration;

use crate::agent::EmbeddingServiceConfig;
use crate::config::llm_refs::{load_llm_refs, LlmRefConfig};
use crate::error::{Error, Result};
use crate::model_inference::linalg::embedding_api::EmbeddingAPI;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::model_config::{LlmServiceConfig, ModelRefSpec};
use crate::model_inference::model_factory::build_llm;
use crate::model_inference::nn::queued_embedding_model::QueuedEmbeddingModel;

/// Resolves the chat LLM service config `owner` references through `llm_ref_id`.
///
/// The single resolver for model references: callers holding a loaded
/// [`LlmRefConfig`] list (role service loaders) resolve against it, while
/// callers without one use [`build_llm_from_ref_id`]. The resolved config is
/// tagged with the ref's canonical config id so inference usage can be
/// attributed.
pub fn resolve_llm_service_config(
    llm_ref_id: Option<&str>,
    llm_refs: &[LlmRefConfig],
) -> Result<LlmServiceConfig> {
    let ref_id = llm_ref_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::ValidationError("missing llm_ref_id".into()))?;
    let llm_ref = find_llm_ref(llm_refs, ref_id)
        .ok_or_else(|| Error::ValidationError(format!("references missing llm_ref '{ref_id}'")))?;
    if !llm_ref.enabled {
        return Err(Error::ValidationError(format!(
            "references disabled llm_ref '{}'",
            llm_ref.name
        )));
    }
    match &llm_ref.model {
        ModelRefSpec::ChatLlm { llm } => {
            let mut config = llm.clone();
            config.usage_config_id = Some(llm_ref.canonical_config_id().to_string());
            Ok(config)
        }
        ModelRefSpec::TextEmbeddingLocal { .. } => Err(Error::ValidationError(format!(
            "references non-chat model_ref '{}' as llm_ref",
            llm_ref.name
        ))),
    }
}

/// Resolves the local embedding model name `owner` references through
/// `embedding_model_ref_id`; `Ok(None)` when the reference is absent.
pub fn resolve_local_embedding_model_name(
    embedding_model_ref_id: Option<&str>,
    llm_refs: &[LlmRefConfig],
    owner: &str,
) -> Result<Option<String>> {
    let Some(ref_id) = embedding_model_ref_id.map(str::trim).filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let llm_ref = find_llm_ref(llm_refs, ref_id).ok_or_else(|| {
        Error::ValidationError(format!(
            "'{owner}' references missing embedding model_ref '{ref_id}'"
        ))
    })?;
    if !llm_ref.enabled {
        return Err(Error::ValidationError(format!(
            "'{owner}' references disabled embedding model_ref '{}'",
            llm_ref.name
        )));
    }
    llm_ref.local_embedding_model_name().map(Some)
}

/// Loads the global llm_refs and builds the chat LLM references.
pub fn build_llm_from_ref_id(llm_ref_id: Option<&str>) -> Result<Arc<dyn LLMBase>> {
    let config = resolve_llm_service_config(llm_ref_id, &load_llm_refs()?)?;
    build_llm_model(&config)
}

pub fn build_llm_model(config: &LlmServiceConfig) -> Result<Arc<dyn LLMBase>> {
    build_llm(config.clone())
}

pub fn build_embedding_model(config: &EmbeddingServiceConfig) -> Arc<dyn EmbeddingBase> {
    Arc::new(
        EmbeddingAPI::new(
            config.model_name.clone(),
            config.api_endpoint.clone(),
            config.api_key.clone(),
            Duration::from_secs(config.timeout_secs),
        )
        .with_retry_count(config.retry_count),
    )
}

pub fn build_local_embedding_model(model_name: &str) -> Result<Arc<dyn EmbeddingBase>> {
    Ok(Arc::new(QueuedEmbeddingModel::new(model_name.to_string())?))
}

fn find_llm_ref<'a>(llm_refs: &'a [LlmRefConfig], ref_id: &str) -> Option<&'a LlmRefConfig> {
    llm_refs.iter().find(|item| item.id == ref_id || item.config_id == ref_id)
}
