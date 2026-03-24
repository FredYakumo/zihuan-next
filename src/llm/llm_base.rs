use crate::llm::model::{InferenceParam, Message};

pub trait LLMBase: std::fmt::Debug {
    fn get_model_name(&self) -> &str;

    fn inference(&self, param: &InferenceParam) -> Message;
}