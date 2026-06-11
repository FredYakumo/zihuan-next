use serde::{Deserialize, Serialize};
use std::time::Duration;
use zihuan_core::connection_manager::ConnectionManager;
use zihuan_core::error::{Error, Result};

use crate::active_adapter_manager::ActiveAdapterManager;
use crate::system_config::BotAdapterConnection;
use crate::ws_action::{json_i64, next_echo, response_success, ws_send_action_async};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotLoginInfo {
    pub user_id: String,
    pub nickname: String,
}

pub fn qq_avatar_url(user_id: &str) -> Option<String> {
    let user_id = user_id.trim();
    if user_id.is_empty() {
        None
    } else {
        Some(format!("https://q1.qlogo.cn/g?b=qq&nk={user_id}&s=640"))
    }
}

pub async fn fetch_login_info(connection: &BotAdapterConnection) -> Result<BotLoginInfo> {
    fetch_login_info_via_websocket(connection).await
}

/// Fetches login info through the active bot adapter's existing WebSocket connection.
/// This is preferred over `fetch_login_info` when the adapter is already running, because
/// it reuses the existing WebSocket connection rather than creating a new one.
pub async fn fetch_login_info_via_adapter_connection(connection_id: &str) -> Result<BotLoginInfo> {
    let adapter = ActiveAdapterManager::shared().get_or_create(connection_id).await?;

    let max_retries = 10u32;
    let retry_delay = Duration::from_millis(500);
    let mut last_err = None;

    for attempt in 1..=max_retries {
        match ws_send_action_async(&adapter, "get_login_info", serde_json::json!({})).await {
            Ok(response) => return parse_login_info(&response),
            Err(err) => {
                let err_msg = err.to_string();
                if err_msg.contains("not connected yet") && attempt < max_retries {
                    tokio::time::sleep(retry_delay).await;
                    last_err = Some(err);
                    continue;
                }
                return Err(err);
            }
        }
    }

    Err(last_err
        .unwrap_or_else(|| Error::ValidationError("failed to fetch login info via adapter after retries".to_string())))
}

async fn fetch_login_info_via_websocket(connection: &BotAdapterConnection) -> Result<BotLoginInfo> {
    use futures_util::{SinkExt, StreamExt};

    let request = websocket_request(connection)?;
    let (mut ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
    let echo = next_echo();
    let payload = serde_json::json!({
        "action": "get_login_info",
        "params": {},
        "echo": echo,
    });

    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text(payload.to_string().into()))
        .await?;

    loop {
        let Some(message) = ws_stream.next().await else {
            return Err(Error::ValidationError(
                "WebSocket closed before get_login_info response arrived".to_string(),
            ));
        };

        let message = message?;
        let text = match message {
            tokio_tungstenite::tungstenite::Message::Text(text) => text,
            tokio_tungstenite::tungstenite::Message::Binary(data) => String::from_utf8(data.to_vec())
                .map_err(|err| {
                    Error::ValidationError(format!("get_login_info WebSocket response is not valid UTF-8: {err}"))
                })?
                .into(),
            tokio_tungstenite::tungstenite::Message::Close(_) => {
                return Err(Error::ValidationError(
                    "WebSocket closed before get_login_info response arrived".to_string(),
                ));
            }
            _ => continue,
        };

        let response: serde_json::Value = serde_json::from_str(&text).map_err(|err| {
            Error::ValidationError(format!(
                "failed to parse get_login_info WebSocket response as JSON: {}",
                summarize_body_for_error(&text, &err.to_string())
            ))
        })?;

        if response.get("echo").and_then(|value| value.as_str()) != Some(echo.as_str()) {
            continue;
        }

        if !response_success(&response) {
            let message = response
                .get("message")
                .and_then(|value| value.as_str())
                .or_else(|| response.get("wording").and_then(|value| value.as_str()))
                .unwrap_or("unknown error");
            return Err(Error::ValidationError(format!(
                "NapCat get_login_info WebSocket action returned failure: {}",
                message
            )));
        }

        return parse_login_info(&response);
    }
}

fn parse_login_info(payload: &serde_json::Value) -> Result<BotLoginInfo> {
    let data = payload
        .get("data")
        .and_then(|value| value.as_object())
        .ok_or_else(|| Error::ValidationError("NapCat /get_login_info response missing data object".to_string()))?;

    let user_id = data
        .get("user_id")
        .and_then(|value| {
            json_i64(Some(value))
                .map(|id| id.to_string())
                .or_else(|| value.as_str().map(str::to_string))
        })
        .ok_or_else(|| {
            Error::ValidationError("NapCat /get_login_info response missing valid data.user_id".to_string())
        })?;

    let nickname = data
        .get("nickname")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();

    Ok(BotLoginInfo { user_id, nickname })
}

fn websocket_request(connection: &BotAdapterConnection) -> Result<http::Request<()>> {
    let mut builder = http::Request::builder()
        .uri(&connection.bot_server_url)
        .header("Host", host_header(&connection.bot_server_url))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tokio_tungstenite::tungstenite::handshake::client::generate_key(),
        );

    if let Some(token) = connection
        .bot_server_token
        .as_ref()
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
    {
        builder = builder.header("Authorization", format!("Bearer {}", token));
    }

    Ok(builder.body(())?)
}

fn host_header(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .filter(|host| !host.is_empty())
        .unwrap_or("localhost")
}

fn summarize_body_for_error(body: &str, error: &str) -> String {
    let body = body.trim();
    if body.is_empty() {
        format!("{error}; empty body")
    } else {
        let preview: String = body.chars().take(160).collect();
        if body.chars().count() > 160 {
            format!("{error}; body starts with: {preview}...")
        } else {
            format!("{error}; body: {preview}")
        }
    }
}
