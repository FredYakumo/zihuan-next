use crate::message_restore::cache_message_snapshot;
use crate::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use log::{debug, warn};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::block_in_place;
use zihuan_bot_types::message::Message;
use zihuan_core::error::Result;

/// Message Cache Node — caches a MessageEvent to Redis (with optional TTL) or falls back to
/// an in-process memory store when Redis is not provided or temporarily unavailable.
///
/// Logical cache key format:
///   Friend message : `friend_{sender_id}_{message_id}`
///   Group  message : `group_{group_id}_{sender_id}_{message_id}`
///
/// When `bucket_name` is provided, the Redis storage key is namespaced as:
///   `message_cache:{bucket_name}:{logical_key}`
/// If omitted, bucket_name defaults to `default`.
///
/// The Redis ConnectionManager is created and owned by `RedisNode`; this node reuses the
/// shared manager exposed through `redis_ref` instead of opening its own connections.
pub struct MessageCacheNode {
    id: String,
    name: String,
    /// In-process fallback cache: cache-key → serialised JSON value.
    memory_cache: Arc<TokioMutex<HashMap<String, String>>>,
    /// Guard to ensure run-start cleanup happens once per graph execution.
    run_initialized: bool,
}

impl MessageCacheNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            memory_cache: Arc::new(TokioMutex::new(HashMap::new())),
            run_initialized: false,
        }
    }

    fn redis_bucket_tracker_key(&self, bucket_name: &str) -> String {
        format!("message_cache:{}:bucket:{}:keys", self.id, bucket_name)
    }

    fn normalize_bucket_name(bucket_name: Option<&str>) -> String {
        let bucket_name = bucket_name.unwrap_or("default").trim();
        if bucket_name.is_empty() {
            "default".to_string()
        } else {
            bucket_name.to_string()
        }
    }

    fn storage_key(bucket_name: &str, logical_key: &str) -> String {
        format!("message_cache:{}:{}", bucket_name, logical_key)
    }

    fn initialize_run(
        &mut self,
        redis_ref: Option<&Arc<crate::data_value::RedisConfig>>,
    ) -> Result<()> {
        if self.run_initialized {
            return Ok(());
        }

        let memory_cache = self.memory_cache.clone();
        let clear_memory = async move {
            memory_cache.lock().await.clear();
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(clear_memory));
        } else {
            tokio::runtime::Runtime::new()?.block_on(clear_memory);
        }

        debug!(
            "[MessageCacheNode] Cleared in-memory cache for new graph run (node={})",
            self.id
        );

        self.run_initialized = true;
        Ok(())
    }
}

impl Node for MessageCacheNode {
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("消息缓存 - 将MessageEvent缓存到内存或Redis（支持TTL过期时间）")
    }

    fn on_graph_start(&mut self) -> Result<()> {
        self.run_initialized = false;
        Ok(())
    }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "消息事件" },
        port! { name = "redis_ref",     ty = RedisRef,     desc = "可选：Redis连接配置引用（若不提供则使用内存缓存）", optional },
        port! { name = "bucket_name",   ty = String,       desc = "可选：Redis存储桶/命名空间名称，默认 default", optional },
        port! { name = "ttl_secs",      ty = Integer,      desc = "可选：Redis缓存过期时间（秒），不设置则永久缓存", optional },
    ];

    node_output![
        port! { name = "success",       ty = Boolean,      desc = "消息是否缓存成功" },
        port! { name = "message_event", ty = MessageEvent, desc = "传递输入的消息事件" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let message_event = inputs
            .get("message_event")
            .and_then(|v| match v {
                DataValue::MessageEvent(e) => Some(e.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("message_event is required".to_string())
            })?;

        let redis_ref = inputs.get("redis_ref").and_then(|v| match v {
            DataValue::RedisRef(r) => Some(r.clone()),
            _ => None,
        });

        let ttl_secs = inputs.get("ttl_secs").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u64),
            _ => None,
        });

        let bucket_name = inputs
            .get("bucket_name")
            .and_then(|v| match v {
                DataValue::String(s) => Some(Self::normalize_bucket_name(Some(s.as_str()))),
                _ => None,
            })
            .unwrap_or_else(|| Self::normalize_bucket_name(None));

        self.initialize_run(redis_ref.as_ref())?;
        cache_message_snapshot(&message_event);

        // ── Build cache key ──────────────────────────────────────────────────────────
        let sender_id = message_event.sender.user_id.to_string();
        let message_id = message_event.message_id.to_string();
        let logical_key = if let Some(gid) = message_event.group_id {
            format!("group_{}_{}_{}", gid, sender_id, message_id)
        } else {
            format!("friend_{}_{}", sender_id, message_id)
        };
        let cache_key = Self::storage_key(&bucket_name, &logical_key);

        // ── Build serialised cache value ─────────────────────────────────────────────
        let content: String = message_event
            .message_list
            .iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join("");

        let at_targets: Vec<String> = message_event
            .message_list
            .iter()
            .filter_map(|m| {
                if let Message::At(at) = m {
                    Some(at.target_id())
                } else {
                    None
                }
            })
            .collect();

        let cache_value = serde_json::json!({
            "message_id":      message_event.message_id,
            "message_type":    message_event.message_type.as_str(),
            "sender": {
                "user_id":  message_event.sender.user_id,
                "nickname": message_event.sender.nickname,
                "card":     message_event.sender.card,
                "role":     message_event.sender.role,
            },
            "group_id":        message_event.group_id,
            "group_name":      message_event.group_name,
            "is_group_message":message_event.is_group_message,
            "content":         content,
            "at_targets":      at_targets,
        })
        .to_string();

        info!(
            "[MessageCacheNode] Caching message {} (bucket={}, key={})",
            message_id, bucket_name, cache_key
        );

        // ── Attempt Redis caching ────────────────────────────────────────────────────
        let mut success = false;

        if let Some(ref redis_config) = redis_ref {
            if let Some(ref url) = redis_config.url {
                let url = url.clone();
                let redis_cm = redis_config.redis_cm.clone();
                let cached_url = redis_config.cached_redis_url.clone();
                let tracker_key = self.redis_bucket_tracker_key(&bucket_name);
                let key = cache_key.clone();
                let value = cache_value.clone();

                let run = async move {
                    let mut cm_guard = redis_cm.lock().await;
                    let mut url_guard = cached_url.lock().await;

                    // Re-create connection manager when URL changes or on first call.
                    if url_guard.as_deref() != Some(url.as_str()) {
                        *cm_guard = None;
                        *url_guard = Some(url.clone());
                    }

                    if cm_guard.is_none() {
                        let client = redis::Client::open(url.as_str())?;
                        match ConnectionManager::new(client).await {
                            Ok(cm) => {
                                info!("[MessageCacheNode] Connected to Redis at {}", url);
                                *cm_guard = Some(cm);
                            }
                            Err(e) => return Err(e),
                        }
                    }

                    let cm = cm_guard.as_mut().unwrap();
                    if let Some(ttl) = ttl_secs {
                        cm.set_ex::<_, _, ()>(&key, value, ttl).await?;
                    } else {
                        cm.set::<_, _, ()>(&key, value).await?;
                    }

                    cm.sadd::<_, _, ()>(&tracker_key, &key).await?;
                    Ok::<(), redis::RedisError>(())
                };

                let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    block_in_place(|| handle.block_on(run))
                } else {
                    tokio::runtime::Runtime::new()?.block_on(run)
                };

                match result {
                    Ok(_) => {
                        info!(
                            "[MessageCacheNode] Cached message {} in Redis \
                             (bucket={}, key={}, ttl={:?}s)",
                            message_id, bucket_name, cache_key, ttl_secs
                        );
                        success = true;
                    }
                    Err(e) => {
                        warn!(
                            "[MessageCacheNode] Redis cache failed for message {} \
                             (bucket={}, key={}): {} — falling back to memory",
                            message_id, bucket_name, cache_key, e
                        );
                    }
                }
            } else {
                warn!(
                    "[MessageCacheNode] redis_ref provided but has no URL \
                     — falling back to memory cache"
                );
            }
        }

        // ── Memory fallback ──────────────────────────────────────────────────────────
        if !success {
            let memory_cache = self.memory_cache.clone();
            let key = cache_key.clone();
            let value = cache_value.clone();

            let run = async move {
                memory_cache.lock().await.insert(key, value);
            };

            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(run));
            } else {
                tokio::runtime::Runtime::new()?.block_on(run);
            }

            debug!(
                "[MessageCacheNode] Cached message {} in memory (bucket={}, key={})",
                message_id, bucket_name, cache_key
            );
            success = true;
        }

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert(
            "message_event".to_string(),
            DataValue::MessageEvent(message_event),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
