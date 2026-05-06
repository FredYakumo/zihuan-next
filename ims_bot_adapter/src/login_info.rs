use serde::{Deserialize, Serialize};
use serde_json::json;
use zihuan_core::error::{Error, Result};

use crate::system_config::BotAdapterConnection;
use crate::ws_action::{json_i64, response_success};

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
    let payload: serde_json::Value = response.json().await?;

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
