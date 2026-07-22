/// Extract host from URL for WebSocket handshake
pub fn extract_host(url: &str) -> Option<&str> {
    url.strip_prefix("ws://")
        .or_else(|| url.strip_prefix("wss://"))
        .and_then(|s| s.split('/').next())
        .and_then(|s| s.split(':').next())
}

/// Infers an image MIME type from a URL by inspecting the path extension.
pub fn content_type_from_url(url: &str) -> &'static str {
    let path = url.split('?').next().unwrap_or(url).to_lowercase();
    match path.rsplit('.').next().unwrap_or("") {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        _ => "image/jpeg",
    }
}

/// Normalizes an image content type supported by QQ media upload.
pub fn supported_image_content_type(content_type: &str) -> Option<&'static str> {
    match content_type.split(';').next()?.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" => Some("image/jpeg"),
        "image/png" => Some("image/png"),
        "image/gif" => Some("image/gif"),
        "image/webp" => Some("image/webp"),
        "image/bmp" => Some("image/bmp"),
        "image/avif" => Some("image/avif"),
        _ => None,
    }
}

/// Detects a supported image content type from its binary signature.
pub fn image_content_type_from_bytes(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if bytes.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && matches!(&bytes[8..12], b"avif" | b"avis") {
        return Some("image/avif");
    }
    None
}

/// Returns the usual filename extension for a supported image content type.
pub fn image_extension_for_content_type(content_type: &str) -> &'static str {
    match content_type {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/avif" => "avif",
        _ => "img",
    }
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
