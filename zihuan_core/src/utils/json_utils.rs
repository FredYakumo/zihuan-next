//! Shared helpers for reading string fields out of a JSON object: tool call arguments,
//! storage record payloads, and adapter response payloads all use the same shape.

use serde_json::Value;

use crate::error::{Error, Result};

/// Extracts a trimmed, non-empty string from a JSON object field if present.
pub fn optional_string_argument(arguments: &Value, name: &str) -> Option<String> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Like [`optional_string_argument`], but fails with `{name} is required` when absent.
pub fn required_string_argument(arguments: &Value, name: &str) -> Result<String> {
    optional_string_argument(arguments, name)
        .ok_or_else(|| Error::ValidationError(format!("{name} is required")))
}
