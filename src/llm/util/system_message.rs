use crate::llm::model::Message;

#[allow(non_snake_case)]
pub fn SystemMessage<S: Into<String>>(content: S) -> Message {
    Message::system(content)
}