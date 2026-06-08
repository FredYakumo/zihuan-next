use crate::models::message::Message;

#[macro_export]
macro_rules! sender_display_name {
    ($sender_name:expr, $sender_card:expr) => {{
        let card = $sender_card.trim();
        if card.is_empty() {
            $sender_name.to_string()
        } else {
            card.to_string()
        }
    }};
}
pub use crate::sender_display_name;

/// Checks whether any message in the list carries actual content (text, image, or
/// recursively meaningful reply/forward content). A depth limit of 8 prevents
/// infinite recursion from deeply nested or cyclic structures.
pub fn messages_have_effective_content(messages: &[Message], depth: usize) -> bool {
    if depth > 8 {
        return false;
    }

    for message in messages {
        match message {
            Message::PlainText(plain) => {
                if !plain.text.trim().is_empty() {
                    return true;
                }
            }
            Message::Image(_) => return true,
            Message::Forward(forward) => {
                if forward
                    .content
                    .iter()
                    .any(|node| messages_have_effective_content(&node.content, depth + 1))
                {
                    return true;
                }
            }
            Message::Reply(reply) => {
                if let Some(source_messages) = reply.message_source.as_deref() {
                    if matches!(source_messages, [Message::Reply(_)]) {
                        continue;
                    }
                    if messages_have_effective_content(source_messages, depth + 1) {
                        return true;
                    }
                }
            }
            Message::At(_) => {}
        }
    }

    false
}
