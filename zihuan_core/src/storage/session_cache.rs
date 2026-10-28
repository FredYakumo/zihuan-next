use std::collections::{BTreeSet, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use tokio::sync::Mutex as TokioMutex;
use tokio::task::block_in_place;

use crate::error::Result;
use crate::model_inference::llm::LLMMessage;
use crate::qq_context::QqChatContextBackend;
use crate::storage::redis;
use crate::storage::RedisConfig;

/// Run-scoped OpenAI message session cache reference, passed between nodes.
///
/// This reference points at the live storage owned by the service-layer agent or provider node.
/// The storage persists for the duration of a single graph execution and is reset
/// by the node on the next graph start.
#[derive(Clone)]
pub struct LLMMessageSessionCacheRef {
    pub node_id: String,
    pub memory_cache: Arc<TokioMutex<HashMap<String, Vec<LLMMessage>>>>,
    /// Redis settings and shared connection this cache reads and writes through.
    ///
    /// Kept public because the service seeds the URL directly after construction; all I/O goes
    /// through the `storage::redis` primitives so a dropped connection is rebuilt rather than
    /// left dead.
    pub redis: Arc<RedisConfig>,
    pub sender_bucket_map: Arc<TokioMutex<HashMap<String, String>>>,
    pub default_bucket_name: Arc<TokioMutex<String>>,
}

impl LLMMessageSessionCacheRef {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            memory_cache: Arc::new(TokioMutex::new(HashMap::new())),
            redis: Arc::new(RedisConfig::new(None, None, None, None, None)),
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
        format!("llm_message_session:{}:{}:{}", self.node_id, bucket_name, sender_id)
    }

    fn tracker_key(&self, bucket_name: &str) -> String {
        format!("llm_message_session:{}:bucket:{}:keys", self.node_id, bucket_name)
    }

    fn tracker_registry_key(&self) -> String {
        format!("llm_message_session:{}:tracker_sets", self.node_id)
    }

    async fn default_bucket_name(&self) -> String {
        self.default_bucket_name.lock().await.clone()
    }

    pub async fn set_default_bucket_name(&self, bucket_name: Option<&str>) {
        let bucket_name = Self::normalize_bucket_name(bucket_name);
        *self.default_bucket_name.lock().await = bucket_name;
    }

    /// Seed the Redis URL this cache reads and writes through.
    ///
    /// The connection itself is opened lazily by the first operation. Setting `cached_redis_url`
    /// is enough: `storage::redis` treats it as the URL the current connection uses and falls
    /// back to it when `url` was never seeded.
    pub async fn configure_redis(&self, url: Option<String>) {
        *self.redis.cached_redis_url.lock().await = url;
        // A stale connection must not survive a re-point at a different Redis.
        *self.redis.redis_cm.lock().await = None;
    }

    pub fn get_messages_blocking(&self, sender_id: &str) -> Result<Vec<LLMMessage>> {
        let sender_id = sender_id.to_string();
        let fut = self.get_messages(&sender_id);
        let run = || {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(fut))
            } else {
                tokio::runtime::Runtime::new()?.block_on(fut)
            }
        };

        match catch_unwind(AssertUnwindSafe(run)) {
            Ok(result) => result,
            Err(_) => Err(crate::string_error!(
                "Redis connection driver unexpectedly terminated while reading session cache"
            )),
        }
    }

    pub fn set_messages_blocking(&self, sender_id: &str, messages: Vec<LLMMessage>) -> Result<()> {
        let sender_id = sender_id.to_string();
        let fut = self.set_messages(&sender_id, messages);
        let run = || {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(fut))
            } else {
                tokio::runtime::Runtime::new()?.block_on(fut)
            }
        };

        match catch_unwind(AssertUnwindSafe(run)) {
            Ok(result) => result,
            Err(_) => Err(crate::string_error!(
                "Redis connection driver unexpectedly terminated while writing session cache"
            )),
        }
    }

    pub fn append_messages_blocking(
        &self,
        sender_id: &str,
        incoming_messages: Vec<LLMMessage>,
    ) -> Result<()> {
        let sender_id = sender_id.to_string();
        let fut = self.append_messages(&sender_id, incoming_messages);
        let run = || {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(fut))
            } else {
                tokio::runtime::Runtime::new()?.block_on(fut)
            }
        };

        match catch_unwind(AssertUnwindSafe(run)) {
            Ok(result) => result,
            Err(_) => Err(crate::string_error!(
                "Redis connection driver unexpectedly terminated while appending session cache"
            )),
        }
    }

    pub async fn get_messages(&self, sender_id: &str) -> Result<Vec<LLMMessage>> {
        let default_bucket_name = self.default_bucket_name().await;
        let bucket_name = {
            let sender_bucket_map = self.sender_bucket_map.lock().await;
            sender_bucket_map.get(sender_id).cloned().unwrap_or(default_bucket_name)
        };
        let key = self.storage_key(&bucket_name, sender_id);

        if self.redis.cached_redis_url.lock().await.is_some() {
            if let Some(raw) = redis::get_value(&self.redis, &key).await? {
                let messages: Vec<LLMMessage> = serde_json::from_str(&raw)?;
                return Ok(messages);
            }
        }

        let cache = self.memory_cache.lock().await;
        Ok(cache.get(&key).cloned().unwrap_or_default())
    }

    /// Which backend currently serves this cache's reads and writes.
    ///
    /// Redis is used whenever a URL has been seeded; otherwise the cache is process-memory
    /// only and is lost on restart.
    pub async fn backend(&self) -> QqChatContextBackend {
        if self.redis.cached_redis_url.lock().await.is_some() {
            QqChatContextBackend::Redis
        } else {
            QqChatContextBackend::Memory
        }
    }

    /// Every sender id that currently has a cached conversation in this session cache.
    ///
    /// Reads the in-process memory map and, when Redis is attached, unions the ids recorded in
    /// the Redis tracker sets so senders carried over from a previous process are included too.
    pub async fn list_sender_ids(&self) -> Vec<String> {
        let prefix = format!("llm_message_session:{}:", self.node_id);
        let mut senders = BTreeSet::new();

        {
            let cache = self.memory_cache.lock().await;
            for key in cache.keys() {
                if let Some(sender_id) = Self::parse_sender_id(key, &prefix) {
                    senders.insert(sender_id);
                }
            }
        }

        // Enumeration is best-effort: an unreachable Redis leaves the memory-derived list, but
        // the failure is logged because otherwise a broken connection looks like "no senders".
        if self.redis.cached_redis_url.lock().await.is_some() {
            match redis::set_members(&self.redis, &self.tracker_registry_key()).await {
                Ok(tracker_keys) => {
                    for tracker_key in tracker_keys {
                        match redis::set_members(&self.redis, &tracker_key).await {
                            Ok(keys) => {
                                for key in keys {
                                    if let Some(sender_id) = Self::parse_sender_id(&key, &prefix)
                                    {
                                        senders.insert(sender_id);
                                    }
                                }
                            }
                            Err(error) => log::warn!(
                                "[session_cache] failed to read tracker set '{tracker_key}': {error}"
                            ),
                        }
                    }
                }
                Err(error) => log::warn!(
                    "[session_cache] failed to read sender registry '{}': {error}",
                    self.tracker_registry_key()
                ),
            }
        }

        senders.into_iter().collect()
    }

    /// Split a storage key `llm_message_session:{node}:{bucket}:{sender}` into its sender id.
    ///
    /// The same cache also holds auxiliary histories whose key adds another segment, such as the
    /// legacy preprompt transcript `chat_preprompt:{sender}`. Those are not user conversations and
    /// must not surface as senders, so only single-segment keys qualify.
    fn parse_sender_id(key: &str, prefix: &str) -> Option<String> {
        let remainder = key.strip_prefix(prefix)?;
        let (_, sender_id) = remainder.split_once(':')?;
        if sender_id.is_empty() || sender_id.contains(':') {
            return None;
        }
        Some(sender_id.to_string())
    }

    /// Read and delete an auxiliary string value from the Redis backing this cache.
    ///
    /// Used to migrate data out of a legacy key exactly once; returns `None` when the cache is
    /// memory-only or the key is absent.
    pub async fn redis_take_string(&self, key: &str) -> Option<String> {
        if self.redis.cached_redis_url.lock().await.is_none() {
            return None;
        }
        redis::take_value(&self.redis, key).await.ok().flatten()
    }

    pub async fn clear_messages(&self, sender_id: &str) -> Result<bool> {
        let default_bucket_name = self.default_bucket_name().await;
        let bucket_name = {
            let mut sender_bucket_map = self.sender_bucket_map.lock().await;
            sender_bucket_map.remove(sender_id).unwrap_or(default_bucket_name)
        };
        let key = self.storage_key(&bucket_name, sender_id);
        let mut cleared = false;

        if self.redis.cached_redis_url.lock().await.is_some() {
            let deleted_count = redis::del_value(&self.redis, &key).await?;
            redis::set_remove(&self.redis, &self.tracker_key(&bucket_name), &key).await?;
            cleared |= deleted_count > 0;
        }

        let mut memory_cache = self.memory_cache.lock().await;
        cleared |= memory_cache.remove(&key).is_some();

        Ok(cleared)
    }

    pub async fn set_messages(&self, sender_id: &str, messages: Vec<LLMMessage>) -> Result<()> {
        let default_bucket_name = self.default_bucket_name().await;
        let bucket_name = {
            let mut sender_bucket_map = self.sender_bucket_map.lock().await;
            sender_bucket_map
                .entry(sender_id.to_string())
                .or_insert(default_bucket_name)
                .clone()
        };
        let key = self.storage_key(&bucket_name, sender_id);

        if self.redis.cached_redis_url.lock().await.is_some() {
            let serialized = serde_json::to_string(&messages)?;
            redis::set_value(&self.redis, &key, &serialized).await?;
            let tracker_key = self.tracker_key(&bucket_name);
            redis::set_add(&self.redis, &tracker_key, &key).await?;
            redis::set_add(&self.redis, &self.tracker_registry_key(), &tracker_key).await?;
        }

        let mut memory_cache = self.memory_cache.lock().await;
        memory_cache.insert(key, messages);

        Ok(())
    }

    pub async fn append_messages(
        &self,
        sender_id: &str,
        incoming_messages: Vec<LLMMessage>,
    ) -> Result<()> {
        let default_bucket_name = self.default_bucket_name().await;
        let bucket_name = {
            let mut sender_bucket_map = self.sender_bucket_map.lock().await;
            sender_bucket_map
                .entry(sender_id.to_string())
                .or_insert(default_bucket_name)
                .clone()
        };
        let key = self.storage_key(&bucket_name, sender_id);

        if self.redis.cached_redis_url.lock().await.is_some() {
            let existing_json = redis::get_value(&self.redis, &key).await?;
            let mut existing_messages: Vec<LLMMessage> = existing_json
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_default();
            existing_messages.extend(incoming_messages.clone());

            let serialized = serde_json::to_string(&existing_messages)?;
            redis::set_value(&self.redis, &key, &serialized).await?;
            let tracker_key = self.tracker_key(&bucket_name);
            redis::set_add(&self.redis, &tracker_key, &key).await?;
            redis::set_add(&self.redis, &self.tracker_registry_key(), &tracker_key).await?;
        }

        let mut memory_cache = self.memory_cache.lock().await;
        let entry = memory_cache.entry(key).or_default();
        entry.extend(incoming_messages);

        Ok(())
    }
}

impl std::fmt::Debug for LLMMessageSessionCacheRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LLMMessageSessionCacheRef")
            .field("node_id", &self.node_id)
            .field("memory_cache", &"<TokioMutex<HashMap<...>>>")
            .field("redis", &self.redis)
            .field("sender_bucket_map", &"<TokioMutex<HashMap<...>>>")
            .field("default_bucket_name", &"<TokioMutex<String>>")
            .finish()
    }
}
