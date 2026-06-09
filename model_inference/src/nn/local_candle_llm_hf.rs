use std::sync::Arc;

use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;

use crate::nn::local_llm_registry::get_local_llm_model_info;
use crate::system_config::LlmServiceConfig;

pub fn build_local_candle_hf_llm(config: LlmServiceConfig) -> Result<Arc<dyn LLMBase>> {
    let model_info = get_local_llm_model_info(&config.model_name)?;
    Err(Error::ValidationError(model_info.reason.unwrap_or_else(|| {
        "standard HF local Candle runtime is not implemented yet; choose api_style=candle_hf only after the HF runtime is added"
            .to_string()
    })))
}
