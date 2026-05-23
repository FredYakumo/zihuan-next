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

/// Shortens a string to the specified character limit, appending "...(truncated)" if truncated.
pub fn shorten_text(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let truncated: String = text.chars().take(limit).collect();
    format!("{}...(truncated)", truncated)
}
