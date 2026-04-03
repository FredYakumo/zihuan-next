use zihuan_bot_types::event_model::MessageEvent;
use zihuan_llm_types::tooling::FunctionTool;
use redis::{aio::ConnectionManager, AsyncCommands};
use serde::Serialize;
use serde_json::Value;
use sqlx::mysql::MySqlPool;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;

tokio::task_local! {
    pub static SESSION_CLAIM_CONTEXT: Arc<SessionClaimContext>;
}

/// Redis connection configuration, passed between nodes as a reference
#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: Option<String>,
    pub reconnect_max_attempts: Option<u32>,
    pub reconnect_interval_secs: Option<u64>,
}

/// MySQL connection configuration, passed between nodes as a reference.
/// The `pool` field carries the live connection pool created by `MySqlNode`;
/// downstream nodes should use it directly instead of reconnecting.
/// The `runtime_handle` points at the tokio runtime that owns the pool's
/// background tasks, so downstream nodes can execute queries on the same
/// runtime instead of creating throwaway runtimes.
#[derive(Clone)]
pub struct MySqlConfig {
    pub url: Option<String>,
    pub reconnect_max_attempts: Option<u32>,
    pub reconnect_interval_secs: Option<u64>,
    /// Live connection pool maintained by the MySqlNode.
    pub pool: Option<MySqlPool>,
    /// Handle to the tokio runtime that owns the pool and its background tasks.
    pub runtime_handle: Option<tokio::runtime::Handle>,
}

impl fmt::Debug for MySqlConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MySqlConfig")
            .field("url", &self.url)
            .field("reconnect_max_attempts", &self.reconnect_max_attempts)
            .field("reconnect_interval_secs", &self.reconnect_interval_secs)
            .field("pool", &self.pool.as_ref().map(|_| "<MySqlPool>"))
            .field(
                "runtime_handle",
                &self.runtime_handle.as_ref().map(|_| "<Handle>"),
            )
            .finish()
    }
}

#[derive(Clone)]
pub struct TavilyRef {
    pub api_token: String,
    pub timeout: Duration,
}

impl TavilyRef {
    pub fn new(api_token: impl Into<String>, timeout: Duration) -> Self {
        Self {
            api_token: api_token.into(),
            timeout,
        }
    }
}

impl fmt::Debug for TavilyRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TavilyRef")
            .field("api_token", &"<redacted>")
            .field("timeout", &self.timeout)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct SessionStateEntry {
    pub in_session: bool,
    pub state_json: Value,
    pub(crate) claim_token: Option<u64>,
}

#[derive(Clone)]
pub struct SessionStateRef {
    pub node_id: String,
    pub entries: Arc<TokioMutex<HashMap<String, SessionStateEntry>>>,
    pub next_claim_token: Arc<AtomicU64>,
}

#[derive(Debug, Clone)]
pub struct SessionClaim {
    pub session_ref: Arc<SessionStateRef>,
    pub sender_id: String,
    pub claim_token: u64,
}

#[derive(Debug, Default)]
pub struct SessionClaimContext {
    claims: Mutex<HashMap<(String, String), SessionClaim>>,
}

impl SessionClaimContext {
    pub fn register_claim(&self, claim: SessionClaim) {
        self.claims.lock().unwrap().insert(
            (claim.session_ref.node_id.clone(), claim.sender_id.clone()),
            claim,
        );
    }

    pub fn unregister_claim(&self, session_node_id: &str, sender_id: &str) -> Option<SessionClaim> {
        self.claims
            .lock()
            .unwrap()
            .remove(&(session_node_id.to_string(), sender_id.to_string()))
    }

    pub fn claim_token_for(&self, session_node_id: &str, sender_id: &str) -> Option<u64> {
        self.claims
            .lock()
            .unwrap()
            .get(&(session_node_id.to_string(), sender_id.to_string()))
            .map(|claim| claim.claim_token)
    }

    pub fn drain_claims(&self) -> Vec<SessionClaim> {
        self.claims
            .lock()
            .unwrap()
            .drain()
            .map(|(_, claim)| claim)
            .collect()
    }
}

impl SessionStateRef {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            entries: Arc::new(TokioMutex::new(HashMap::new())),
            next_claim_token: Arc::new(AtomicU64::new(0)),
        }
    }

    pub async fn get_state(&self, sender_id: &str) -> SessionStateEntry {
        self.entries
            .lock()
            .await
            .get(sender_id)
            .cloned()
            .unwrap_or(SessionStateEntry {
                in_session: false,
                state_json: Value::Null,
                claim_token: None,
            })
    }

    pub async fn clear_state(&self, sender_id: &str) -> bool {
        self.entries.lock().await.remove(sender_id).is_some()
    }

    pub async fn try_claim(
        &self,
        sender_id: &str,
        state_json: Option<Value>,
    ) -> (SessionStateEntry, bool) {
        let mut entries = self.entries.lock().await;
        if let Some(existing) = entries.get(sender_id) {
            if existing.in_session {
                return (existing.clone(), false);
            }
        }

        let claim_token = self.next_claim_token.fetch_add(1, Ordering::Relaxed) + 1;
        let entry = SessionStateEntry {
            in_session: true,
            state_json: state_json.unwrap_or(Value::Null),
            claim_token: Some(claim_token),
        };
        entries.insert(sender_id.to_string(), entry.clone());
        (entry, true)
    }

    pub async fn release(&self, sender_id: &str, claim_token: Option<u64>) -> bool {
        let mut entries = self.entries.lock().await;
        let Some(existing) = entries.get(sender_id) else {
            return false;
        };

        if let Some(expected_token) = claim_token {
            if existing.claim_token != Some(expected_token) {
                return false;
            }
        }

        entries.insert(
            sender_id.to_string(),
            SessionStateEntry {
                in_session: false,
                state_json: Value::Null,
                claim_token: None,
            },
        );
        true
    }
}

impl fmt::Debug for SessionStateRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionStateRef")
            .field("node_id", &self.node_id)
            .field("entries", &"<TokioMutex<HashMap<...>>>")
            .finish()
    }
}

/// Run-scoped OpenAI message session cache reference, passed between nodes.
///
/// This reference points at the live storage owned by `OpenAIMessageSessionCacheProviderNode`.
/// The storage persists for the duration of a single graph execution and is reset
/// by the node on the next graph start.
#[derive(Clone)]
pub struct OpenAIMessageSessionCacheRef {
    pub node_id: String,
    pub memory_cache: Arc<TokioMutex<HashMap<String, Vec<zihuan_llm_types::OpenAIMessage>>>>,
    pub redis_cm: Arc<TokioMutex<Option<ConnectionManager>>>,
    pub cached_redis_url: Arc<TokioMutex<Option<String>>>,
    pub sender_bucket_map: Arc<TokioMutex<HashMap<String, String>>>,
    pub default_bucket_name: Arc<TokioMutex<String>>,
}

impl OpenAIMessageSessionCacheRef {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            memory_cache: Arc::new(TokioMutex::new(HashMap::new())),
            redis_cm: Arc::new(TokioMutex::new(None)),
            cached_redis_url: Arc::new(TokioMutex::new(None)),
            sender_bucket_map: Arc::new(TokioMutex::new(HashMap::new())),
            default_bucket_name: Arc::new(TokioMutex::new(Self::normalize_bucket_name(None))),
        }
    }

    fn normalize_bucket_name(bucket_name: Option<&str>) -> String {
        let bucket_name = bucket_name.unwrap_or("default").trim();
        if bucket_name.is_empty() {
            "default".to_string()
        } else {
            bucket_name.to_string()
        }
    }

    fn storage_key(&self, bucket_name: &str, sender_id: &str) -> String {
        format!(
            "openai_message_session:{}:{}:{}",
            self.node_id, bucket_name, sender_id
        )
    }

    async fn default_bucket_name(&self) -> String {
        self.default_bucket_name.lock().await.clone()
    }

    pub async fn set_default_bucket_name(&self, bucket_name: Option<&str>) {
        let bucket_name = Self::normalize_bucket_name(bucket_name);
        *self.default_bucket_name.lock().await = bucket_name;
    }

    pub async fn get_messages(
        &self,
        sender_id: &str,
    ) -> zihuan_core::error::Result<Vec<zihuan_llm_types::OpenAIMessage>> {
        let default_bucket_name = self.default_bucket_name().await;
        let bucket_name = {
            let sender_bucket_map = self.sender_bucket_map.lock().await;
            sender_bucket_map
                .get(sender_id)
                .cloned()
                .unwrap_or(default_bucket_name)
        };
        let key = self.storage_key(&bucket_name, sender_id);

        let redis_url = {
            let url_guard = self.cached_redis_url.lock().await;
            url_guard.clone()
        };

        if let Some(url) = redis_url {
            let mut cm_guard = self.redis_cm.lock().await;
            let mut url_guard = self.cached_redis_url.lock().await;

            if url_guard.as_deref() != Some(url.as_str()) {
                *cm_guard = None;
                *url_guard = Some(url.clone());
            }

            if cm_guard.is_none() {
                let client = redis::Client::open(url.as_str())?;
                match ConnectionManager::new(client).await {
                    Ok(cm) => {
                        *cm_guard = Some(cm);
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            if let Some(cm) = cm_guard.as_mut() {
                let existing_json: Option<String> = cm.get(&key).await?;
                if let Some(raw) = existing_json {
                    let messages: Vec<zihuan_llm_types::OpenAIMessage> = serde_json::from_str(&raw)?;
                    return Ok(messages);
                }
            }
        }

        let cache = self.memory_cache.lock().await;
        Ok(cache.get(&key).cloned().unwrap_or_default())
    }

    pub async fn clear_messages(&self, sender_id: &str) -> zihuan_core::error::Result<bool> {
        let default_bucket_name = self.default_bucket_name().await;
        let bucket_name = {
            let mut sender_bucket_map = self.sender_bucket_map.lock().await;
            sender_bucket_map
                .remove(sender_id)
                .unwrap_or(default_bucket_name)
        };
        let key = self.storage_key(&bucket_name, sender_id);
        let mut cleared = false;

        let redis_url = {
            let url_guard = self.cached_redis_url.lock().await;
            url_guard.clone()
        };

        if let Some(url) = redis_url {
            let mut cm_guard = self.redis_cm.lock().await;
            let mut url_guard = self.cached_redis_url.lock().await;

            if url_guard.as_deref() != Some(url.as_str()) {
                *cm_guard = None;
                *url_guard = Some(url.clone());
            }

            if cm_guard.is_none() {
                let client = redis::Client::open(url.as_str())?;
                match ConnectionManager::new(client).await {
                    Ok(cm) => {
                        *cm_guard = Some(cm);
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            if let Some(cm) = cm_guard.as_mut() {
                let deleted_count: i32 = cm.del(&key).await?;
                let tracker_key = format!(
                    "openai_message_session:{}:bucket:{}:keys",
                    self.node_id, bucket_name
                );
                let _: () = cm.srem(&tracker_key, &key).await?;
                cleared |= deleted_count > 0;
            }
        }

        let mut memory_cache = self.memory_cache.lock().await;
        cleared |= memory_cache.remove(&key).is_some();

        Ok(cleared)
    }

    pub async fn set_messages(
        &self,
        sender_id: &str,
        messages: Vec<zihuan_llm_types::OpenAIMessage>,
    ) -> zihuan_core::error::Result<()> {
        let default_bucket_name = self.default_bucket_name().await;
        let bucket_name = {
            let mut sender_bucket_map = self.sender_bucket_map.lock().await;
            sender_bucket_map
                .entry(sender_id.to_string())
                .or_insert(default_bucket_name)
                .clone()
        };
        let key = self.storage_key(&bucket_name, sender_id);

        let redis_url = {
            let url_guard = self.cached_redis_url.lock().await;
            url_guard.clone()
        };

        if let Some(url) = redis_url {
            let mut cm_guard = self.redis_cm.lock().await;
            let mut url_guard = self.cached_redis_url.lock().await;

            if url_guard.as_deref() != Some(url.as_str()) {
                *cm_guard = None;
                *url_guard = Some(url.clone());
            }

            if cm_guard.is_none() {
                let client = redis::Client::open(url.as_str())?;
                match ConnectionManager::new(client).await {
                    Ok(cm) => {
                        *cm_guard = Some(cm);
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            if let Some(cm) = cm_guard.as_mut() {
                let serialized = serde_json::to_string(&messages)?;
                cm.set::<_, _, ()>(&key, serialized).await?;
                let tracker_key = format!(
                    "openai_message_session:{}:bucket:{}:keys",
                    self.node_id, bucket_name
                );
                let tracker_registry_key =
                    format!("openai_message_session:{}:tracker_sets", self.node_id);
                cm.sadd::<_, _, ()>(&tracker_key, &key).await?;
                cm.sadd::<_, _, ()>(&tracker_registry_key, &tracker_key)
                    .await?;
            }
        }

        let mut memory_cache = self.memory_cache.lock().await;
        memory_cache.insert(key, messages);

        Ok(())
    }

    pub async fn append_messages(
        &self,
        sender_id: &str,
        incoming_messages: Vec<zihuan_llm_types::OpenAIMessage>,
    ) -> zihuan_core::error::Result<()> {
        let default_bucket_name = self.default_bucket_name().await;
        let bucket_name = {
            let mut sender_bucket_map = self.sender_bucket_map.lock().await;
            sender_bucket_map
                .entry(sender_id.to_string())
                .or_insert(default_bucket_name)
                .clone()
        };
        let key = self.storage_key(&bucket_name, sender_id);

        let redis_url = {
            let url_guard = self.cached_redis_url.lock().await;
            url_guard.clone()
        };

        if let Some(url) = redis_url {
            let mut cm_guard = self.redis_cm.lock().await;
            let mut url_guard = self.cached_redis_url.lock().await;

            if url_guard.as_deref() != Some(url.as_str()) {
                *cm_guard = None;
                *url_guard = Some(url.clone());
            }

            if cm_guard.is_none() {
                let client = redis::Client::open(url.as_str())?;
                match ConnectionManager::new(client).await {
                    Ok(cm) => {
                        *cm_guard = Some(cm);
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            if let Some(cm) = cm_guard.as_mut() {
                let existing_json: Option<String> = cm.get(&key).await?;
                let mut existing_messages: Vec<zihuan_llm_types::OpenAIMessage> = existing_json
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()?
                    .unwrap_or_default();
                existing_messages.extend(incoming_messages.clone());

                let serialized = serde_json::to_string(&existing_messages)?;
                cm.set::<_, _, ()>(&key, serialized).await?;
                let tracker_key = format!(
                    "openai_message_session:{}:bucket:{}:keys",
                    self.node_id, bucket_name
                );
                let tracker_registry_key =
                    format!("openai_message_session:{}:tracker_sets", self.node_id);
                cm.sadd::<_, _, ()>(&tracker_key, &key).await?;
                cm.sadd::<_, _, ()>(&tracker_registry_key, &tracker_key)
                    .await?;
            }
        }

        let mut memory_cache = self.memory_cache.lock().await;
        let entry = memory_cache.entry(key).or_default();
        entry.extend(incoming_messages);

        Ok(())
    }
}

impl fmt::Debug for OpenAIMessageSessionCacheRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAIMessageSessionCacheRef")
            .field("node_id", &self.node_id)
            .field("memory_cache", &"<TokioMutex<HashMap<...>>>")
            .field("redis_cm", &"<TokioMutex<Option<ConnectionManager>>>")
            .field("cached_redis_url", &"<TokioMutex<Option<String>>>")
            .field("sender_bucket_map", &"<TokioMutex<HashMap<...>>>")
            .field("default_bucket_name", &"<TokioMutex<String>>")
            .finish()
    }
}

/// Shared loop control state, passed from LoopNode to LoopBreakNode via LoopControlRef ports.
pub struct LoopControl {
    break_flag: AtomicBool,
    /// Carries the "current iteration data" so downstream nodes can write back
    /// updated state without creating a data-edge cycle in the graph.
    current_state: Mutex<DataValue>,
}

impl std::fmt::Debug for LoopControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoopControl")
            .field("break_flag", &self.break_flag.load(Ordering::Relaxed))
            .field("current_state", &"<Mutex<DataValue>>")
            .finish()
    }
}

impl LoopControl {
    pub fn new() -> Self {
        Self {
            break_flag: AtomicBool::new(false),
            current_state: Mutex::new(DataValue::Boolean(false)),
        }
    }
    pub fn request_break(&self) {
        self.break_flag.store(true, Ordering::SeqCst);
    }
    pub fn should_break(&self) -> bool {
        self.break_flag.load(Ordering::SeqCst)
    }
    pub fn reset(&self) {
        self.break_flag.store(false, Ordering::SeqCst);
    }
    /// Called by LoopNode on_start to set the initial state.
    pub fn init_state(&self, v: DataValue) {
        *self.current_state.lock().unwrap() = v;
    }
    /// Called by LoopNode on_update to read the current state.
    pub fn get_state(&self) -> DataValue {
        self.current_state.lock().unwrap().clone()
    }
    /// Called by LoopStateUpdateNode to write the updated state for the next iteration.
    pub fn update_state(&self, v: DataValue) {
        *self.current_state.lock().unwrap() = v;
    }
}

/// Dataflow datatype. Use for checking compatibility between ports.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum DataType {
    Any,
    String,
    Integer,
    Float,
    Boolean,
    Json,
    Binary,
    Vec(Box<DataType>),
    MessageEvent,
    OpenAIMessage,
    QQMessage,
    FunctionTools,
    BotAdapterRef,
    RedisRef,
    MySqlRef,
    TavilyRef,
    SessionStateRef,
    OpenAIMessageSessionCacheRef,
    Password,
    LLModel,
    LoopControlRef,
    Custom(String),
}

impl DataType {
    pub fn is_compatible_with(&self, other: &DataType) -> bool {
        match (self, other) {
            (DataType::Any, _) | (_, DataType::Any) => true,
            (DataType::Vec(left), DataType::Vec(right)) => left.is_compatible_with(right),
            _ => self == other,
        }
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Any => write!(f, "Any"),
            DataType::String => write!(f, "String"),
            DataType::Integer => write!(f, "Integer"),
            DataType::Float => write!(f, "Float"),
            DataType::Boolean => write!(f, "Boolean"),
            DataType::Json => write!(f, "Json"),
            DataType::Binary => write!(f, "Binary"),
            DataType::Vec(inner) => write!(f, "Vec<{}>", inner),
            DataType::MessageEvent => write!(f, "MessageEvent"),
            DataType::OpenAIMessage => write!(f, "OpenAIMessage"),
            DataType::QQMessage => write!(f, "QQMessage"),
            DataType::FunctionTools => write!(f, "FunctionTools"),
            DataType::BotAdapterRef => write!(f, "BotAdapterRef"),
            DataType::RedisRef => write!(f, "RedisRef"),
            DataType::MySqlRef => write!(f, "MySqlRef"),
            DataType::TavilyRef => write!(f, "TavilyRef"),
            DataType::SessionStateRef => write!(f, "SessionStateRef"),
            DataType::OpenAIMessageSessionCacheRef => write!(f, "OpenAIMessageSessionCacheRef"),
            DataType::Password => write!(f, "Password"),
            DataType::LLModel => write!(f, "LLModel"),
            DataType::LoopControlRef => write!(f, "LoopControlRef"),
            DataType::Custom(name) => write!(f, "Custom({})", name),
        }
    }
}

impl<'de> serde::Deserialize<'de> for DataType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        struct DataTypeVisitor;

        impl DataTypeVisitor {
            fn from_str<E: de::Error>(s: &str) -> Result<DataType, E> {
                // Backward-compat: handle "Vec<Inner>" string format produced by Display
                if let Some(inner_str) = s.strip_prefix("Vec<").and_then(|t| t.strip_suffix('>')) {
                    let inner = DataTypeVisitor::from_str(inner_str)?;
                    return Ok(DataType::Vec(Box::new(inner)));
                }
                match s {
                    "Any" => Ok(DataType::Any),
                    "String" => Ok(DataType::String),
                    "Integer" => Ok(DataType::Integer),
                    "Float" => Ok(DataType::Float),
                    "Boolean" => Ok(DataType::Boolean),
                    "Json" => Ok(DataType::Json),
                    "Binary" => Ok(DataType::Binary),
                    "MessageEvent" => Ok(DataType::MessageEvent),
                    "OpenAIMessage" => Ok(DataType::OpenAIMessage),
                    "Message" => Ok(DataType::OpenAIMessage),
                    "QQMessage" => Ok(DataType::QQMessage),
                    "FunctionTools" => Ok(DataType::FunctionTools),
                    "BotAdapterRef" => Ok(DataType::BotAdapterRef),
                    "RedisRef" => Ok(DataType::RedisRef),
                    "MySqlRef" => Ok(DataType::MySqlRef),
                    "TavilyRef" => Ok(DataType::TavilyRef),
                    "SessionStateRef" => Ok(DataType::SessionStateRef),
                    "OpenAIMessageSessionCacheRef" => Ok(DataType::OpenAIMessageSessionCacheRef),
                    "Password" => Ok(DataType::Password),
                    "LLModel" => Ok(DataType::LLModel),
                    "LoopControlRef" => Ok(DataType::LoopControlRef),
                    other => Err(de::Error::unknown_variant(
                        other,
                        &[
                            "Any",
                            "String",
                            "Integer",
                            "Float",
                            "Boolean",
                            "Json",
                            "Binary",
                            "Vec",
                            "MessageEvent",
                            "OpenAIMessage",
                            "Message",
                            "QQMessage",
                            "FunctionTools",
                            "BotAdapterRef",
                            "RedisRef",
                            "MySqlRef",
                            "TavilyRef",
                            "SessionStateRef",
                            "OpenAIMessageSessionCacheRef",
                            "Password",
                            "LLModel",
                            "LoopControlRef",
                            "Custom",
                        ],
                    )),
                }
            }
        }

        impl<'de> Visitor<'de> for DataTypeVisitor {
            type Value = DataType;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a DataType string or {\"Vec\":...} / {\"Custom\":...} object")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                DataTypeVisitor::from_str(v)
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let key: String = map
                    .next_key()?
                    .ok_or_else(|| de::Error::missing_field("variant key"))?;
                match key.as_str() {
                    "Vec" => {
                        let inner: DataType = map.next_value()?;
                        Ok(DataType::Vec(Box::new(inner)))
                    }
                    "Custom" => {
                        let name: String = map.next_value()?;
                        Ok(DataType::Custom(name))
                    }
                    other => Err(de::Error::unknown_variant(other, &["Vec", "Custom"])),
                }
            }
        }

        deserializer.deserialize_any(DataTypeVisitor)
    }
}

/// Actual data flowing through the dataflow graph
#[derive(Clone)]
pub enum DataValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Json(Value),
    Binary(Vec<u8>),
    Vec(Box<DataType>, std::vec::Vec<DataValue>),
    MessageEvent(MessageEvent),
    OpenAIMessage(zihuan_llm_types::OpenAIMessage),
    QQMessage(zihuan_bot_types::message::Message),
    FunctionTools(Vec<Arc<dyn FunctionTool>>),
    BotAdapterRef(zihuan_bot_types::BotAdapterHandle),
    RedisRef(Arc<RedisConfig>),
    MySqlRef(Arc<MySqlConfig>),
    TavilyRef(Arc<TavilyRef>),
    SessionStateRef(Arc<SessionStateRef>),
    OpenAIMessageSessionCacheRef(Arc<OpenAIMessageSessionCacheRef>),
    Password(String),
    LLModel(Arc<dyn zihuan_llm_types::llm_base::LLMBase>),
    LoopControlRef(Arc<LoopControl>),
}

impl DataValue {
    pub fn data_type(&self) -> DataType {
        match self {
            DataValue::String(_) => DataType::String,
            DataValue::Integer(_) => DataType::Integer,
            DataValue::Float(_) => DataType::Float,
            DataValue::Boolean(_) => DataType::Boolean,
            DataValue::Json(_) => DataType::Json,
            DataValue::Binary(_) => DataType::Binary,
            DataValue::Vec(ty, _) => DataType::Vec(ty.clone()),
            DataValue::OpenAIMessage(_) => DataType::OpenAIMessage,
            DataValue::QQMessage(_) => DataType::QQMessage,
            DataValue::MessageEvent(_) => DataType::MessageEvent,
            DataValue::FunctionTools(_) => DataType::FunctionTools,
            DataValue::BotAdapterRef(_) => DataType::BotAdapterRef,
            DataValue::RedisRef(_) => DataType::RedisRef,
            DataValue::MySqlRef(_) => DataType::MySqlRef,
            DataValue::TavilyRef(_) => DataType::TavilyRef,
            DataValue::SessionStateRef(_) => DataType::SessionStateRef,
            DataValue::OpenAIMessageSessionCacheRef(_) => DataType::OpenAIMessageSessionCacheRef,
            DataValue::Password(_) => DataType::Password,
            DataValue::LLModel(_) => DataType::LLModel,
            DataValue::LoopControlRef(_) => DataType::LoopControlRef,
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            DataValue::String(value) | DataValue::Password(value) => value.clone(),
            DataValue::Integer(value) => value.to_string(),
            DataValue::Float(value) => value.to_string(),
            DataValue::Boolean(value) => value.to_string(),
            DataValue::BotAdapterRef(_) => "BotAdapterRef".to_string(),
            DataValue::TavilyRef(_) => "TavilyRef".to_string(),
            DataValue::LoopControlRef(_) => "LoopControlRef".to_string(),
            other => {
                serde_json::to_string(&other.to_json()).unwrap_or_else(|_| format!("{other:?}"))
            }
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            DataValue::String(s) => Value::String(s.clone()),
            DataValue::Integer(i) => Value::Number((*i).into()),
            DataValue::Float(f) => serde_json::json!(f),
            DataValue::Boolean(b) => Value::Bool(*b),
            DataValue::Json(v) => v.clone(),
            DataValue::Binary(bytes) => {
                Value::Array(bytes.iter().map(|b| Value::Number((*b).into())).collect())
            }
            DataValue::Vec(_, items) => {
                Value::Array(items.iter().map(|item| item.to_json()).collect())
            }
            DataValue::OpenAIMessage(m) => {
                serde_json::json!({
                    "role": zihuan_llm_types::role_to_str(&m.role),
                    "content": m.content,
                    "tool_calls": m.tool_calls,
                })
            }
            DataValue::QQMessage(m) => serde_json::to_value(m).unwrap_or(Value::Null),
            DataValue::MessageEvent(event) => {
                serde_json::json!({
                    "message_id": event.message_id,
                    "message_type": event.message_type.as_str(),
                    "sender": {
                        "user_id": event.sender.user_id,
                        "nickname": event.sender.nickname,
                        "card": event.sender.card,
                        "role": event.sender.role,
                    },
                    "group_id": event.group_id,
                    "group_name": event.group_name,
                    "is_group_message": event.is_group_message,
                })
            }
            DataValue::FunctionTools(tools) => {
                let tool_defs: Vec<Value> = tools.iter().map(|t| t.get_json()).collect();
                Value::Array(tool_defs)
            }
            DataValue::Password(value) => Value::String(value.clone()),
            DataValue::LLModel(m) => serde_json::json!({
                "type": "LLModel",
                "model_name": m.get_model_name(),
            }),
            DataValue::BotAdapterRef(_) => Value::String("BotAdapterRef".to_string()),
            DataValue::RedisRef(config) => serde_json::json!({
                "type": "RedisRef",
                "url": config.url,
                "reconnect_max_attempts": config.reconnect_max_attempts,
                "reconnect_interval_secs": config.reconnect_interval_secs,
            }),
            DataValue::MySqlRef(config) => serde_json::json!({
                "type": "MySqlRef",
                "url": config.url,
                "reconnect_max_attempts": config.reconnect_max_attempts,
                "reconnect_interval_secs": config.reconnect_interval_secs,
            }),
            DataValue::TavilyRef(tavily_ref) => serde_json::json!({
                "type": "TavilyRef",
                "timeout_secs": tavily_ref.timeout.as_secs(),
            }),
            DataValue::SessionStateRef(session_ref) => serde_json::json!({
                "type": "SessionStateRef",
                "node_id": session_ref.node_id,
            }),
            DataValue::OpenAIMessageSessionCacheRef(cache_ref) => serde_json::json!({
                "type": "OpenAIMessageSessionCacheRef",
                "node_id": cache_ref.node_id,
            }),
            DataValue::LoopControlRef(_) => Value::Null,
        }
    }
}

impl fmt::Debug for DataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataValue::String(value) => f.debug_tuple("String").field(value).finish(),
            DataValue::Integer(value) => f.debug_tuple("Integer").field(value).finish(),
            DataValue::Float(value) => f.debug_tuple("Float").field(value).finish(),
            DataValue::Boolean(value) => f.debug_tuple("Boolean").field(value).finish(),
            DataValue::Json(value) => f.debug_tuple("Json").field(value).finish(),
            DataValue::Binary(value) => f.debug_tuple("Binary").field(value).finish(),
            DataValue::Vec(ty, value) => f.debug_tuple("Vec").field(ty).field(value).finish(),
            DataValue::OpenAIMessage(value) => f.debug_tuple("OpenAIMessage").field(value).finish(),
            DataValue::QQMessage(value) => f.debug_tuple("QQMessage").field(value).finish(),
            DataValue::MessageEvent(value) => f.debug_tuple("MessageEvent").field(value).finish(),
            DataValue::FunctionTools(value) => f.debug_tuple("FunctionTools").field(value).finish(),
            DataValue::BotAdapterRef(_) => f.debug_tuple("BotAdapterRef").finish(),
            DataValue::RedisRef(config) => f.debug_tuple("RedisRef").field(config).finish(),
            DataValue::MySqlRef(config) => f.debug_tuple("MySqlRef").field(config).finish(),
            DataValue::TavilyRef(tavily_ref) => f.debug_tuple("TavilyRef").field(tavily_ref).finish(),
            DataValue::SessionStateRef(session_ref) => {
                f.debug_tuple("SessionStateRef").field(session_ref).finish()
            }
            DataValue::OpenAIMessageSessionCacheRef(cache_ref) => f
                .debug_tuple("OpenAIMessageSessionCacheRef")
                .field(cache_ref)
                .finish(),
            DataValue::Password(value) => f.debug_tuple("Password").field(value).finish(),
            DataValue::LLModel(m) => f.debug_tuple("LLModel").field(&m.get_model_name()).finish(),
            DataValue::LoopControlRef(_) => f.debug_tuple("LoopControlRef").finish(),
        }
    }
}

impl Serialize for DataValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_json().serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::DataType;

    #[test]
    fn any_type_is_compatible_with_concrete_types() {
        assert!(DataType::Any.is_compatible_with(&DataType::String));
        assert!(DataType::MessageEvent.is_compatible_with(&DataType::Any));
        assert!(DataType::Any.is_compatible_with(&DataType::Vec(Box::new(DataType::Integer))));
    }

    #[test]
    fn concrete_types_remain_strict() {
        assert!(DataType::String.is_compatible_with(&DataType::String));
        assert!(!DataType::String.is_compatible_with(&DataType::Integer));
        assert!(!DataType::Vec(Box::new(DataType::String))
            .is_compatible_with(&DataType::Vec(Box::new(DataType::Integer))));
    }
}
