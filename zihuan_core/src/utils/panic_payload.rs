use std::any::Any;

/// Text carried by a panic payload, falling back to a placeholder when the
/// panic was raised with a non-string payload.
pub fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "unknown panic".to_string()
    }
}
