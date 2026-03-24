use crate::llm::model::Message;

/// Shortcut to construct a user message.
#[allow(non_snake_case)]
pub fn UserMessage<S: Into<String>>(content: S) -> Message {
    Message::user(content)
}