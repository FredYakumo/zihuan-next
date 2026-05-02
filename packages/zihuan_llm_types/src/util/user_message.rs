use crate::model::OpenAIMessage;

#[allow(non_snake_case)]
pub fn UserMessage<S: Into<String>>(content: S) -> OpenAIMessage {
    OpenAIMessage::user(content)
}
