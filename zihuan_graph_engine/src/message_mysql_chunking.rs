use log::warn;

pub const MESSAGE_ID_MAX_CHARS: usize = 64;
pub const SENDER_ID_MAX_CHARS: usize = 64;
pub const SENDER_NAME_MAX_CHARS: usize = 128;
pub const GROUP_ID_MAX_CHARS: usize = 64;
pub const GROUP_NAME_MAX_CHARS: usize = 128;
pub const CONTENT_MAX_CHARS: usize = 2048;
pub const AT_TARGET_LIST_MAX_CHARS: usize = 512;
pub const MEDIA_ID_LIST_MAX_CHARS: usize = 1024;
pub const MEDIA_JSON_MAX_CHARS: usize = 4096;

fn truncate_to_char_limit(value: &str, max_chars: usize) -> Option<String> {
    if value.chars().count() <= max_chars {
        return None;
    }

    let truncated_end = value
        .char_indices()
        .nth(max_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(value.len());
    Some(value[..truncated_end].to_string())
}

pub fn truncate_field_if_needed(
    field_name: &str,
    value: String,
    max_chars: usize,
    message_id: &str,
) -> String {
    let original_chars = value.chars().count();
    if let Some(truncated) = truncate_to_char_limit(&value, max_chars) {
        warn!(
            "[MessageMySQLPersistence] Truncated field '{}' for message {} from {} to {} chars",
            field_name, message_id, original_chars, max_chars
        );
        truncated
    } else {
        value
    }
}

pub fn truncate_optional_field_if_needed(
    field_name: &str,
    value: Option<String>,
    max_chars: usize,
    message_id: &str,
) -> Option<String> {
    value.map(|value| truncate_field_if_needed(field_name, value, max_chars, message_id))
}

pub fn split_content_chunks(content: &str, max_chars: usize) -> Vec<String> {
    if content.is_empty() {
        return vec![String::new()];
    }

    let mut chunks = Vec::new();
    let mut chunk_start = 0usize;
    let mut chunk_chars = 0usize;

    for (idx, _) in content.char_indices() {
        if chunk_chars == max_chars {
            chunks.push(content[chunk_start..idx].to_string());
            chunk_start = idx;
            chunk_chars = 0;
        }
        chunk_chars += 1;
    }

    chunks.push(content[chunk_start..].to_string());
    chunks
}
