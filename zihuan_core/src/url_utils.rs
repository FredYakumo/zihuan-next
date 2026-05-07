/// Extract host from URL for WebSocket handshake
pub fn extract_host(url: &str) -> Option<&str> {
    url.strip_prefix("ws://")
        .or_else(|| url.strip_prefix("wss://"))
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.split(':').next())
}

/// Percent-encode a password for safe inclusion in a URL
pub fn pct_encode(input: &str) -> String {
    // Encode everything except unreserved characters per RFC 3986: ALPHA / DIGIT / '-' / '.' / '_' / '~'
    let mut out = String::new();
    for &b in input.as_bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '~' {
            out.push(c);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}
