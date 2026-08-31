use std::sync::Arc;

use crate::error::Result;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm_api::LLMAPI;
use crate::model_inference::model_config::{LlmApiStyle, LlmServiceConfig};
use crate::model_inference::nn::local_candle_llm_gguf::build_local_candle_gguf_llm;
use crate::model_inference::nn::local_candle_llm_hf::build_local_candle_hf_llm;

pub fn build_llm(config: LlmServiceConfig) -> Result<Arc<dyn LLMBase>> {
    match config.api_style {
        LlmApiStyle::OpenAiChatCompletions
        | LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat
        | LlmApiStyle::OpenAiResponses
        | LlmApiStyle::OpenAiResponsesMessageCompat
        | LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
            let api = LLMAPI::new(
                config.model_name,
                config.api_endpoint,
                config.api_key,
                config.api_style,
                config.stream,
                config.supports_multimodal_input,
                config.include_reasoning_content,
                config.thinking_type,
                config.reasoning_effort,
                config.context_length,
                std::time::Duration::from_secs(config.timeout_secs),
            )
            .with_retry_count(config.retry_count);
            Ok(Arc::new(api))
        }
        LlmApiStyle::CandleGguf => build_local_candle_gguf_llm(config),
        LlmApiStyle::CandleHf => build_local_candle_hf_llm(config),
    }
}
