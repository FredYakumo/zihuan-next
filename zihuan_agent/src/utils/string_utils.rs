/// Checks whether the bracket directive `[no reply]` (case-insensitive) is present.
pub fn is_no_reply_directive(inner: &str) -> bool {
    inner.eq_ignore_ascii_case("no reply")
}
