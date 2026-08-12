/// Checks whether the bracket directive `[no reply]` or `[no_reply]` (case-insensitive) is present.
pub fn is_no_reply_directive(inner: &str) -> bool {
    let stripped = inner.strip_prefix('[').and_then(|s| s.strip_suffix(']')).unwrap_or(inner);
    let normalized = stripped.trim().replace('_', " ");
    normalized.eq_ignore_ascii_case("no reply")
}
