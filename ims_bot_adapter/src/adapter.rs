use async_recursion::async_recursion;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

use super::event;
use super::models::{MessageEvent, MessageType, Profile, RawMessageEvent};
use crate::ws_action::ws_send_action_async;
use storage_handler::{
    enrich_event_images, enrich_message_images, ImageCacheAdapter, PendingImageUpload,
};
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::{mpsc, oneshot};
use zihuan_core::error::Result;
use zihuan_core::ims_bot_adapter::models::message::{ForwardNodeMessage, Message};
use zihuan_core::url_utils::extract_host;
use zihuan_graph_engine::message_restore::restore_message_snapshot;
use zihuan_graph_engine::object_storage::S3Ref;

/// Trait for brain agents that handle event processing
pub trait BrainAgentTrait: Send + Sync {
    fn on_event(
        &self,
        ims_bot_adapter: &mut BotAdapter,
        event: &super::models::MessageEvent,
    ) -> Result<()>;
    fn name(&self) -> &'static str;
    fn clone_box(&self) -> AgentBox;
}

pub type AgentBox = Box<dyn BrainAgentTrait>;

impl Clone for AgentBox {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Configuration for BotAdapter initialization
pub struct BotAdapterConfig {
    pub url: String,
    pub token: String,
    pub qq_id: String,
    pub brain_agent: Option<AgentBox>,
    pub object_storage: Option<Arc<S3Ref>>,
}

impl BotAdapterConfig {
    pub fn new(url: impl Into<String>, token: impl Into<String>, qq_id: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            token: token.into(),
            qq_id: qq_id.into(),
            brain_agent: None,
            object_storage: None,
        }
    }

    pub fn with_brain_agent(mut self, agent: Option<AgentBox>) -> Self {
        self.brain_agent = agent;
        self
    }

    pub fn with_object_storage(mut self, object_storage: Option<Arc<S3Ref>>) -> Self {
        self.object_storage = object_storage;
        self
    }
}

/// Pending action response channels keyed by echo ID.
pub type PendingActions = Arc<TokioMutex<HashMap<String, oneshot::Sender<serde_json::Value>>>>;

/// BotAdapter connects to the QQ bot server via WebSocket and processes events
pub struct BotAdapter {
    url: String,
    token: String,
    bot_profile: Option<Profile>,
    brain_agent: Option<AgentBox>,
    event_handlers: Vec<event::EventHandler>,
    /// Sender half for outbound WebSocket actions (set once the connection is live).
    pub action_tx: Option<mpsc::UnboundedSender<String>>,
    /// Echo → oneshot channel map for correlating action responses.
    pub pending_actions: PendingActions,
    pub object_storage: Option<Arc<S3Ref>>,
    pub pending_image_uploads: Arc<TokioMutex<VecDeque<PendingImageUpload>>>,
    pub image_retry_task_running: Arc<AtomicBool>,
}

/// Shared handle for BotAdapter that allows mutation inside async tasks
pub type SharedBotAdapter = Arc<TokioMutex<BotAdapter>>;

#[derive(Clone)]
struct BotAdapterImageCacheHandle(SharedBotAdapter);

#[async_trait::async_trait]
impl ImageCacheAdapter for BotAdapterImageCacheHandle {
    async fn object_storage(&self) -> Option<Arc<S3Ref>> {
        self.0.lock().await.object_storage.clone()
    }

    async fn enqueue_pending_image(&self, pending: PendingImageUpload) -> bool {
        use std::sync::atomic::Ordering;

        let (queue, running_flag) = {
            let guard = self.0.lock().await;
            (
                guard.pending_image_uploads.clone(),
                guard.image_retry_task_running.clone(),
            )
        };

        queue.lock().await.push_back(pending);
        !running_flag.swap(true, Ordering::SeqCst)
    }

    async fn dequeue_pending_image(&self) -> Option<PendingImageUpload> {
        let queue = {
            let guard = self.0.lock().await;
            guard.pending_image_uploads.clone()
        };
        let item = queue.lock().await.pop_front();
        item
    }

    async fn set_retry_task_running(&self, running: bool) {
        use std::sync::atomic::Ordering;

        let flag = {
            let guard = self.0.lock().await;
            guard.image_retry_task_running.clone()
        };
        flag.store(running, Ordering::SeqCst);
    }

    async fn image_detail_request_context(&self) -> (String, String) {
        let guard = self.0.lock().await;
        (guard.get_http_base_url(), guard.get_token().to_string())
    }
}

impl BotAdapter {
    pub async fn new(config: BotAdapterConfig) -> Self {
        Self {
            url: config.url,
            token: config.token,
            bot_profile: Some(Profile {
                qq_id: config.qq_id,
                ..Default::default()
            }),
            brain_agent: config.brain_agent,
            event_handlers: Vec::new(),
            action_tx: None,
            pending_actions: Arc::new(TokioMutex::new(HashMap::new())),
            object_storage: config.object_storage,
            pending_image_uploads: Arc::new(TokioMutex::new(VecDeque::new())),
            image_retry_task_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Convert this adapter into a shared, mutex-protected handle
    pub fn into_shared(self) -> SharedBotAdapter {
        Arc::new(TokioMutex::new(self))
    }
}

/// Downcast a type-erased `BotAdapterHandle` back to `SharedBotAdapter`.
///
/// # Panics
/// Panics if the handle did not originate from a `SharedBotAdapter`.
pub fn shared_from_handle(
    handle: &zihuan_core::ims_bot_adapter::BotAdapterHandle,
) -> SharedBotAdapter {
    handle
        .clone()
        .downcast::<TokioMutex<BotAdapter>>()
        .expect("BotAdapterHandle contains unexpected concrete type")
}

impl BotAdapter {
    pub fn get_bot_id(&self) -> &str {
        self.bot_profile
            .as_ref()
            .expect("BotProfile must be initialized before accessing bot_id")
            .qq_id
            .as_str()
    }

    pub fn get_bot_profile(&self) -> Option<&Profile> {
        self.bot_profile.as_ref()
    }

    /// Derive an HTTP base URL from the WebSocket URL (ws→http, wss→https, path stripped)
    pub fn get_http_base_url(&self) -> String {
        let url = &self.url;
        if url.starts_with("wss://") {
            let rest = &url["wss://".len()..];
            let host_port = rest.split('/').next().unwrap_or(rest);
            format!("https://{}", host_port)
        } else if url.starts_with("ws://") {
            let rest = &url["ws://".len()..];
            let host_port = rest.split('/').next().unwrap_or(rest);
            format!("http://{}", host_port)
        } else {
            url.clone()
        }
    }

    pub fn get_token(&self) -> &str {
        &self.token
    }

    pub fn get_brain_agent(&self) -> Option<&AgentBox> {
        self.brain_agent.as_ref()
    }

    pub fn register_event_handler(&mut self, handler: event::EventHandler) {
        self.event_handlers.push(handler);
    }

    pub fn get_event_handlers(&self) -> Vec<event::EventHandler> {
        self.event_handlers.clone()
    }

    /// Start the WebSocket connection and begin processing events using a shared handle
    pub async fn start(adapter: SharedBotAdapter) -> Result<()> {
        let (url, token) = {
            let guard = adapter.lock().await;
            (guard.url.clone(), guard.token.clone())
        };

        info!("Connecting to bot server at {}", url);

        // Build the WebSocket request with authorization header
        let request = http::Request::builder()
            .uri(&url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Host", extract_host(&url).unwrap_or("localhost"))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())?;

        let (ws_stream, _) = connect_async(request).await?;
        info!("Connected to the qq bot server successfully.");

        let (mut write, mut read) = ws_stream.split();

        let (action_tx, mut action_rx) = mpsc::unbounded_channel::<String>();
        {
            let mut guard = adapter.lock().await;
            guard.action_tx = Some(action_tx);
        }

        tokio::spawn(async move {
            while let Some(msg) = action_rx.recv().await {
                if write.send(WsMessage::Text(msg)).await.is_err() {
                    break;
                }
            }
        });

        while let Some(msg_result) = read.next().await {
            match msg_result {
                Ok(WsMessage::Text(text)) => {
                    let adapter_clone = adapter.clone();
                    BotAdapter::process_event(adapter_clone, text).await;
                }
                Ok(WsMessage::Binary(data)) => {
                    if let Ok(text) = String::from_utf8(data) {
                        let adapter_clone = adapter.clone();
                        BotAdapter::process_event(adapter_clone, text).await;
                    } else {
                        warn!("Received binary message that is not valid UTF-8");
                    }
                }
                Ok(WsMessage::Close(_)) => {
                    info!("WebSocket connection closed");
                    break;
                }
                Ok(WsMessage::Ping(_)) | Ok(WsMessage::Pong(_)) => {
                    // Heartbeat messages, ignore
                }
                Ok(WsMessage::Frame(_)) => {
                    // Raw frame, ignore
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Process a single event message
    async fn process_event(adapter: SharedBotAdapter, message: String) {
        debug!("Received message: {}", message);

        // Parse the JSON message
        let message_json: serde_json::Value = match serde_json::from_str(&message) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to parse message as JSON: {}", e);
                return;
            }
        };

        // Check if this is an action response (has "echo" field).
        // Dispatch it to the waiting oneshot channel and return early.
        if let Some(echo) = message_json.get("echo").and_then(|v| v.as_str()) {
            let pending = {
                let guard = adapter.lock().await;
                guard.pending_actions.clone()
            };
            let mut map = pending.lock().await;
            if let Some(tx) = map.remove(echo) {
                let _ = tx.send(message_json);
                return;
            }
        }

        // Check if this is a message event (has message_type field)
        if message_json.get("message_type").is_none() {
            debug!("Ignoring non-message event");
            return;
        }

        // Parse as RawMessageEvent
        let raw_event: RawMessageEvent = match serde_json::from_value(message_json) {
            Ok(e) => e,
            Err(e) => {
                error!("Failed to parse message event: {}", e);
                return;
            }
        };

        // Create the MessageEvent (messages are already deserialized in RawMessageEvent)
        let mut event = MessageEvent {
            message_id: raw_event.message_id,
            message_type: raw_event.message_type,
            sender: raw_event.sender.clone(),
            message_list: raw_event.message.clone(),
            group_id: raw_event.group_id,
            group_name: raw_event.group_name.clone(),
            is_group_message: matches!(raw_event.message_type, MessageType::Group),
        };

        let image_cache_handle = BotAdapterImageCacheHandle(adapter.clone());
        enrich_event_images(&image_cache_handle, &mut event).await;
        hydrate_message_segments(
            &adapter,
            &image_cache_handle,
            event.message_id,
            &mut event.message_list,
        )
        .await;

        // Dispatch to the unified message handler
        let adapter_clone = adapter.clone();
        tokio::spawn(async move {
            event::process_message(adapter_clone, event).await;
        });
    }
}

#[derive(Debug, serde::Deserialize)]
struct NapCatForwardResponse {
    data: Option<NapCatForwardData>,
}

#[derive(Debug, serde::Deserialize)]
struct NapCatForwardData {
    #[serde(default)]
    messages: Vec<NapCatForwardNode>,
}

#[derive(Debug, serde::Deserialize)]
struct NapCatForwardNode {
    #[serde(default)]
    data: NapCatForwardNodeData,
}

#[derive(Debug, serde::Deserialize, Default)]
struct NapCatForwardNodeData {
    #[serde(default, alias = "user_id", alias = "sender_id")]
    user_id: Option<serde_json::Value>,
    #[serde(default, alias = "name")]
    nickname: Option<String>,
    #[serde(default)]
    id: Option<serde_json::Value>,
    #[serde(default, alias = "message")]
    content: Vec<Message>,
}

impl NapCatForwardNodeData {
    fn value_to_string(value: Option<serde_json::Value>) -> Option<String> {
        match value? {
            serde_json::Value::String(text) => Some(text),
            serde_json::Value::Number(number) => Some(number.to_string()),
            _ => None,
        }
    }

    fn into_forward_node(self) -> ForwardNodeMessage {
        ForwardNodeMessage {
            user_id: Self::value_to_string(self.user_id),
            nickname: self.nickname,
            id: Self::value_to_string(self.id),
            content: self.content,
        }
    }
}

#[async_recursion]
async fn hydrate_message_segments(
    adapter: &SharedBotAdapter,
    image_cache_handle: &BotAdapterImageCacheHandle,
    root_message_id: i64,
    messages: &mut [Message],
) {
    for message in messages {
        match message {
            Message::Reply(reply) => match restore_message_snapshot(reply.id) {
                Ok(Some(snapshot)) => {
                    let image_count = snapshot
                        .messages
                        .iter()
                        .filter(|message| matches!(message, Message::Image(_)))
                        .count();
                    info!(
                        "[adapter] hydrated reply source for message_id={} via {} (segments={}, images={})",
                        reply.id,
                        snapshot.source.as_str(),
                        snapshot.messages.len(),
                        image_count
                    );
                    reply.message_source = Some(snapshot.messages);
                }
                Ok(None) => {
                    debug!(
                        "[adapter] reply source miss for message_id={} (no cache/mysql snapshot)",
                        reply.id
                    );
                }
                Err(error) => {
                    debug!(
                        "[adapter] failed to hydrate reply source for message_id={}: {}",
                        reply.id, error
                    );
                }
            },
            Message::Forward(forward) => {
                if forward.content.is_empty() {
                    if let Some(forward_id) = forward.id.clone() {
                        match fetch_forward_content(adapter, &forward_id).await {
                            Ok(content) => {
                                info!(
                                    "[adapter] hydrated forward content for forward_id={} (nodes={})",
                                    forward_id,
                                    content.len()
                                );
                                forward.content = content;
                            }
                            Err(error) => {
                                warn!(
                                    "[adapter] failed to hydrate forward content for forward_id={}: {}",
                                    forward_id, error
                                );
                            }
                        }
                    }
                }

                for node in &mut forward.content {
                    hydrate_message_segments(
                        adapter,
                        image_cache_handle,
                        root_message_id,
                        &mut node.content,
                    )
                    .await;
                    enrich_message_images(image_cache_handle, root_message_id, &mut node.content)
                        .await;
                }
            }
            _ => {}
        }
    }
}

async fn fetch_forward_content(
    adapter: &SharedBotAdapter,
    forward_id: &str,
) -> Result<Vec<ForwardNodeMessage>> {
    let response = ws_send_action_async(
        adapter,
        "get_forward_msg",
        serde_json::json!({ "message_id": forward_id }),
    )
    .await?;
    let payload: NapCatForwardResponse = serde_json::from_value(response)?;
    let nodes = payload
        .data
        .map(|data| {
            data.messages
                .into_iter()
                .map(|node| node.data.into_forward_node())
                .collect()
        })
        .unwrap_or_default();

    Ok(nodes)
}
