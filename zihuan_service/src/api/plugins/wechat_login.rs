//! QR-code login for the WeChatPadPro plugin.
//!
//! The 861 bridge binds an auth key (授权码) to the WeChat account that scans the QR code, and
//! account-level API calls are authorized by that key afterwards. The stored connection
//! credential may be the bridge admin key, so logging in means: reuse the credential when it
//! can already start a login, otherwise mint a fresh auth key via the admin route, show the
//! bridge's QR and poll until the scan is confirmed. The confirmed auth key then replaces
//! `pad_token` in the connection record and the running adapters are refreshed.

use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use zihuan_core::ims_bot_adapter::runtime::wechat_pad::check_bridge_response;
use zihuan_core::ims_bot_adapter::{parse_ims_bot_adapter_connection, sync_enabled_bot_adapters};
use zihuan_core::storage::{self, ConnectionConfig, ConnectionKind};

use super::{find_plugin, plugin_connections, render_error, PluginRecord};

const LOGIN_QR_PATH: &str = "/login/GetLoginQrCodePad";
const LOGIN_STATUS_PATH: &str = "/login/CheckLoginStatus";
const LOGIN_ACCOUNT_PATH: &str = "/login/GetLoginStatus";
const GEN_AUTH_KEY_PATH: &str = "/admin/GenAuthKey1";

#[derive(Deserialize)]
pub struct WechatLoginQrRequest {
    pub plugin_id: String,
}

#[derive(Deserialize)]
pub struct WechatLoginStatusRequest {
    pub plugin_id: String,
    #[serde(default)]
    pub auth_key: String,
}

/// The bridge base URL and credential stored on the plugin's WeChat connection.
fn wechat_endpoint(plugin: &PluginRecord) -> Result<(String, String), String> {
    let connection =
        plugin_connections(plugin)?.into_iter().next().ok_or("插件还没有关联连接")?;
    let ConnectionKind::BotAdapter(raw) = &connection.kind else {
        return Err("关联连接不是 bot_adapter 类型".to_string());
    };
    let bot = parse_ims_bot_adapter_connection(raw).map_err(|err| err.to_string())?;
    let wechat = bot.wechat.ok_or("关联连接没有微信配置")?;
    let base_url = wechat.pad_url.trim().trim_end_matches('/').to_string();
    if base_url.is_empty() {
        return Err("关联连接的 pad_url 未配置".to_string());
    }
    Ok((base_url, wechat.pad_token.trim().to_string()))
}

/// GET with the credential as `key` query parameter and Bearer header, like the adapter sends.
async fn bridge_get(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    token: &str,
) -> std::result::Result<String, String> {
    let endpoint = format!("{base_url}{path}");
    let request = client.get(&endpoint);
    let request = if token.is_empty() { request } else { request.bearer_auth(token).query(&[("key", token)]) };
    let response = request.send().await.map_err(|error| format!("{path}: {error}"))?;
    check_bridge_response(response).await.map_err(|error| format!("{path}: {error}"))
}

/// POST JSON with the same credential handling as [`bridge_get`].
async fn bridge_post_json(
    client: &reqwest::Client,
    base_url: &str,
    path: &str,
    token: &str,
    body: &Value,
) -> std::result::Result<String, String> {
    let endpoint = format!("{base_url}{path}");
    let request = client.post(&endpoint).json(body);
    let request = if token.is_empty() { request } else { request.bearer_auth(token).query(&[("key", token)]) };
    let response = request.send().await.map_err(|error| format!("{path}: {error}"))?;
    check_bridge_response(response).await.map_err(|error| format!("{path}: {error}"))
}

fn pick_text(value: &Value, keys: &[&str]) -> String {
    for key in keys {
        let text = value.get(*key).and_then(Value::as_str).unwrap_or_default();
        if !text.trim().is_empty() {
            return text.trim().to_string();
        }
    }
    String::new()
}

/// The (wxid, nickname) carried by a login reply; empty strings when the bridge sent none.
fn login_account(data: &Value) -> (String, String) {
    let wxid = pick_text(data, &["wxid", "Wxid", "WXID", "UserName", "userName"]);
    let nickname = pick_text(data, &["NickName", "nickName", "nickname"]);
    (wxid, nickname)
}

fn extract_auth_key(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let keys = value.get("Data")?.get("authKeys")?.as_array()?;
    keys.iter().find_map(|key| key.as_str()).map(str::to_string)
}

async fn qr_login(
    client: &reqwest::Client,
    base_url: &str,
    auth_key: &str,
) -> std::result::Result<String, String> {
    bridge_post_json(client, base_url, LOGIN_QR_PATH, auth_key, &json!({})).await
}

/// Mints a fresh auth key through the admin route, which requires the admin credential.
async fn mint_auth_key(
    client: &reqwest::Client,
    base_url: &str,
    admin_token: &str,
) -> std::result::Result<String, String> {
    let body = bridge_post_json(
        client,
        base_url,
        GEN_AUTH_KEY_PATH,
        admin_token,
        &json!({ "Count": 1, "Days": 365, "Remark": "zihuan-wechat-login" }),
    )
    .await?;
    extract_auth_key(&body).ok_or_else(|| format!("桥未返回授权码: {body}"))
}

/// Replaces the connection credential with the confirmed auth key (plus the account wxid when
/// known) and refreshes the running adapters, returning the updated connection.
async fn persist_login(
    plugin: &PluginRecord,
    auth_key: &str,
    wxid: &str,
) -> std::result::Result<ConnectionConfig, String> {
    let mut connection =
        plugin_connections(plugin)?.into_iter().next().ok_or("插件还没有关联连接")?;
    let ConnectionKind::BotAdapter(raw) = &connection.kind else {
        return Err("关联连接不是 bot_adapter 类型".to_string());
    };
    let mut bot = parse_ims_bot_adapter_connection(raw).map_err(|err| err.to_string())?;
    let wechat = bot.wechat.as_mut().ok_or("关联连接没有微信配置")?;
    wechat.pad_token = auth_key.to_string();
    if !wxid.is_empty() {
        wechat.self_wxid = wxid.to_string();
    }
    connection.kind = ConnectionKind::BotAdapter(serde_json::to_value(&bot).unwrap_or(Value::Null));
    connection.updated_at = chrono::Utc::now().to_rfc3339();
    storage::upsert_connection(connection.clone()).map_err(|err| err.to_string())?;
    let refreshed = storage::load_connections().map_err(|err| err.to_string())?;
    sync_enabled_bot_adapters(&refreshed).await;
    Ok(connection)
}

#[handler]
pub async fn wechat_login_qr(req: &mut Request, res: &mut Response) {
    let body: WechatLoginQrRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    let plugin = match find_plugin(&body.plugin_id) {
        Ok(plugin) => plugin,
        Err(error) => return render_error(res, StatusCode::NOT_FOUND, error),
    };
    let (base_url, token) = match wechat_endpoint(&plugin) {
        Ok(endpoint) => endpoint,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error),
    };
    let client = reqwest::Client::new();

    // A credential that already reports a confirmed login needs no new QR code.
    if let Ok(body) = bridge_get(&client, &base_url, LOGIN_STATUS_PATH, &token).await {
        if let Ok(value) = serde_json::from_str::<Value>(&body) {
            let data = value.get("Data").cloned().unwrap_or(Value::Null);
            let (wxid, nickname) = login_account(&data);
            let state = data.get("state").and_then(Value::as_i64).unwrap_or(0);
            if state >= 2 || !wxid.is_empty() {
                return res.render(Json(json!({
                    "status": "logged_in",
                    "auth_key": token,
                    "wxid": wxid,
                    "nickname": nickname,
                })));
            }
        }
    }

    // Start a login with the stored credential; when it cannot start one (it is the admin key
    // or an expired auth key), mint a fresh auth key via the admin route and try again.
    let mut auth_key = token.clone();
    let qr_body = match qr_login(&client, &base_url, &auth_key).await {
        Ok(body) => body,
        Err(login_error) => {
            auth_key = match mint_auth_key(&client, &base_url, &auth_key).await {
                Ok(key) => key,
                Err(mint_error) => {
                    return render_error(
                        res,
                        StatusCode::BAD_GATEWAY,
                        format!("获取登录二维码失败: {login_error}；生成授权码失败: {mint_error}"),
                    );
                }
            };
            match qr_login(&client, &base_url, &auth_key).await {
                Ok(body) => body,
                Err(error) => {
                    return render_error(res, StatusCode::BAD_GATEWAY, format!("获取登录二维码失败: {error}"))
                }
            }
        }
    };

    let parsed: Value = match serde_json::from_str(&qr_body) {
        Ok(value) => value,
        Err(_) => return render_error(res, StatusCode::BAD_GATEWAY, format!("桥返回了无效的二维码响应: {qr_body}")),
    };
    let data = parsed.get("Data").cloned().unwrap_or(Value::Null);
    let qr_base64 = data.get("qrCodeBase64").and_then(Value::as_str).unwrap_or_default();
    let qr_link = pick_text(&data, &["QrLink", "QrCodeUrl"]);
    let expires_in_secs = data.get("expiredTime").and_then(Value::as_i64).unwrap_or(290).max(30);
    if qr_base64.is_empty() && qr_link.is_empty() {
        return render_error(res, StatusCode::BAD_GATEWAY, format!("桥未返回二维码: {qr_body}"));
    }
    res.render(Json(json!({
        "status": "qr",
        "auth_key": auth_key,
        "qr_base64": qr_base64,
        "qr_link": qr_link,
        "expires_in_secs": expires_in_secs,
    })));
}

#[handler]
pub async fn wechat_login_status(req: &mut Request, res: &mut Response) {
    let body: WechatLoginStatusRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    let plugin = match find_plugin(&body.plugin_id) {
        Ok(plugin) => plugin,
        Err(error) => return render_error(res, StatusCode::NOT_FOUND, error),
    };
    let (base_url, _) = match wechat_endpoint(&plugin) {
        Ok(endpoint) => endpoint,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error),
    };
    let auth_key = body.auth_key.trim().to_string();
    if auth_key.is_empty() {
        return render_error(res, StatusCode::BAD_REQUEST, "auth_key is required".to_string());
    }

    let client = reqwest::Client::new();
    let body_text = match bridge_get(&client, &base_url, LOGIN_STATUS_PATH, &auth_key).await {
        Ok(body) => body,
        Err(error) => return res.render(Json(json!({ "status": "error", "message": error }))),
    };
    let value: Value = match serde_json::from_str(&body_text) {
        Ok(value) => value,
        Err(_) => return res.render(Json(json!({ "status": "error", "message": body_text }))),
    };
    let data = value.get("Data").cloned().unwrap_or(Value::Null);
    let state = data.get("state").and_then(Value::as_i64).unwrap_or(0);
    let (mut wxid, mut nickname) = login_account(&data);

    if state >= 2 || !wxid.is_empty() {
        // Confirmed: fill in account info from the login status route when the scan reply lacks it.
        if wxid.is_empty() {
            if let Ok(account_body) = bridge_get(&client, &base_url, LOGIN_ACCOUNT_PATH, &auth_key).await {
                if let Ok(account_value) = serde_json::from_str::<Value>(&account_body) {
                    let account = account_value.get("Data").cloned().unwrap_or(Value::Null);
                    let found = login_account(&account);
                    if wxid.is_empty() {
                        wxid = found.0;
                    }
                    if nickname.is_empty() {
                        nickname = found.1;
                    }
                }
            }
        }
        let connection = match persist_login(&plugin, &auth_key, &wxid).await {
            Ok(connection) => connection,
            Err(error) => {
                return render_error(
                    res,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("登录成功但写回连接失败: {error}"),
                );
            }
        };
        // Best effort: the live connection is saved already; the metadata copy is display-only.
        let mut plugin = plugin;
        plugin.extra_install_metadata["connection_config"] =
            serde_json::to_value(&connection).unwrap_or(Value::Null);
        plugin.updated_at = chrono::Utc::now().to_rfc3339();
        let _ = super::save_plugin_record(&plugin);
        return res.render(Json(json!({ "status": "logged_in", "wxid": wxid, "nickname": nickname })));
    }

    if state < 0 {
        return res.render(Json(json!({
            "status": "expired",
            "message": value.get("Text").and_then(Value::as_str).unwrap_or("二维码已过期"),
        })));
    }
    res.render(Json(json!({
        "status": if state == 1 { "scanned" } else { "waiting" },
        "message": value.get("Text").and_then(Value::as_str).unwrap_or(""),
    })));
}
