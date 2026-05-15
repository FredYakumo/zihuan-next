use zihuan_core::llm::{ContentPart, MessageContent, OpenAIMessage};

const IMAGE_OMITTED_PLACEHOLDER: &str = "[image omitted]";
const VIDEO_OMITTED_PLACEHOLDER: &str = "[video omitted]";

pub fn downgrade_messages_for_model(
    messages: Vec<OpenAIMessage>,
    supports_multimodal_input: bool,
) -> Vec<OpenAIMessage> {
    if supports_multimodal_input {
        return messages;
    }

    messages
        .into_iter()
        .map(downgrade_message_for_text_only_model)
        .collect()
}

pub fn downgrade_message_for_model(
    message: OpenAIMessage,
    supports_multimodal_input: bool,
) -> OpenAIMessage {
    if supports_multimodal_input {
        return message;
    }

    downgrade_message_for_text_only_model(message)
}

fn downgrade_message_for_text_only_model(mut message: OpenAIMessage) -> OpenAIMessage {
    if let Some(MessageContent::Parts(parts)) = message.content {
        message.content = Some(MessageContent::Text(parts_to_text(parts)));
    }
    message
}

fn parts_to_text(parts: Vec<ContentPart>) -> String {
    let mut segments = Vec::with_capacity(parts.len());

    for part in parts {
        match part {
            ContentPart::Text { text } => segments.push(text),
            ContentPart::ImageUrl { image_url } => {
                segments.push(media_placeholder(
                    IMAGE_OMITTED_PLACEHOLDER,
                    image_url.as_url(),
                ));
            }
            ContentPart::VideoUrl { video_url } => {
                segments.push(media_placeholder(
                    VIDEO_OMITTED_PLACEHOLDER,
                    video_url.as_url(),
                ));
            }
        }
    }

    segments.join("\n")
}

fn media_placeholder(prefix: &str, url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix} {trimmed}")
    }
}
