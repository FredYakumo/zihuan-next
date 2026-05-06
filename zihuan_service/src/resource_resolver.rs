use std::sync::Arc;
use std::time::Duration;

use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_llm::linalg::embedding_api::EmbeddingAPI;
use zihuan_llm::llm_api::LLMAPI;
use zihuan_llm::system_config::{EmbeddingServiceConfig, LlmRefConfig, LlmServiceConfig};

pub fn resolve_llm_service_config(
    llm_ref_id: Option<&str>,
    legacy_llm: Option<&LlmServiceConfig>,
    llm_refs: &[LlmRefConfig],
    agent_name: &str,
) -> Result<LlmServiceConfig> {
    if let Some(ref_id) = llm_ref_id.filter(|value| !value.trim().is_empty()) {
        let llm_ref = llm_refs
            .iter()
            .find(|item| item.id == ref_id)
            .ok_or_else(|| {
                Error::ValidationError(format!(
                    "agent '{}' references missing llm_ref '{}'",
                    agent_name, ref_id
                ))
            })?;
        if !llm_ref.enabled {
            return Err(Error::ValidationError(format!(
                "agent '{}' references disabled llm_ref '{}'",
                agent_name, llm_ref.name
            )));
        }
        return Ok(llm_ref.llm.clone());
    }

    legacy_llm.cloned().ok_or_else(|| {
        Error::ValidationError(format!(
            "agent '{}' is missing llm config: set llm_ref_id or legacy llm",
            agent_name
        ))
    })
}

pub fn build_llm_model(config: &LlmServiceConfig) -> Arc<dyn LLMBase> {
    Arc::new(
        LLMAPI::new(
            config.model_name.clone(),
            config.api_endpoint.clone(),
            config.api_key.clone(),
            config.stream,
            config.supports_multimodal_input,
            Duration::from_secs(config.timeout_secs),
        )
        .with_retry_count(config.retry_count),
    )
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
