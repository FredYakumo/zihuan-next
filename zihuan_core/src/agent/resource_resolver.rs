use std::sync::Arc;
use std::time::Duration;

use crate::inference::linalg::embedding_api::EmbeddingAPI;
use crate::inference::nn::queued_embedding_model::QueuedEmbeddingModel;
use crate::inference::nodes::llm_node::build_llm;
use crate::inference::system_config::{LlmRefConfig, LlmServiceConfig, ModelRefSpec};
use crate::agent::EmbeddingServiceConfig;
use crate::error::{Error, Result};
use crate::llm::embedding_base::EmbeddingBase;
use crate::llm::llm_base::LLMBase;

pub fn resolve_llm_service_config(
    llm_ref_id: Option<&str>,
    llm_refs: &[LlmRefConfig],
    agent_name: &str,
) -> Result<LlmServiceConfig> {
    let ref_id = llm_ref_id
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| Error::ValidationError(format!("agent '{}' is missing llm_ref_id", agent_name)))?;
    let llm_ref = llm_refs.iter().find(|item| item.id == ref_id).ok_or_else(|| {
        Error::ValidationError(format!("agent '{}' references missing llm_ref '{}'", agent_name, ref_id))
    })?;
    if !llm_ref.enabled {
        return Err(Error::ValidationError(format!(
            "agent '{}' references disabled llm_ref '{}'",
            agent_name, llm_ref.name
        )));
    }
    match &llm_ref.model {
        ModelRefSpec::ChatLlm { llm } => Ok(llm.clone()),
        ModelRefSpec::TextEmbeddingLocal { .. } => Err(Error::ValidationError(format!(
            "agent '{}' references non-chat model_ref '{}' as llm_ref",
            agent_name, llm_ref.name
        ))),
    }
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

pub fn resolve_local_embedding_model_name(
    embedding_model_ref_id: Option<&str>,
    llm_refs: &[LlmRefConfig],
    agent_name: &str,
) -> Result<Option<String>> {
    let Some(ref_id) = embedding_model_ref_id.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };

    let llm_ref = llm_refs.iter().find(|item| item.id == ref_id).ok_or_else(|| {
        Error::ValidationError(format!(
            "agent '{}' references missing embedding_model_ref '{}'",
            agent_name, ref_id
        ))
    })?;
    if !llm_ref.enabled {
        return Err(Error::ValidationError(format!(
            "agent '{}' references disabled embedding model_ref '{}'",
            agent_name, llm_ref.name
        )));
    }

    match &llm_ref.model {
        ModelRefSpec::TextEmbeddingLocal { model_name } => Ok(Some(model_name.clone())),
        ModelRefSpec::ChatLlm { .. } => Err(Error::ValidationError(format!(
            "agent '{}' references chat model_ref '{}' as embedding_model_ref",
            agent_name, llm_ref.name
        ))),
    }
}

pub fn build_local_embedding_model(model_name: &str) -> Result<Arc<dyn EmbeddingBase>> {
    Ok(Arc::new(QueuedEmbeddingModel::new(model_name.to_string())?))
}
