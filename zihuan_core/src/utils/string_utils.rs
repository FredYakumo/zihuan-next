// String utility functions for zihuan_core

/// Derives an S3 object key from a remote URL, stripping the scheme and host
/// and sanitizing the path so only safe characters remain for S3 storage.
pub fn derive_tavily_s3_key(url: &str) -> String {
    let after_scheme = url.find("://").map(|i| &url[i + 3..]).unwrap_or(url);
    let path_start = after_scheme.find('/').map(|i| i + 1).unwrap_or(0);
    let path = after_scheme[path_start..].split('?').next().unwrap_or("");
    let sanitized: String = path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('/');
    if trimmed.is_empty() {
        "tavily/image.jpg".to_string()
    } else {
        format!("tavily/{}", &trimmed[..trimmed.len().min(200)])
    }
}

/// Strips leading @Bot mentions from the message text. Supports both English `@` and
/// Chinese `＠` prefixes, matching against `bot_id` and `bot_name` patterns. After removing
/// a mention, trailing punctuation separators (whitespace, commas, periods, colons, etc.)
/// are also trimmed. The loop repeats until no more mention prefixes are found, handling
/// consecutive mentions.
pub fn strip_leading_bot_mention(text: &str, bot_id: &str, bot_name: &str) -> String {
    let mut remaining = text.trim_start();
    loop {
        let mut stripped = false;

        for pattern in [bot_id, bot_name] {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                continue;
            }

            for prefix in [format!("@{pattern}"), format!("＠{pattern}")] {
                if let Some(rest) = remaining.strip_prefix(&prefix) {
                    remaining = rest.trim_start_matches(|c: char| {
                        matches!(
                            c,
                            ' ' | '\t'
                                | '\n'
                                | '\r'
                                | ','
                                | '，'
                                | '。'
                                | ':'
                                | '：'
                                | '!'
                                | '！'
                                | '?'
                                | '？'
                        )
                    });
                    stripped = true;
                    break;
                }
            }

            if stripped {
                break;
            }
        }

        if !stripped {
            break;
        }
    }

    remaining.trim().to_string()
}

/// Shortens a string to the specified character limit, appending "...(truncated)" if truncated.
pub fn shorten_text(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let truncated: String = text.chars().take(limit).collect();
    format!("{}...(truncated)", truncated)
}

/// Parses a tag-style value, stripping optional matching single or double quotes.
/// Returns `None` if the trimmed value is empty; otherwise returns the unquoted content.
pub fn parse_tag_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.len() >= 2 {
        let quoted = trimmed
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| {
                trimmed
                    .strip_prefix('\'')
                    .and_then(|value| value.strip_suffix('\''))
            });
        if let Some(value) = quoted {
            let inner = value.trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }

    Some(trimmed.to_string())
}

use serde_json::Value;

/// Extracts an optional owned `String` from a `serde_json::Value` map by key.
/// Returns `None` if the key is missing, the value is not a string, or the string
/// is empty/blank.
pub fn extract_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}
