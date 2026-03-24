use crate::llm::model::MessageRole;

/// Parse a role string from chat APIs into MessageRole
pub fn str_to_role(s: &str) -> MessageRole {
    match s {
        "system" => MessageRole::System,
        "user" => MessageRole::User,
        "tool" => MessageRole::Tool,
        _ => MessageRole::Assistant,
    }
}