use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_tungstenite::connect_async;
use zihuan_core::error::{Error, Result};

use crate::system_config::BotAdapterConnection;
use crate::ws_action::{json_i64, next_echo, response_success};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotLoginInfo {
    pub user_id: String,
    pub nickname: String,
}

pub fn websocket_url_to_http_base(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("wss://") {
        let host_port = rest.split('/').next().unwrap_or(rest);
        format!("https://{}", host_port)
    } else if let Some(rest) = url.strip_prefix("ws://") {
        let host_port = rest.split('/').next().unwrap_or(rest);
        format!("http://{}", host_port)
    } else {
        url.to_string()
    }
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
    match fetch_login_info_via_http(connection).await {
        Ok(info) => Ok(info),
        Err(http_err) => match fetch_login_info_via_websocket(connection).await {
            Ok(info) => Ok(info),
            Err(ws_err) => Err(Error::ValidationError(format!(
                "failed to fetch bot login info via HTTP and WebSocket; http: {http_err}; websocket: {ws_err}"
            ))),
        },
    }
}

async fn fetch_login_info_via_http(connection: &BotAdapterConnection) -> Result<BotLoginInfo> {
    let base_url = websocket_url_to_http_base(&connection.bot_server_url);
    let endpoint = format!("{}/get_login_info", base_url.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let mut request = client.post(endpoint).json(&json!({}));
    if let Some(token) = connection
        .bot_server_token
        .as_ref()
        .map(|token| token.trim())
        .filter(|token| !token.is_empty())
    {
        request = request.bearer_auth(token);
    }

    let response = request.send().await?;
    let status = response.status();
    let body = response.text().await?;
    let payload: serde_json::Value = serde_json::from_str(&body).map_err(|err| {
        Error::ValidationError(format!(
            "NapCat /get_login_info returned non-JSON HTTP {} response: {}",
            status, summarize_body_for_error(&body, &err.to_string())
        ))
    })?;

    if !status.is_success() {
        let message = payload
            .get("message")
            .and_then(|value| value.as_str())
            .or_else(|| payload.get("wording").and_then(|value| value.as_str()))
            .unwrap_or("unknown error");
        return Err(Error::ValidationError(format!(
            "NapCat /get_login_info failed with HTTP {}: {}",
            status, message
        )));
    }

    if !response_success(&payload) {
        let message = payload
            .get("message")
            .and_then(|value| value.as_str())
            .or_else(|| payload.get("wording").and_then(|value| value.as_str()))
            .unwrap_or("unknown error");
        return Err(Error::ValidationError(format!(
            "NapCat /get_login_info returned failure: {}",
            message
        )));
    }

    parse_login_info(&payload)
}

async fn fetch_login_info_via_websocket(connection: &BotAdapterConnection) -> Result<BotLoginInfo> {
    use futures_util::{SinkExt, StreamExt};

    let request = websocket_request(connection)?;
    let (mut ws_stream, _) = connect_async(request).await?;
    let echo = next_echo();
    let payload = json!({
        "action": "get_login_info",
        "params": {},
        "echo": echo,
    });

    ws_stream
        .send(tokio_tungstenite::tungstenite::Message::Text(
            payload.to_string().into(),
        ))
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
            tokio_tungstenite::tungstenite::Message::Binary(data) => {
                String::from_utf8(data.to_vec()).map_err(|err| {
                    Error::ValidationError(format!(
                        "get_login_info WebSocket response is not valid UTF-8: {err}"
                    ))
                })?
                .into()
            }
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

        if response
            .get("echo")
            .and_then(|value| value.as_str())
            != Some(echo.as_str())
        {
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
        .ok_or_else(|| {
            Error::ValidationError("NapCat /get_login_info response missing data object".to_string())
        })?;

    let user_id = data
        .get("user_id")
        .and_then(|value| {
            json_i64(Some(value))
                .map(|id| id.to_string())
                .or_else(|| value.as_str().map(str::to_string))
        })
        .ok_or_else(|| {
            Error::ValidationError(
                "NapCat /get_login_info response missing valid data.user_id".to_string(),
            )
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
