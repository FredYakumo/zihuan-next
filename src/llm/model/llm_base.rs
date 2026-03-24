use super::{inference_param::InferenceParam, message::Message};

pub trait LLMBase: std::fmt::Debug {
    fn get_model_name(&self) -> &str;

    fn inference(&self, param: &InferenceParam) -> Message;
}