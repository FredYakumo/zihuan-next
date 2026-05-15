// String utility functions for zihuan_core

/// Shortens a string to the specified character limit, appending "...(truncated)" if truncated.
pub fn shorten_text(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let truncated: String = text.chars().take(limit).collect();
    format!("{}...(truncated)", truncated)
}
