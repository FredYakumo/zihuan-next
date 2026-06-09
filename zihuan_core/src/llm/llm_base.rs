use crate::llm::model::{InferenceParam, LLMMessage};
use crate::llm::StreamToken;
use tokio::sync::mpsc;

pub trait LLMBase: std::fmt::Debug + Send + Sync {
    fn get_model_name(&self) -> &str;

    fn api_style(&self) -> Option<&str> {
        None
    }

    fn supports_multimodal_input(&self) -> bool {
        false
    }

    fn inference(&self, param: &InferenceParam) -> LLMMessage;

    fn as_streaming(&self) -> Option<&dyn StreamingLLMBase> {
        None
    }
}

pub trait StreamingLLMBase: LLMBase {
    fn inference_streaming<'a>(
        &'a self,
        param: &'a InferenceParam<'a>,
        token_tx: mpsc::UnboundedSender<StreamToken>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LLMMessage> + Send + 'a>>;
}
