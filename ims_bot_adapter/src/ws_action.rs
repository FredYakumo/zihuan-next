use crate::adapter::SharedBotAdapter;
use base64::Engine;
use log::warn;
use serde_json::Value;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::oneshot;
use tokio::task::block_in_place;
use zihuan_core::error::Result;
use zihuan_core::ims_bot_adapter::models::message::{ImageMessage, Message};
use zihuan_graph_engine::object_storage::S3Ref;

/// Global counter for generating unique echo IDs.
static ECHO_COUNTER: AtomicU64 = AtomicU64::new(0);
const LOG_PREFIX: &str = "[ws_action]";

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

pub fn qq_message_list_to_send_json(
    adapter_ref: &SharedBotAdapter,
    messages: &[Message],
) -> serde_json::Value {
    let normalized = normalize_messages_for_send(adapter_ref, messages);
    qq_message_list_to_json(&normalized)
}

fn normalize_messages_for_send(
    adapter_ref: &SharedBotAdapter,
    messages: &[Message],
) -> Vec<Message> {
    messages
        .iter()
        .map(|message| normalize_message_for_send(adapter_ref, message))
        .collect()
}

fn normalize_message_for_send(adapter_ref: &SharedBotAdapter, message: &Message) -> Message {
    match message {
        Message::Image(image) => Message::Image(normalize_image_for_send(adapter_ref, image)),
        Message::Forward(forward) => {
            let mut cloned = forward.clone();
            for node in &mut cloned.content {
                node.content = normalize_messages_for_send(adapter_ref, &node.content);
            }
            Message::Forward(cloned)
        }
        _ => message.clone(),
    }
}

fn normalize_image_for_send(adapter_ref: &SharedBotAdapter, image: &ImageMessage) -> ImageMessage {
    if let Some(local_path) = outbound_local_image_path(image) {
        let mut normalized = image.clone();
        normalized.file = Some(format!("file://{local_path}"));
        normalized.path = None;
        normalized.url = None;
        normalized.object_url = None;
        normalized.object_key = None;
        normalized.local_path = None;
        return normalized;
    }

    if let Some(base64_file) = outbound_base64_file(adapter_ref, image) {
        let mut normalized = image.clone();
        normalized.file = Some(base64_file);
        normalized.path = None;
        normalized.url = None;
        normalized.object_url = None;
        normalized.object_key = None;
        normalized.local_path = None;
        return normalized;
    }

    let mut normalized = image.clone();
    if normalized.file.is_none() {
        normalized.file = image
            .url
            .clone()
            .or_else(|| image.object_url.clone())
            .or_else(|| image.path.clone())
            .or_else(|| image.local_path.clone());
    }
    normalized
}

fn outbound_local_image_path(image: &ImageMessage) -> Option<String> {
    for path in [
        image.local_path.as_deref(),
        image.path.as_deref(),
        image
            .file
            .as_deref()
            .and_then(|value| value.strip_prefix("file://")),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(normalized) = normalize_existing_local_path(path) {
            return Some(normalized);
        }
    }
    None
}

fn normalize_existing_local_path(path: &str) -> Option<String> {
    let direct = Path::new(path);
    if direct.exists() {
        return Some(path.to_string());
    }

    let file_uri_candidate = path.replace('\\', "/");
    let file_uri_candidate = file_uri_candidate.trim_start_matches('/');
    if file_uri_candidate.len() >= 3 {
        let bytes = file_uri_candidate.as_bytes();
        let looks_like_windows_drive = bytes[1] == b':' && bytes[2] == b'/';
        if looks_like_windows_drive {
            let candidate = PathBuf::from(file_uri_candidate.replace('/', "\\"));
            if candidate.exists() {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }

    None
}

fn outbound_object_storage_key(
    adapter_ref: &SharedBotAdapter,
    image: &ImageMessage,
) -> Option<String> {
    if let Some(key) = image.object_key.as_deref().map(str::trim).filter(|key| !key.is_empty()) {
        return Some(key.to_string());
    }

    let object_storage = block_on_async(async {
        adapter_ref.lock().await.object_storage.clone()
    })?;

    for locator in [
        image.object_url.as_deref(),
        image.url.as_deref(),
        image.file.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(key) = object_key_from_locator(&object_storage, locator) {
            return Some(key);
        }
    }

    None
}

fn outbound_base64_file(adapter_ref: &SharedBotAdapter, image: &ImageMessage) -> Option<String> {
    if let Some(file) = image.file.as_deref() {
        if file.starts_with("base64://") {
            return Some(file.to_string());
        }
        if let Some(base64_payload) = file.strip_prefix("data:").and_then(data_url_base64_payload) {
            return Some(format!("base64://{base64_payload}"));
        }
    }

    if let Some(key) = outbound_object_storage_key(adapter_ref, image) {
        match block_on_async(download_object_storage_bytes(adapter_ref, &key)) {
            Ok(Some(bytes)) => {
                return Some(format!(
                    "base64://{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ));
            }
            Ok(None) => {}
            Err(error) => {
                warn!(
                    "{LOG_PREFIX} failed to read object storage image for outbound send key={}: {}",
                    key, error
                );
            }
        }
    }

    for url in [
        image.object_url.as_deref(),
        image.url.as_deref(),
        image
            .file
            .as_deref()
            .filter(|value| value.starts_with("http://") || value.starts_with("https://")),
    ]
    .into_iter()
    .flatten()
    {
        match block_on_async(download_remote_bytes(url)) {
            Ok(Some(bytes)) => {
                return Some(format!(
                    "base64://{}",
                    base64::engine::general_purpose::STANDARD.encode(bytes)
                ));
            }
            Ok(None) => {}
            Err(error) => {
                warn!(
                    "{LOG_PREFIX} failed to download outbound image url={}: {}",
                    url, error
                );
            }
        }
    }

    None
}

fn object_key_from_locator(object_storage: &S3Ref, locator: &str) -> Option<String> {
    let locator = locator.trim();
    if locator.is_empty() {
        return None;
    }

    let prefixes = object_storage_url_prefixes(object_storage);
    for prefix in prefixes {
        if let Some(rest) = locator.strip_prefix(&prefix) {
            let key = rest.trim_start_matches('/');
            if !key.is_empty() {
                return Some(key.to_string());
            }
        }
    }

    None
}

fn object_storage_url_prefixes(object_storage: &S3Ref) -> Vec<String> {
    let mut prefixes = Vec::new();

    if let Some(public_base_url) = object_storage.public_base_url.as_deref() {
        let public_base_url = public_base_url.trim_end_matches('/');
        if !public_base_url.is_empty() {
            prefixes.push(public_base_url.to_string());
        }
    }

    if object_storage.path_style {
        let endpoint_prefix = format!(
            "{}/{}",
            object_storage.endpoint.trim_end_matches('/'),
            object_storage.bucket.trim_matches('/')
        );
        prefixes.push(endpoint_prefix);
    } else if let Ok(endpoint) = reqwest::Url::parse(&object_storage.endpoint) {
        if let Some(host) = endpoint.host_str() {
            let scheme = endpoint.scheme();
            prefixes.push(format!(
                "{scheme}://{}.{}",
                object_storage.bucket.trim_matches('/'),
                host
            ));
        }
    }

    prefixes.sort();
    prefixes.dedup();
    prefixes
}

async fn download_object_storage_bytes(
    adapter_ref: &SharedBotAdapter,
    object_key: impl AsRef<str>,
) -> Result<Option<Vec<u8>>> {
    let object_storage = adapter_ref.lock().await.object_storage.clone();
    let Some(object_storage) = object_storage else {
        return Ok(None);
    };
    object_storage
        .get_object_bytes(object_key.as_ref())
        .await
        .map(Some)
}

async fn download_remote_bytes(url: &str) -> Result<Option<Vec<u8>>> {
    let response = reqwest::Client::new().get(url).send().await?;
    if !response.status().is_success() {
        return Ok(None);
    }
    Ok(Some(response.bytes().await?.to_vec()))
}

fn data_url_base64_payload(value: &str) -> Option<&str> {
    let (_, payload) = value.split_once(',')?;
    value.contains(";base64,").then_some(payload)
}

fn block_on_async<F>(future: F) -> F::Output
where
    F: Future,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(future))
    } else {
        tokio::runtime::Runtime::new()
            .expect("failed to create tokio runtime for outbound image normalization")
            .block_on(future)
    }
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
