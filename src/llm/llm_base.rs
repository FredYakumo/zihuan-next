use crate::llm::model::{InferenceParam, OpenAIMessage};

pub trait LLMBase: std::fmt::Debug + Send + Sync {
    fn get_model_name(&self) -> &str;

    fn inference(&self, param: &InferenceParam) -> OpenAIMessage;
}
