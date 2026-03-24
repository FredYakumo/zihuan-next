use crate::llm::model::Message;

#[allow(non_snake_case)]
pub fn UserMessage<S: Into<String>>(content: S) -> Message {
    Message::user(content)
}