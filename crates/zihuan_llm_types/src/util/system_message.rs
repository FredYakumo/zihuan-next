use crate::model::OpenAIMessage;

#[allow(non_snake_case)]
pub fn SystemMessage<S: Into<String>>(content: S) -> OpenAIMessage {
    OpenAIMessage::system(content)
}
