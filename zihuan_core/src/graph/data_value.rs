pub use crate::data_refs::{MySqlConfig, RelationalDbConnection, SqliteConfig};
use crate::storage::{LLMMessageSessionCacheRef, RedisConfig, S3Ref};
use crate::ims_bot_adapter::models::event_model::MessageEvent;
use crate::ims_bot_adapter::models::message::ImageMessage;
use crate::ims_bot_adapter::models::sender_model::Sender as GraphSender;
use crate::model_inference::llm::tooling::FunctionTool;
use crate::model_inference::llm::MessagePart;
pub use crate::rag::{WebSearchEngine, WebSearchImage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;

tokio::task_local! {
    pub static SESSION_CLAIM_CONTEXT: Arc<SessionClaimContext>;
}

tokio::task_local! {
    pub static EXECUTION_TASK_ID: String;
}


#[derive(Debug, Clone)]
pub struct SessionStateEntry {
    pub in_session: bool,
    pub state_json: Value,
    pub(crate) claim_token: Option<u64>,
    /// When the claim was taken; backs the staleness watchdog in `try_claim_with_ttl`.
    pub(crate) claimed_at: Option<std::time::Instant>,
}

impl SessionStateEntry {
    pub fn claim_token(&self) -> Option<u64> {
        self.claim_token
    }
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
        self.claims
            .lock()
            .unwrap()
            .insert((claim.session_ref.node_id.clone(), claim.sender_id.clone()), claim);
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
        self.claims.lock().unwrap().drain().map(|(_, claim)| claim).collect()
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
        self.entries.lock().await.get(sender_id).cloned().unwrap_or(SessionStateEntry {
            in_session: false,
            state_json: Value::Null,
            claim_token: None,
            claimed_at: None,
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
        self.try_claim_with_ttl(sender_id, state_json, None).await
    }

    /// Claim with a staleness watchdog. When `stale_ttl` is set and the existing claim
    /// is older than the TTL, the claim is treated as leaked (its owning task panicked
    /// or hung without releasing) and is stolen. `release` is token-guarded, so a late
    /// release from the stolen owner cannot undo the new claim.
    pub async fn try_claim_with_ttl(
        &self,
        sender_id: &str,
        state_json: Option<Value>,
        stale_ttl: Option<std::time::Duration>,
    ) -> (SessionStateEntry, bool) {
        let mut entries = self.entries.lock().await;
        if let Some(existing) = entries.get(sender_id) {
            if existing.in_session {
                let expired = match (existing.claimed_at, stale_ttl) {
                    (Some(claimed_at), Some(ttl)) => claimed_at.elapsed() > ttl,
                    _ => false,
                };
                if !expired {
                    return (existing.clone(), false);
                }
                log::warn!(
                    "Session claim for {sender_id} on {} held past {:?}; treating it as leaked and stealing it",
                    self.node_id, stale_ttl
                );
            }
        }

        let claim_token = self.next_claim_token.fetch_add(1, Ordering::Relaxed) + 1;
        let entry = SessionStateEntry {
            in_session: true,
            state_json: state_json.unwrap_or(Value::Null),
            claim_token: Some(claim_token),
            claimed_at: Some(std::time::Instant::now()),
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
                claimed_at: None,
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


/// Shared loop control state, passed from LoopNode to LoopBreakNode via LoopControlRef ports.
pub struct LoopControl {
    break_flag: AtomicBool,
    /// Carries the "current iteration data" so downstream nodes can write back
    /// updated state without creating a data-edge cycle in the graph.
    current_state: Mutex<DataValue>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageData {
    pub metadata: ImageMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_storage_path: Option<String>,
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
    Vector,
    MessageEvent,
    Sender,
    LLMMessage,
    QQMessage,
    Image,
    MessagePart,
    FunctionTools,
    BotAdapterRef,
    S3Ref,
    RedisRef,
    RdbRef,
    RetrievalStoreRef,
    WebSearchEngineRef,
    SessionStateRef,
    LLMMessageSessionCacheRef,
    Password,
    LLModel,
    EmbeddingModel,
    LoopControlRef,
    Custom(String),
}

impl DataType {
    pub fn is_compatible_with(&self, other: &DataType) -> bool {
        match (self, other) {
            (DataType::Any, _) | (_, DataType::Any) => true,
            (DataType::Vector, DataType::Vector) => true,
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
            DataType::Vector => write!(f, "Vector"),
            DataType::MessageEvent => write!(f, "MessageEvent"),
            DataType::Sender => write!(f, "Sender"),
            DataType::LLMMessage => write!(f, "LLMMessage"),
            DataType::QQMessage => write!(f, "QQMessage"),
            DataType::Image => write!(f, "Image"),
            DataType::MessagePart => write!(f, "MessagePart"),
            DataType::FunctionTools => write!(f, "FunctionTools"),
            DataType::BotAdapterRef => write!(f, "BotAdapterRef"),
            DataType::S3Ref => write!(f, "S3Ref"),
            DataType::RedisRef => write!(f, "RedisRef"),
            DataType::RdbRef => write!(f, "RdbRef"),
            DataType::RetrievalStoreRef => write!(f, "RetrievalStoreRef"),
            DataType::WebSearchEngineRef => write!(f, "WebSearchEngineRef"),
            DataType::SessionStateRef => write!(f, "SessionStateRef"),
            DataType::LLMMessageSessionCacheRef => write!(f, "LLMMessageSessionCacheRef"),
            DataType::Password => write!(f, "Password"),
            DataType::LLModel => write!(f, "LLModel"),
            DataType::EmbeddingModel => write!(f, "EmbeddingModel"),
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
                    "Vector" => Ok(DataType::Vector),
                    "MessageEvent" => Ok(DataType::MessageEvent),
                    "Sender" => Ok(DataType::Sender),
                    "LLMMessage" => Ok(DataType::LLMMessage),
                    "Message" => Ok(DataType::LLMMessage),
                    "QQMessage" => Ok(DataType::QQMessage),
                    "Image" => Ok(DataType::Image),
                    "MessagePart" => Ok(DataType::MessagePart),
                    "FunctionTools" => Ok(DataType::FunctionTools),
                    "BotAdapterRef" => Ok(DataType::BotAdapterRef),
                    "S3Ref" => Ok(DataType::S3Ref),
                    "RedisRef" => Ok(DataType::RedisRef),
                    "RdbRef" => Ok(DataType::RdbRef),
                    "RetrievalStoreRef" => Ok(DataType::RetrievalStoreRef),
                    "WebSearchEngineRef" => Ok(DataType::WebSearchEngineRef),
                    "SessionStateRef" => Ok(DataType::SessionStateRef),
                    "LLMMessageSessionCacheRef" => Ok(DataType::LLMMessageSessionCacheRef),
                    "Password" => Ok(DataType::Password),
                    "LLModel" => Ok(DataType::LLModel),
                    "EmbeddingModel" => Ok(DataType::EmbeddingModel),
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
                            "Vector",
                            "Vec",
                            "MessageEvent",
                            "Sender",
                            "LLMMessage",
                            "Message",
                            "QQMessage",
                            "Image",
                            "MessagePart",
                            "FunctionTools",
                            "BotAdapterRef",
                            "S3Ref",
                            "RedisRef",
                            "RdbRef",
                            "RetrievalStoreRef",
                            "WebSearchEngineRef",
                            "SessionStateRef",
                            "LLMMessageSessionCacheRef",
                            "Password",
                            "LLModel",
                            "EmbeddingModel",
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
                let key: String =
                    map.next_key()?.ok_or_else(|| de::Error::missing_field("variant key"))?;
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
    Vector(Vec<f32>),
    MessageEvent(MessageEvent),
    Sender(GraphSender),
    LLMMessage(crate::model_inference::llm::LLMMessage),
    QQMessage(crate::ims_bot_adapter::models::message::Message),
    Image(ImageData),
    MessagePart(MessagePart),
    FunctionTools(Vec<Arc<dyn FunctionTool>>),
    BotAdapterRef(crate::ims_bot_adapter::BotAdapterHandle),
    S3Ref(Arc<S3Ref>),
    RedisRef(Arc<RedisConfig>),
    RdbRef(RelationalDbConnection),
    RetrievalStoreRef(crate::retrieval::RetrievalStoreRef),
    WebSearchEngineRef(Arc<dyn WebSearchEngine>),
    SessionStateRef(Arc<SessionStateRef>),
    LLMMessageSessionCacheRef(Arc<LLMMessageSessionCacheRef>),
    Password(String),
    LLModel(Arc<dyn crate::model_inference::llm::llm_base::LLMBase>),
    EmbeddingModel(Arc<dyn crate::model_inference::llm::embedding_base::EmbeddingBase>),
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
            DataValue::Vector(_) => DataType::Vector,
            DataValue::Sender(_) => DataType::Sender,
            DataValue::LLMMessage(_) => DataType::LLMMessage,
            DataValue::QQMessage(_) => DataType::QQMessage,
            DataValue::Image(_) => DataType::Image,
            DataValue::MessagePart(_) => DataType::MessagePart,
            DataValue::MessageEvent(_) => DataType::MessageEvent,
            DataValue::FunctionTools(_) => DataType::FunctionTools,
            DataValue::BotAdapterRef(_) => DataType::BotAdapterRef,
            DataValue::S3Ref(_) => DataType::S3Ref,
            DataValue::RedisRef(_) => DataType::RedisRef,
            DataValue::RdbRef(_) => DataType::RdbRef,
            DataValue::RetrievalStoreRef(_) => DataType::RetrievalStoreRef,
            DataValue::WebSearchEngineRef(_) => DataType::WebSearchEngineRef,
            DataValue::SessionStateRef(_) => DataType::SessionStateRef,
            DataValue::LLMMessageSessionCacheRef(_) => DataType::LLMMessageSessionCacheRef,
            DataValue::Password(_) => DataType::Password,
            DataValue::LLModel(_) => DataType::LLModel,
            DataValue::EmbeddingModel(_) => DataType::EmbeddingModel,
            DataValue::LoopControlRef(_) => DataType::LoopControlRef,
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            DataValue::String(value) | DataValue::Password(value) => value.clone(),
            DataValue::Integer(value) => value.to_string(),
            DataValue::Float(value) => value.to_string(),
            DataValue::Boolean(value) => value.to_string(),
            DataValue::Vector(_) => "Vector".to_string(),
            DataValue::BotAdapterRef(_) => "BotAdapterRef".to_string(),
            DataValue::S3Ref(_) => "S3Ref".to_string(),
            DataValue::RetrievalStoreRef(_) => "RetrievalStoreRef".to_string(),
            DataValue::WebSearchEngineRef(_) => "WebSearchEngineRef".to_string(),
            DataValue::LoopControlRef(_) => "LoopControlRef".to_string(),
            DataValue::EmbeddingModel(_) => "EmbeddingModel".to_string(),
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
            DataValue::Vector(values) => {
                Value::Array(values.iter().map(|value| serde_json::json!(value)).collect())
            }
            DataValue::Vec(_, items) => {
                Value::Array(items.iter().map(|item| item.to_json()).collect())
            }
            DataValue::LLMMessage(m) => serde_json::to_value(m).unwrap_or(Value::Null),
            DataValue::QQMessage(m) => serde_json::to_value(m).unwrap_or(Value::Null),
            DataValue::Image(image) => serde_json::to_value(image).unwrap_or(Value::Null),
            DataValue::MessagePart(part) => serde_json::to_value(part).unwrap_or(Value::Null),
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
            DataValue::Sender(sender) => serde_json::to_value(sender).unwrap_or(Value::Null),
            DataValue::FunctionTools(tools) => {
                let tool_defs: Vec<Value> = tools.iter().map(|t| t.get_json()).collect();
                Value::Array(tool_defs)
            }
            DataValue::Password(value) => Value::String(value.clone()),
            DataValue::LLModel(m) => serde_json::json!({
                "type": "LLModel",
                "model_name": m.get_model_name(),
            }),
            DataValue::EmbeddingModel(m) => serde_json::json!({
                "type": "EmbeddingModel",
                "model_name": m.get_model_name(),
            }),
            DataValue::BotAdapterRef(_) => Value::String("BotAdapterRef".to_string()),
            DataValue::S3Ref(config) => serde_json::json!({
                "type": "S3Ref",
                "endpoint": config.endpoint,
                "bucket": config.bucket,
                "region": config.region,
                "public_base_url": config.public_base_url,
                "path_style": config.path_style,
            }),
            DataValue::RedisRef(config) => serde_json::json!({
                "type": "RedisRef",
                "url": config.url,
                "reconnect_max_attempts": config.reconnect_max_attempts,
                "reconnect_interval_secs": config.reconnect_interval_secs,
            }),
            DataValue::RdbRef(connection) => match connection {
                RelationalDbConnection::MySql(config) => serde_json::json!({
                    "type": "RdbRef",
                    "engine": "mysql",
                    "url": config.url,
                    "reconnect_max_attempts": config.reconnect_max_attempts,
                    "reconnect_interval_secs": config.reconnect_interval_secs,
                }),
                RelationalDbConnection::Sqlite(config) => serde_json::json!({
                    "type": "RdbRef",
                    "engine": "sqlite",
                    "path": config.path,
                }),
            },
            DataValue::RetrievalStoreRef(store) => serde_json::json!({
                "type": "RetrievalStoreRef",
                "backend": store.backend().as_str(),
                "connection_id": store.connection_id(),
            }),
            DataValue::WebSearchEngineRef(_) => serde_json::json!({
                "type": "WebSearchEngineRef",
            }),
            DataValue::SessionStateRef(session_ref) => serde_json::json!({
                "type": "SessionStateRef",
                "node_id": session_ref.node_id,
            }),
            DataValue::LLMMessageSessionCacheRef(cache_ref) => serde_json::json!({
                "type": "LLMMessageSessionCacheRef",
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
            DataValue::Vector(value) => f.debug_tuple("Vector").field(value).finish(),
            DataValue::LLMMessage(value) => f.debug_tuple("LLMMessage").field(value).finish(),
            DataValue::QQMessage(value) => f.debug_tuple("QQMessage").field(value).finish(),
            DataValue::Image(value) => f.debug_tuple("Image").field(value).finish(),
            DataValue::MessagePart(value) => f.debug_tuple("MessagePart").field(value).finish(),
            DataValue::MessageEvent(value) => f.debug_tuple("MessageEvent").field(value).finish(),
            DataValue::Sender(value) => f.debug_tuple("Sender").field(value).finish(),
            DataValue::FunctionTools(value) => f.debug_tuple("FunctionTools").field(value).finish(),
            DataValue::BotAdapterRef(_) => f.debug_tuple("BotAdapterRef").finish(),
            DataValue::S3Ref(config) => f.debug_tuple("S3Ref").field(config).finish(),
            DataValue::RedisRef(config) => f.debug_tuple("RedisRef").field(config).finish(),
            DataValue::RdbRef(connection) => f.debug_tuple("RdbRef").field(connection).finish(),
            DataValue::RetrievalStoreRef(store) => {
                f.debug_tuple("RetrievalStoreRef").field(store).finish()
            }
            DataValue::WebSearchEngineRef(_) => f.debug_tuple("WebSearchEngineRef").finish(),
            DataValue::SessionStateRef(session_ref) => {
                f.debug_tuple("SessionStateRef").field(session_ref).finish()
            }
            DataValue::LLMMessageSessionCacheRef(cache_ref) => {
                f.debug_tuple("LLMMessageSessionCacheRef").field(cache_ref).finish()
            }
            DataValue::Password(value) => f.debug_tuple("Password").field(value).finish(),
            DataValue::LLModel(m) => f.debug_tuple("LLModel").field(&m.get_model_name()).finish(),
            DataValue::EmbeddingModel(m) => {
                f.debug_tuple("EmbeddingModel").field(&m.get_model_name()).finish()
            }
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
