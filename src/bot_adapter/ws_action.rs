use crate::bot_adapter::adapter::SharedBotAdapter;
use crate::error::Result;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::oneshot;
use tokio::task::block_in_place;

/// Global counter for generating unique echo IDs.
static ECHO_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn next_echo() -> String {
    format!("zhn_echo_{}", ECHO_COUNTER.fetch_add(1, Ordering::Relaxed))
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

        action_tx
            .send(payload.to_string())
            .map_err(|_| crate::error::Error::ValidationError(
                "Failed to enqueue WebSocket action".to_string(),
            ))?;

        // Wait for the response (30 s timeout).
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            rx,
        )
        .await
        .map_err(|_| crate::error::Error::ValidationError(
            format!("Action '{}' timed out after 30 s", action_name),
        ))?
        .map_err(|_| crate::error::Error::ValidationError(
            "Response channel closed unexpectedly".to_string(),
        ))?;

        Ok(response)
    };

    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(run))
    } else {
        tokio::runtime::Runtime::new()?.block_on(run)
    }
}
