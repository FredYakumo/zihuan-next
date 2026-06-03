use crate::llm::model::LLMMessage;

#[allow(non_snake_case)]
pub fn SystemMessage<S: Into<String>>(content: S) -> LLMMessage {
    LLMMessage::system(content)
}
