use crate::bot_adapter::adapter::SharedBotAdapter;
use crate::error::Result;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::oneshot;
use tokio::task::block_in_place;

/// Global counter for generating unique echo IDs.
static ECHO_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn next_echo() -> String {
    format!("zhn_echo_{}", ECHO_COUNTER.fetch_add(1, Ordering::Relaxed))
}

pub fn json_i64(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.parse::<i64>().ok(),
        _ => None,
    }
}

pub fn response_success(response: &Value) -> bool {
    if let Some(retcode) = json_i64(response.get("retcode")) {
        return retcode == 0;
    }

    response
        .get("status")
        .and_then(|value| value.as_str())
        .map(|status| status.eq_ignore_ascii_case("ok"))
        .unwrap_or(false)
}

pub fn response_message_id(response: &Value) -> Option<i64> {
    response
        .get("data")
        .and_then(|data| json_i64(data.get("message_id")))
}

pub fn qq_message_list_to_json(
    messages: &[crate::bot_adapter::models::message::Message],
) -> serde_json::Value {
    serde_json::Value::Array(
        messages
            .iter()
            .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
            .collect(),
    )
}

pub fn ws_send_action(
    adapter_ref: &SharedBotAdapter,
    action_name: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let echo = next_echo();
    let payload = serde_json::json!({
        "action": action_name,
        "params": params,
        "echo": echo,
    });

    let adapter_ref = adapter_ref.clone();
    let action_name = action_name.to_string();

    let run = async move {
        // Extract action_tx and pending_actions without holding the adapter lock.
        let (action_tx, pending_actions) = {
            let guard = adapter_ref.lock().await;
            let tx = guard.action_tx.clone().ok_or_else(|| {
                crate::error::Error::ValidationError(
                    "Bot adapter WebSocket not connected yet".to_string(),
                )
            })?;
            let pending = guard.pending_actions.clone();
            Ok::<_, crate::error::Error>((tx, pending))
        }?;

        let (tx, rx) = oneshot::channel::<serde_json::Value>();
        pending_actions.lock().await.insert(echo.clone(), tx);

        action_tx.send(payload.to_string()).map_err(|_| {
            crate::error::Error::ValidationError("Failed to enqueue WebSocket action".to_string())
        })?;

        // Wait for the response (30 s timeout).
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| {
                crate::error::Error::ValidationError(format!(
                    "Action '{}' timed out after 30 s",
                    action_name
                ))
            })?
            .map_err(|_| {
                crate::error::Error::ValidationError(
                    "Response channel closed unexpectedly".to_string(),
                )
            })?;

        Ok(response)
    };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}

#[cfg(test)]
mod tests {
    use super::{json_i64, response_message_id, response_success};
    use serde_json::json;

    #[test]
    fn json_i64_accepts_number_and_string() {
        assert_eq!(json_i64(Some(&json!(0))), Some(0));
        assert_eq!(json_i64(Some(&json!("42"))), Some(42));
        assert_eq!(json_i64(Some(&json!("oops"))), None);
        assert_eq!(json_i64(Some(&json!(true))), None);
        assert_eq!(json_i64(None), None);
    }

    #[test]
    fn response_success_supports_retcode_and_status() {
        assert!(response_success(&json!({ "retcode": 0 })));
        assert!(response_success(&json!({ "retcode": "0" })));
        assert!(response_success(&json!({ "status": "ok" })));
        assert!(!response_success(&json!({ "retcode": 1, "status": "ok" })));
        assert!(!response_success(&json!({ "status": "failed" })));
        assert!(!response_success(&json!({})));
    }

    #[test]
    fn response_message_id_supports_string_or_number() {
        assert_eq!(
            response_message_id(&json!({ "data": { "message_id": 123 } })),
            Some(123)
        );
        assert_eq!(
            response_message_id(&json!({ "data": { "message_id": "456" } })),
            Some(456)
        );
        assert_eq!(response_message_id(&json!({ "data": {} })), None);
    }
}
