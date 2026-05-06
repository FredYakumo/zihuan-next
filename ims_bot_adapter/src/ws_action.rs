use crate::adapter::SharedBotAdapter;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::oneshot;
use tokio::task::block_in_place;
use zihuan_core::error::Result;

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

pub fn qq_message_list_to_json(messages: &[crate::models::message::Message]) -> serde_json::Value {
    serde_json::Value::Array(
        messages
            .iter()
            .map(|m| match m {
                crate::models::message::Message::Image(image) => serde_json::json!({
                    "type": "image",
                    "data": {
                        "file": image.file.clone(),
                        "path": image
                            .path
                            .clone()
                            .or_else(|| image.local_path.clone()),
                        "url": image
                            .url
                            .clone()
                            .or_else(|| image.object_url.clone()),
                        "name": image.name.clone(),
                        "thumb": image.thumb.clone(),
                        "summary": image.summary.clone(),
                        "sub_type": image.sub_type,
                    }
                }),
                _ => serde_json::to_value(m).unwrap_or(serde_json::Value::Null),
            })
            .collect(),
    )
}

pub async fn ws_send_action_async(
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

    // Extract action_tx and pending_actions without holding the adapter lock.
    let (action_tx, pending_actions) = {
        let guard = adapter_ref.lock().await;
        let tx = guard.action_tx.clone().ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(
                "Bot adapter WebSocket not connected yet".to_string(),
            )
        })?;
        let pending = guard.pending_actions.clone();
        Ok::<_, zihuan_core::error::Error>((tx, pending))
    }?;

    let (tx, rx) = oneshot::channel::<serde_json::Value>();
    pending_actions.lock().await.insert(echo.clone(), tx);

    action_tx.send(payload.to_string()).map_err(|_| {
        zihuan_core::error::Error::ValidationError("Failed to enqueue WebSocket action".to_string())
    })?;

    // Wait for the response (30 s timeout).
    let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
        .await
        .map_err(|_| {
            zihuan_core::error::Error::ValidationError(format!(
                "Action '{}' timed out after 30 s",
                action_name
            ))
        })?
        .map_err(|_| {
            zihuan_core::error::Error::ValidationError(
                "Response channel closed unexpectedly".to_string(),
            )
        })?;

    Ok(response)
}

pub fn ws_send_action(
    adapter_ref: &SharedBotAdapter,
    action_name: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let run = ws_send_action_async(adapter_ref, action_name, params);

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}
