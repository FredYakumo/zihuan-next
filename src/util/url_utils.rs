/// Extract host from URL for WebSocket handshake
pub fn extract_host(url: &str) -> Option<&str> {
    url.strip_prefix("ws://")
        .or_else(|| url.strip_prefix("wss://"))
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.split(':').next())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_host() {
        assert_eq!(extract_host("ws://localhost:3001"), Some("localhost"));
        assert_eq!(extract_host("wss://example.com/path"), Some("example.com"));
        assert_eq!(extract_host("ws://192.168.1.1:8080/ws"), Some("192.168.1.1"));
    }
}