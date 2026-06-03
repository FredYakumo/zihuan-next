use crate::llm::model::LLMMessage;

#[allow(non_snake_case)]
pub fn UserMessage<S: Into<String>>(content: S) -> LLMMessage {
    LLMMessage::user(content)
}
