use futures_util::StreamExt;
use log::{debug, error, info, warn};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

use super::event;
use super::models::{MessageEvent, MessageType, Profile, RawMessageEvent};
use crate::util::message_store::MessageStore;
use crate::util::url_utils::extract_host;
use crate::error::Result;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;

/// Trait for brain agents that handle event processing
pub trait BrainAgentTrait: Send + Sync {
    fn on_event(&self, bot_adapter: &mut BotAdapter, event: &super::models::MessageEvent) -> Result<()>;
    fn name(&self) -> &'static str;
    fn clone_box(&self) -> AgentBox;
}

/// Type alias for a boxed brain agent
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
    pub redis_url: Option<String>,
    pub database_url: Option<String>,
    pub redis_reconnect_max_attempts: Option<u32>,
    pub redis_reconnect_interval_secs: Option<u64>,
    pub mysql_reconnect_max_attempts: Option<u32>,
    pub mysql_reconnect_interval_secs: Option<u64>,
    pub qq_id: String,
    pub brain_agent: Option<AgentBox>,
}

impl BotAdapterConfig {
    pub fn new(
        url: impl Into<String>,
        token: impl Into<String>,
        qq_id: impl Into<String>,
    ) -> Self {
        Self {
            url: url.into(),
            token: token.into(),
            redis_url: None,
            database_url: None,
            redis_reconnect_max_attempts: None,
            redis_reconnect_interval_secs: None,
            mysql_reconnect_max_attempts: None,
            mysql_reconnect_interval_secs: None,
            qq_id: qq_id.into(),
            brain_agent: None,
        }
    }

    pub fn with_redis_url(mut self, url: Option<String>) -> Self {
        self.redis_url = url;
        self
    }

    pub fn with_database_url(mut self, url: Option<String>) -> Self {
        self.database_url = url;
        self
    }

    pub fn with_redis_reconnect(mut self, max_attempts: Option<u32>, interval_secs: Option<u64>) -> Self {
        self.redis_reconnect_max_attempts = max_attempts;
        self.redis_reconnect_interval_secs = interval_secs;
        self
    }

    pub fn with_mysql_reconnect(mut self, max_attempts: Option<u32>, interval_secs: Option<u64>) -> Self {
        self.mysql_reconnect_max_attempts = max_attempts;
        self.mysql_reconnect_interval_secs = interval_secs;
        self
    }

    pub fn with_brain_agent(mut self, agent: Option<AgentBox>) -> Self {
        self.brain_agent = agent;
        self
    }
}

/// BotAdapter connects to the QQ bot server via WebSocket and processes events
pub struct BotAdapter {
    url: String,
    token: String,
    message_store: Arc<TokioMutex<MessageStore>>,
    bot_profile: Option<Profile>,
    brain_agent: Option<AgentBox>,
    event_handlers: Vec<event::EventHandler>,
}

/// Shared handle for BotAdapter that allows mutation inside async tasks
pub type SharedBotAdapter = Arc<TokioMutex<BotAdapter>>;

impl BotAdapter {
    pub async fn new(config: BotAdapterConfig) -> Self {
        // Use provided redis_url, fallback to env var
        let redis_url = config.redis_url.or_else(|| env::var("REDIS_URL").ok());

        // Use provided database_url, fallback to env var
        let database_url = if config.database_url.is_some() {
            config.database_url
        } else {
            env::var("DATABASE_URL").ok()
        };

        let message_store = Arc::new(TokioMutex::new(
            MessageStore::new(
                redis_url.as_deref(),
                database_url.as_deref(),
                config.redis_reconnect_max_attempts,
                config.redis_reconnect_interval_secs,
                config.mysql_reconnect_max_attempts,
                config.mysql_reconnect_interval_secs,
            )
            .await,
        ));
        
        // Load recent messages from MySQL into Redis/memory cache on startup
        {
            let store = message_store.lock().await;
            match store.load_messages_from_mysql(1000).await {
                Ok(count) => info!("[BotAdapter] Loaded {} messages from MySQL into cache on startup", count),
                Err(e) => warn!("[BotAdapter] Failed to load messages from MySQL: {}", e),
            }
        }

        Self {
            url: config.url,
            token: config.token,
            message_store,
            bot_profile: Some(Profile {
                qq_id: config.qq_id,
                ..Default::default()
            }),
            brain_agent: config.brain_agent,
            event_handlers: Vec::new(),
        }
    }

    /// Convert this adapter into a shared, mutex-protected handle
    pub fn into_shared(self) -> SharedBotAdapter {
        Arc::new(TokioMutex::new(self))
    }

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

    pub fn get_message_store(&self) -> Arc<TokioMutex<MessageStore>> {
        self.message_store.clone()
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
    pub async fn start(
        adapter: SharedBotAdapter,
    ) -> Result<()> {
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

        let (mut _write, mut read) = ws_stream.split();

        // Process incoming messages
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
        let event = MessageEvent {
            message_id: raw_event.message_id,
            message_type: raw_event.message_type,
            sender: raw_event.sender.clone(),
            message_list: raw_event.message.clone(),
            group_id: raw_event.group_id,
            group_name: raw_event.group_name.clone(),
            is_group_message: matches!(raw_event.message_type, MessageType::Group),
        };

        // Store the message in the message store (async spawn)
        let store = {
            let guard = adapter.lock().await;
            guard.message_store.clone()
        };
        let msg_id = raw_event.message_id.to_string();
        let msg_str = serde_json::to_string(&raw_event).unwrap_or_default();
        let store_for_spawn = store.clone();
        tokio::spawn(async move {
            let store = store_for_spawn.lock().await;
            store.store_message(&msg_id, &msg_str).await;
        });

        // Dispatch to the unified message handler
        let store = store.clone();
        let adapter_clone = adapter.clone();
        tokio::spawn(async move {
            event::process_message(adapter_clone, event, store).await;
        });
    }
}
