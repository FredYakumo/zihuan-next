//! Personal-WeChat backend built on a self-hosted WeChatPadPro (iPad protocol) bridge.
//!
//! WeChatPadPro pushes inbound messages to an HTTP webhook and exposes REST calls for sending.
//! The rest of the adapter speaks OneBot 11, so this backend translates in both directions and
//! keeps the OneBot JSON contract that the parser, handlers and reply pipeline already depend on.
//!
//! The webhook route itself lives in the web layer (`zihuan_service`); [`dispatch_webhook`] fans
//! a raw payload into the running backend instance registered for the same token.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::adapter::{BotAdapter, BotAdapterBackend, SharedBotAdapter};
use super::ws_action::json_id;
use crate::error::Result;

const LOG_PREFIX: &str = "[wechat_pad]";

/// Candidate send endpoints, tried in order: bridge builds differ in their route casing.
const SEND_TEXT_PATHS: &[&str] = &[
    "/api/v1/message/SendText",
    "/api/v1/message/SendTextMsg",
    "/api/v1/message/sendText",
    "/api/message/SendText",
];

static MESSAGE_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Port this server listens on, registered at startup so the bridge callback URL can be derived
/// without asking the operator to type it into the connection form.
static SERVICE_PORT: OnceLock<u16> = OnceLock::new();

pub fn set_service_port(port: u16) {
    let _ = SERVICE_PORT.set(port);
}

/// Connection settings for one WeChat account backed by a WeChatPadPro bridge.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WeChatPadConfig {
    /// Bridge base URL, e.g. `http://127.0.0.1:8849`.
    #[serde(default)]
    pub pad_url: String,
    /// Bridge `adminKey`, sent as `Authorization: Bearer`.
    #[serde(default)]
    pub pad_token: String,
    /// The bot account's own wxid, used for login info and @-mention detection.
    #[serde(default)]
    pub self_wxid: String,
    /// Secret path segment: the bridge POSTs to `/api/ims/wechat_pad/{webhook_token}`.
    #[serde(default)]
    pub webhook_token: String,
    /// Public URL handed to the bridge on startup; left unset, the hook relay or the derived
    /// callback URL is used instead.
    #[serde(default)]
    pub webhook_url: Option<String>,
    /// Base URL of the in-container hook relay. When set, inbound messages are pulled from the
    /// relay instead of being received on this server's webhook route.
    #[serde(default)]
    pub relay_url: Option<String>,
    /// Aliases such as `小助手` recognised in group text.
    #[serde(default)]
    pub bot_names: Vec<String>,
    /// Display name reported as the bot nickname.
    #[serde(default)]
    pub nickname: Option<String>,
}

impl WeChatPadConfig {
    fn base_url(&self) -> &str {
        self.pad_url.trim().trim_end_matches('/')
    }
}

type WebhookSenders = Mutex<HashMap<String, mpsc::UnboundedSender<Value>>>;

fn webhook_senders() -> &'static WebhookSenders {
    static SENDERS: OnceLock<WebhookSenders> = OnceLock::new();
    SENDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_senders() -> std::sync::MutexGuard<'static, HashMap<String, mpsc::UnboundedSender<Value>>> {
    webhook_senders().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Forward a bridge webhook payload to the adapter instance registered for `token`.
///
/// Returns `false` when no adapter is currently running for that token, so the HTTP layer can
/// answer 404 instead of silently dropping the delivery.
pub fn dispatch_webhook(token: &str, payload: Value) -> bool {
    let senders = lock_senders();
    match senders.get(token) {
        Some(sender) => sender.send(payload).is_ok(),
        None => false,
    }
}

/// OneBot-11 backend over a WeChatPadPro bridge.
pub struct WeChatPadBotAdapter {
    config: WeChatPadConfig,
}

impl WeChatPadBotAdapter {
    pub fn new(config: WeChatPadConfig) -> Self {
        Self { config }
    }

    async fn register_webhook(&self, client: &reqwest::Client) {
        let explicit =
            self.config.webhook_url.as_deref().map(str::trim).filter(|url| !url.is_empty());
        let webhook_url = match explicit {
            Some(url) => url.to_string(),
            None => match derived_webhook_url(&self.config) {
                Some(url) => url,
                None => return,
            },
        };
        if self.config.base_url().is_empty() {
            warn!("{LOG_PREFIX} pad_url is empty; skipping webhook registration");
            return;
        }

        let endpoint = format!("{}/v1/webhook/Config", self.config.base_url());
        let body = json!({
            "webhookUrl": webhook_url,
            "types": [1, 3, 49],
            "includeSelf": false,
        });
        match with_auth(client.post(&endpoint), &self.config).json(&body).send().await {
            Ok(response) if response.status().is_success() => {
                info!("{LOG_PREFIX} registered webhook {webhook_url} with the bridge")
            }
            Ok(response) => {
                warn!("{LOG_PREFIX} webhook registration returned HTTP {}", response.status())
            }
            Err(error) => warn!("{LOG_PREFIX} webhook registration failed: {error}"),
        }
    }
}

/// Callback URL for a bridge running on this machine.
///
/// The bridge lives in a container, so it reaches us through the host gateway rather than
/// `127.0.0.1`. A bridge on another machine needs an address only the operator knows, so it
/// still has to set `webhook_url` explicitly.
fn derived_webhook_url(config: &WeChatPadConfig) -> Option<String> {
    let token = config.webhook_token.trim();
    if token.is_empty() {
        return None;
    }
    if !pad_is_local(config.base_url()) {
        warn!(
            "{LOG_PREFIX} pad_url is not local and webhook_url is empty; skipping automatic \
             webhook registration"
        );
        return None;
    }
    let Some(port) = SERVICE_PORT.get() else {
        warn!("{LOG_PREFIX} service port is not registered yet; skipping webhook registration");
        return None;
    };
    Some(format!("http://host.docker.internal:{port}/api/ims/wechat_pad/{token}"))
}

fn pad_is_local(base_url: &str) -> bool {
    let authority = base_url.split("//").nth(1).unwrap_or(base_url);
    let authority = authority.split('/').next().unwrap_or(authority);
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    matches!(host, "127.0.0.1" | "localhost" | "0.0.0.0" | "::1")
}

#[async_trait]
impl BotAdapterBackend for WeChatPadBotAdapter {
    async fn run(&self, adapter: SharedBotAdapter) -> Result<()> {
        let relay_url = self
            .config
            .relay_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .map(str::to_string);
        let token = self.config.webhook_token.trim().to_string();
        let (payload_tx, mut payload_rx) = mpsc::unbounded_channel::<Value>();

        let cleanup_tx = payload_tx.clone();
        // In relay mode nothing is posted to this server, so hold the inbound sender to keep the
        // loop below alive; the relay loop feeds events through the adapter directly.
        let _inbound_keepalive: Option<mpsc::UnboundedSender<Value>> = if relay_url.is_some() {
            Some(payload_tx)
        } else if token.is_empty() {
            warn!("{LOG_PREFIX} webhook_token is empty; inbound messages cannot be routed");
            Some(payload_tx)
        } else {
            lock_senders().insert(token.clone(), payload_tx);
            None
        };

        let (action_tx, mut action_rx) = mpsc::unbounded_channel::<String>();
        adapter.lock().await.action_tx = Some(action_tx);

        let client = reqwest::Client::new();

        let pump_adapter = adapter.clone();
        let pump_client = client.clone();
        let pump_config = self.config.clone();
        tokio::spawn(async move {
            while let Some(raw) = action_rx.recv().await {
                handle_action(&pump_adapter, &pump_client, &pump_config, &raw).await;
            }
        });

        if let Some(relay_url) = relay_url {
            // The relay owns the bridge webhook; it registers itself, so do not overwrite it.
            info!("{LOG_PREFIX} pulling inbound messages from hook relay {relay_url}");
            let relay_adapter = adapter.clone();
            let relay_config = self.config.clone();
            tokio::spawn(async move {
                run_relay_loop(relay_adapter, relay_config, relay_url).await;
            });
        } else {
            self.register_webhook(&client).await;
            info!(
                "{LOG_PREFIX} listening for bridge webhooks (token set: {}, pad_url: {})",
                !token.is_empty(),
                self.config.base_url()
            );
        }

        while let Some(payload) = payload_rx.recv().await {
            for event in normalize_webhook(&payload, &self.config) {
                BotAdapter::process_event(adapter.clone(), event.to_string()).await;
            }
        }

        if !token.is_empty() {
            let mut senders = lock_senders();
            let still_ours = senders
                .get(&token)
                .map(|sender| sender.same_channel(&cleanup_tx))
                .unwrap_or(false);
            if still_ours {
                senders.remove(&token);
            }
        }
        Ok(())
    }
}

/// Long-poll the in-container hook relay and feed what it buffered into the adapter.
async fn run_relay_loop(adapter: SharedBotAdapter, config: WeChatPadConfig, relay_url: String) {
    let client = reqwest::Client::new();
    let base = relay_url.trim_end_matches('/').to_string();
    // -1 asks the relay to start at its current head: messages queued while this process was
    // down are dropped rather than answered out of context on startup.
    let mut cursor: i64 = -1;
    loop {
        let url = format!("{base}/events?after={cursor}&wait=25000");
        let response = match client.get(&url).timeout(Duration::from_secs(35)).send().await {
            Ok(response) => response,
            Err(error) => {
                debug!("{LOG_PREFIX} hook relay unreachable: {error}");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if !response.status().is_success() {
            warn!("{LOG_PREFIX} hook relay returned HTTP {}", response.status());
            tokio::time::sleep(Duration::from_secs(2)).await;
            continue;
        }
        let body: Value = match response.json().await {
            Ok(body) => body,
            Err(error) => {
                warn!("{LOG_PREFIX} hook relay returned invalid JSON: {error}");
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };
        if let Some(next) = body.get("cursor").and_then(Value::as_i64) {
            cursor = next;
        }
        let entries = body.get("events").and_then(Value::as_array).cloned().unwrap_or_default();
        for entry in entries {
            let payload = entry.get("payload").cloned().unwrap_or(Value::Null);
            for event in normalize_webhook(&payload, &config) {
                BotAdapter::process_event(adapter.clone(), event.to_string()).await;
            }
        }
    }
}

/// Translate one bridge webhook payload into zero or more OneBot message events.
pub fn normalize_webhook(payload: &Value, config: &WeChatPadConfig) -> Vec<Value> {
    if let Some(event_type) = payload.get("event_type").and_then(Value::as_str) {
        if !event_type.eq_ignore_ascii_case("message") {
            return Vec::new();
        }
    }

    let data = payload
        .get("data")
        .or_else(|| payload.get("Data"))
        .or_else(|| payload.get("message"))
        .unwrap_or(payload);

    let items: Vec<&Value> = match data {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };

    items.into_iter().filter_map(|item| normalize_message(item, config)).collect()
}

fn normalize_message(item: &Value, config: &WeChatPadConfig) -> Option<Value> {
    let from =
        pick_text(item, &["FromUserName", "fromUserName", "from_user_name", "FromUser", "from"]);
    let to = pick_text(item, &["ToUserName", "toUserName", "to_user_name", "ToUser", "to"]);
    let room = pick_text(
        item,
        &["ChatRoomName", "StrChatRoomUserName", "chatRoomName", "RoomName", "room"],
    );

    if is_self_sent(item, &from, &config.self_wxid) {
        return None;
    }

    let target = if from.ends_with("@chatroom") {
        from.as_str()
    } else if to.ends_with("@chatroom") {
        to.as_str()
    } else {
        room.as_str()
    };
    let is_group = !target.is_empty() || from.ends_with("@chatroom") || to.ends_with("@chatroom");
    let group_id = target.to_string();

    let reported_sender = pick_text(
        item,
        &[
            "Sender",
            "ActualSender",
            "FromUser",
            "SenderUserName",
            "StrSender",
            "senderId",
            "sender",
        ],
    );
    let sender_id = if is_group && !reported_sender.is_empty() {
        reported_sender
    } else {
        from.clone()
    };
    if sender_id.is_empty() {
        return None;
    }
    if !config.self_wxid.is_empty() && sender_id == config.self_wxid {
        return None;
    }

    let text = pick_text(item, &["Content", "content", "Text", "text", "MsgContent"]);
    let msg_type = json_id(item.get("MsgType").or_else(|| item.get("msgType")))
        .and_then(|value| value.parse::<i64>().ok());
    if msg_type == Some(3) {
        // Image: the payload is XML, not text; image understanding is not wired up yet.
        debug!("{LOG_PREFIX} skipping image message; only text is bridged for now");
        return None;
    }
    let at_ids = at_list(item);

    let mut segments: Vec<Value> = at_ids
        .iter()
        .map(|wxid| json!({ "type": "at", "data": { "qq": wxid } }))
        .collect();
    if !text.trim().is_empty() {
        segments.push(json!({ "type": "text", "data": { "text": text } }));
    }
    if segments.is_empty() {
        debug!("{LOG_PREFIX} dropping message with no text or mention: {}", item);
        return None;
    }

    let nickname = pick_text(
        item,
        &[
            "SenderName",
            "senderName",
            "NickName",
            "nickName",
            "DisplayName",
            "FromNickName",
        ],
    );

    Some(json!({
        "message_id": message_id(item),
        "message_type": if is_group { "group" } else { "private" },
        "sender": {
            "user_id": sender_id,
            "nickname": nickname,
            "card": "",
            "role": "member",
            "sex": "unknown",
        },
        "message": segments,
        "group_id": if is_group { Value::String(group_id) } else { Value::Null },
        "group_name": Value::Null,
    }))
}

fn is_self_sent(item: &Value, from: &str, self_wxid: &str) -> bool {
    let flag = item
        .get("IsSender")
        .or_else(|| item.get("isSender"))
        .or_else(|| item.get("IsSelf"))
        .or_else(|| item.get("isSelf"));
    if let Some(flag) = flag {
        if flag.as_bool() == Some(true) || flag.as_i64() == Some(1) {
            return true;
        }
    }
    !self_wxid.is_empty() && from == self_wxid
}

fn message_id(item: &Value) -> String {
    if let Some(id) = json_id(
        item.get("NewMsgId")
            .or_else(|| item.get("newMsgId"))
            .or_else(|| item.get("MsgId"))
            .or_else(|| item.get("msgId"))
            .or_else(|| item.get("MsgSvrId"))
            .or_else(|| item.get("StrMsgId"))
            .or_else(|| item.get("id")),
    ) {
        if !id.is_empty() {
            return id;
        }
    }

    let created_at = json_id(
        item.get("CreateTime")
            .or_else(|| item.get("createTime"))
            .or_else(|| item.get("Timestamp")),
    )
    .unwrap_or_else(|| chrono::Utc::now().timestamp().to_string());
    let counter = MESSAGE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("wp_{created_at}_{counter}")
}

/// Text fields arrive as strings, numbers, or `{ "string": "..." }` wrappers depending on build.
fn pick_text(value: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(found) = value.get(*key) {
            let text = unbox_text(found);
            if !text.is_empty() {
                return text;
            }
        }
    }
    String::new()
}

fn unbox_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Array(items) => items.iter().map(unbox_text).collect::<Vec<_>>().join(""),
        Value::Object(map) => ["string", "String", "value", "Value", "text", "Text", "str"]
            .iter()
            .find_map(|key| map.get(*key).map(unbox_text))
            .unwrap_or_default(),
    }
}

/// Pull mentioned wxids out of `<msgsource><atuserlist><![CDATA[a,b]]></atuserlist>…`.
fn at_list(item: &Value) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();

    let source = pick_text(item, &["MsgSource", "msgSource", "MsgSourceXml"]);
    if let Some(start) = source.find("<atuserlist") {
        let rest = &source[start..];
        if let Some(open_end) = rest.find('>') {
            let body = &rest[open_end + 1..];
            let body = body.split("</atuserlist>").next().unwrap_or(body);
            let body = body.replace("<![CDATA[", "").replace("]]>", "");
            for part in body.split(&[',', ';', ' '][..]) {
                let id = part.trim();
                if !id.is_empty() && !ids.iter().any(|known| known == id) {
                    ids.push(id.to_string());
                }
            }
        }
    }

    for key in ["AtWxids", "atWxids", "AtList", "atList", "AtUsers"] {
        match item.get(key) {
            Some(Value::Array(items)) => {
                for entry in items {
                    let id = unbox_text(entry).trim().to_string();
                    if !id.is_empty() && !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
            Some(Value::String(text)) => {
                for part in text.split(',') {
                    let id = part.trim();
                    if !id.is_empty() && !ids.iter().any(|known| known == id) {
                        ids.push(id.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    ids
}

async fn handle_action(
    adapter: &SharedBotAdapter,
    client: &reqwest::Client,
    config: &WeChatPadConfig,
    raw: &str,
) {
    let request: Value = match serde_json::from_str(raw) {
        Ok(request) => request,
        Err(error) => {
            warn!("{LOG_PREFIX} failed to parse pending action: {error}");
            return;
        }
    };
    let action = request.get("action").and_then(Value::as_str).unwrap_or_default().to_string();
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    let echo = request.get("echo").and_then(Value::as_str).unwrap_or_default().to_string();

    let response = match action.as_str() {
        "send_private_msg" | "send_group_msg" | "send_msg" => {
            let target = params
                .get("group_id")
                .or_else(|| params.get("user_id"))
                .and_then(|value| json_id(Some(value)))
                .unwrap_or_default();
            let text = render_outbound(&params);
            if target.is_empty() || text.trim().is_empty() {
                failed_echo(&echo, "missing target or text")
            } else {
                match send_text(client, config, &target, &text).await {
                    Ok(message_id) => json!({
                        "status": "ok",
                        "retcode": 0,
                        "data": { "message_id": message_id },
                        "echo": echo,
                    }),
                    Err(error) => {
                        warn!("{LOG_PREFIX} send failed for {target}: {error}");
                        failed_echo(&echo, &error)
                    }
                }
            }
        }
        "get_login_info" => json!({
            "status": "ok",
            "retcode": 0,
            "data": {
                "user_id": config.self_wxid,
                "nickname": config.nickname.clone().unwrap_or_else(|| config.self_wxid.clone()),
            },
            "echo": echo,
        }),
        "get_stranger_info" => json!({
            "status": "ok",
            "retcode": 0,
            "data": {
                "user_id": params.get("user_id").and_then(|value| json_id(Some(value))).unwrap_or_default(),
                "nickname": "",
            },
            "echo": echo,
        }),
        other => {
            debug!("{LOG_PREFIX} unsupported action {other}");
            failed_echo(&echo, "unsupported action")
        }
    };

    BotAdapter::process_event(adapter.clone(), response.to_string()).await;
}

fn failed_echo(echo: &str, message: &str) -> Value {
    json!({
        "status": "failed",
        "retcode": 1404,
        "message": message,
        "echo": echo,
    })
}

/// Flatten OneBot message segments into the plain text WeChatPadPro's `SendText` accepts.
fn render_outbound(params: &Value) -> String {
    let mut text = String::new();
    let segments = match params.get("message") {
        Some(Value::Array(segments)) => segments.clone(),
        Some(Value::String(raw)) => return raw.clone(),
        _ => Vec::new(),
    };

    for segment in segments {
        let kind = segment.get("type").and_then(Value::as_str).unwrap_or_default();
        let data = segment.get("data").cloned().unwrap_or(Value::Null);
        match kind {
            "text" => text.push_str(&unbox_text(data.get("text").unwrap_or(&Value::Null))),
            // WeChatPad has no structured @ payload on SendText, so mentions degrade to text.
            "at" => {
                let target = json_id(data.get("qq").or_else(|| data.get("target")));
                if let Some(target) = target.filter(|value| !value.is_empty()) {
                    text.push('@');
                    text.push_str(&target);
                    text.push(' ');
                }
            }
            other => warn!("{LOG_PREFIX} outbound segment '{other}' is not supported by WeChatPad and was skipped"),
        }
    }

    text.trim().to_string()
}

/// Attach the bridge credential unless the deployment left it empty.
fn with_auth(
    request: reqwest::RequestBuilder,
    config: &WeChatPadConfig,
) -> reqwest::RequestBuilder {
    let token = config.pad_token.trim();
    if token.is_empty() {
        request
    } else {
        request.bearer_auth(token)
    }
}

async fn send_text(
    client: &reqwest::Client,
    config: &WeChatPadConfig,
    target: &str,
    text: &str,
) -> std::result::Result<String, String> {
    if config.base_url().is_empty() {
        return Err("pad_url is not configured".to_string());
    }

    let mut last_error = String::new();
    for path in SEND_TEXT_PATHS {
        let endpoint = format!("{}{}", config.base_url(), path);
        let body = json!({ "toUserName": target, "content": text });
        match with_auth(client.post(&endpoint), config).json(&body).send().await {
            Ok(response) if response.status().is_success() => {
                return Ok(format!("wp{}", MESSAGE_ID_COUNTER.fetch_add(1, Ordering::Relaxed)))
            }
            Ok(response) => last_error = format!("{path} returned HTTP {}", response.status()),
            Err(error) => last_error = format!("{path}: {error}"),
        }
    }

    Err(last_error)
}
