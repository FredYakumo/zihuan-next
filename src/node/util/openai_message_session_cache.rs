use crate::error::Result;
use crate::llm::OpenAIMessage;
use crate::node::data_value::{OpenAIMessageSessionCacheRef, RedisConfig};
use crate::node::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use log::{debug, info, warn};
use redis::{aio::ConnectionManager, AsyncCommands};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::block_in_place;

pub struct OpenAIMessageSessionCacheNode {
    id: String,
    name: String,
    cache_ref: Arc<OpenAIMessageSessionCacheRef>,
    run_initialized: bool,
}

impl OpenAIMessageSessionCacheNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let id = id.into();
        let name = name.into();

        Self {
            id: id.clone(),
            name,
            cache_ref: Arc::new(OpenAIMessageSessionCacheRef {
                node_id: id,
                memory_cache: Arc::new(TokioMutex::new(HashMap::new())),
                redis_cm: Arc::new(TokioMutex::new(None)),
                cached_redis_url: Arc::new(TokioMutex::new(None)),
                sender_bucket_map: Arc::new(TokioMutex::new(HashMap::new())),
            }),
            run_initialized: false,
        }
    }

    fn redis_tracker_registry_key(&self) -> String {
        format!("openai_message_session:{}:tracker_sets", self.id)
    }

    fn redis_bucket_tracker_key(&self, bucket_name: &str) -> String {
        format!("openai_message_session:{}:bucket:{}:keys", self.id, bucket_name)
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
            self.id, bucket_name, sender_id
        )
    }

    fn initialize_run(&mut self, redis_ref: Option<&Arc<RedisConfig>>) -> Result<()> {
        if self.run_initialized {
            return Ok(());
        }

        let memory_cache = self.cache_ref.memory_cache.clone();
        let sender_bucket_map = self.cache_ref.sender_bucket_map.clone();
        let clear_memory = async move {
            memory_cache.lock().await.clear();
            sender_bucket_map.lock().await.clear();
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(clear_memory));
        } else {
            tokio::runtime::Runtime::new()?.block_on(clear_memory);
        }

        debug!(
            "[OpenAIMessageSessionCacheNode] Cleared in-memory cache for new graph run (node={})",
            self.id
        );

        if let Some(redis_config) = redis_ref {
            if let Some(url) = redis_config.url.as_deref() {
                let url = url.to_string();
                let redis_cm = self.cache_ref.redis_cm.clone();
                let cached_url = self.cache_ref.cached_redis_url.clone();
                let tracker_registry_key = self.redis_tracker_registry_key();

                let cleanup = async move {
                    let mut cm_guard = redis_cm.lock().await;
                    let mut url_guard = cached_url.lock().await;

                    if url_guard.as_deref() != Some(url.as_str()) {
                        *cm_guard = None;
                        *url_guard = Some(url.clone());
                    }

                    if cm_guard.is_none() {
                        let client = redis::Client::open(url.as_str())?;
                        match ConnectionManager::new(client).await {
                            Ok(cm) => {
                                info!("[OpenAIMessageSessionCacheNode] Connected to Redis at {}", url);
                                *cm_guard = Some(cm);
                            }
                            Err(e) => return Err(e.into()),
                        }
                    }

                    let cm = cm_guard.as_mut().unwrap();
                    let tracker_keys: Vec<String> = cm.smembers(&tracker_registry_key).await?;
                    let mut previous_keys: Vec<String> = Vec::new();

                    for tracker_key in &tracker_keys {
                        let mut sender_keys: Vec<String> = cm.smembers(tracker_key).await?;
                        previous_keys.append(&mut sender_keys);
                    }

                    let removed = previous_keys.len();
                    if !previous_keys.is_empty() {
                        let _: () = cm.del(previous_keys).await?;
                    }

                    if !tracker_keys.is_empty() {
                        let _: () = cm.del(tracker_keys).await?;
                    }

                    let _: () = cm.del(&tracker_registry_key).await?;

                    Ok::<usize, redis::RedisError>(removed)
                };

                let cleanup_result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    block_in_place(|| handle.block_on(cleanup))
                } else {
                    tokio::runtime::Runtime::new()?.block_on(cleanup)
                };

                match cleanup_result {
                    Ok(removed) => {
                        if removed > 0 {
                            info!(
                                "[OpenAIMessageSessionCacheNode] Cleared {} Redis cache entr{} from previous graph run (node={})",
                                removed,
                                if removed == 1 { "y" } else { "ies" },
                                self.id
                            );
                        } else {
                            debug!(
                                "[OpenAIMessageSessionCacheNode] No prior Redis cache entries to clear for node {}",
                                self.id
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            "[OpenAIMessageSessionCacheNode] Failed to clear previous Redis cache for node {}: {}",
                            self.id, e
                        );
                    }
                }
            }
        }

        self.run_initialized = true;
        Ok(())
    }
}

impl Node for OpenAIMessageSessionCacheNode {
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
        Some("按 sender_id 在单次节点图运行内暂存并累积 Vec<OpenAIMessage>，支持 Redis 或内存回退")
    }

    fn on_graph_start(&mut self) -> Result<()> {
        self.run_initialized = false;
        Ok(())
    }

    node_input![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "要暂存并追加到会话缓存中的 Vec<OpenAIMessage>" },
        port! { name = "sender_id", ty = String, desc = "用户唯一标识，用于区分不同会话" },
        port! { name = "redis_ref", ty = RedisRef, desc = "可选：Redis 连接配置引用（若不提供则使用内存缓存）", optional },
        port! { name = "bucket_name", ty = String, desc = "可选：缓存桶/命名空间名称，默认 default", optional },
    ];

    node_output![
        port! { name = "cache_ref", ty = OpenAIMessageSessionCacheRef, desc = "当前会话缓存节点自身的引用，可供后续节点读取本次运行内的会话暂存" },
        port! { name = "success", ty = Boolean, desc = "是否成功写入 Redis 或内存缓存" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let incoming_messages: Vec<OpenAIMessage> = match inputs.get("messages") {
            Some(DataValue::Vec(_, items)) => items
                .iter()
                .filter_map(|item| match item {
                    DataValue::OpenAIMessage(message) => Some(message.clone()),
                    _ => None,
                })
                .collect(),
            _ => {
                return Err(crate::error::Error::ValidationError(
                    "messages must be Vec<OpenAIMessage> type".to_string(),
                ))
            }
        };

        let sender_id = inputs
            .get("sender_id")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("sender_id is required".to_string()))?;

        let redis_ref = inputs.get("redis_ref").and_then(|v| match v {
            DataValue::RedisRef(r) => Some(r.clone()),
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

        {
            let sender_bucket_map = self.cache_ref.sender_bucket_map.clone();
            let sender_id = sender_id.clone();
            let bucket_name = bucket_name.clone();
            let update_bucket_map = async move {
                sender_bucket_map.lock().await.insert(sender_id, bucket_name);
            };

            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(update_bucket_map));
            } else {
                tokio::runtime::Runtime::new()?.block_on(update_bucket_map);
            }
        }

        let cache_key = self.storage_key(&bucket_name, &sender_id);
        info!(
            "[OpenAIMessageSessionCacheNode] Caching {} message(s) for sender {} (bucket={}, key={})",
            incoming_messages.len(),
            sender_id,
            bucket_name,
            cache_key
        );

        let mut success = false;

        if let Some(ref redis_config) = redis_ref {
            if let Some(ref url) = redis_config.url {
                let url = url.clone();
                let redis_cm = self.cache_ref.redis_cm.clone();
                let cached_url = self.cache_ref.cached_redis_url.clone();
                let tracker_key = self.redis_bucket_tracker_key(&bucket_name);
                let tracker_registry_key = self.redis_tracker_registry_key();
                let key = cache_key.clone();
                let incoming = incoming_messages.clone();

                let run = async move {
                    let mut cm_guard = redis_cm.lock().await;
                    let mut url_guard = cached_url.lock().await;

                    if url_guard.as_deref() != Some(url.as_str()) {
                        *cm_guard = None;
                        *url_guard = Some(url.clone());
                    }

                    if cm_guard.is_none() {
                        let client = redis::Client::open(url.as_str())?;
                        match ConnectionManager::new(client).await {
                            Ok(cm) => {
                                info!("[OpenAIMessageSessionCacheNode] Connected to Redis at {}", url);
                                *cm_guard = Some(cm);
                            }
                            Err(e) => return Err(e),
                        }
                    }

                    let cm = cm_guard.as_mut().unwrap();
                    let existing_json: Option<String> = cm.get(&key).await?;
                    let mut existing_messages: Vec<OpenAIMessage> = existing_json
                        .as_deref()
                        .map(serde_json::from_str)
                        .transpose()
                        .map_err(|e| {
                            redis::RedisError::from((
                                redis::ErrorKind::TypeError,
                                "Failed to deserialize cached OpenAIMessage session",
                                e.to_string(),
                            ))
                        })?
                        .unwrap_or_default();
                    existing_messages.extend(incoming);

                    let serialized = serde_json::to_string(&existing_messages).map_err(|e| {
                        redis::RedisError::from((
                            redis::ErrorKind::TypeError,
                            "Failed to serialize OpenAIMessage session",
                            e.to_string(),
                        ))
                    })?;
                    cm.set::<_, _, ()>(&key, serialized).await?;
                    cm.sadd::<_, _, ()>(&tracker_key, &key).await?;
                    cm.sadd::<_, _, ()>(&tracker_registry_key, &tracker_key).await?;

                    Ok::<Vec<OpenAIMessage>, redis::RedisError>(existing_messages)
                };

                let result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    block_in_place(|| handle.block_on(run))
                } else {
                    tokio::runtime::Runtime::new()?.block_on(run)
                };

                match result {
                    Ok(messages) => {
                        let total_messages = messages.len();
                        success = true;
                        info!(
                            "[OpenAIMessageSessionCacheNode] Cached session history in Redis for sender {} (total_messages={})",
                            sender_id,
                            total_messages
                        );
                    }
                    Err(e) => {
                        warn!(
                            "[OpenAIMessageSessionCacheNode] Redis cache failed for sender {} (bucket={}, key={}): {} — falling back to memory",
                            sender_id,
                            bucket_name,
                            cache_key,
                            e
                        );
                    }
                }
            } else {
                warn!(
                    "[OpenAIMessageSessionCacheNode] redis_ref provided but has no URL — falling back to memory cache"
                );
            }
        }

        if !success {
            let memory_cache = self.cache_ref.memory_cache.clone();
            let key = cache_key.clone();
            let incoming = incoming_messages.clone();

            let run = async move {
                let mut cache = memory_cache.lock().await;
                let entry = cache.entry(key).or_default();
                entry.extend(incoming);
                entry.clone()
            };

            let messages = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(run))
            } else {
                tokio::runtime::Runtime::new()?.block_on(run)
            };
            let total_messages = messages.len();

            debug!(
                "[OpenAIMessageSessionCacheNode] Cached session history in memory for sender {} (total_messages={})",
                sender_id,
                total_messages
            );
            success = true;
        }

        let mut outputs = HashMap::new();
        outputs.insert(
            "cache_ref".to_string(),
            DataValue::OpenAIMessageSessionCacheRef(self.cache_ref.clone()),
        );
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::OpenAIMessageSessionCacheNode;
    use crate::error::Result;
    use crate::llm::{MessageRole, OpenAIMessage};
    use crate::node::data_value::OpenAIMessageSessionCacheRef;
    use crate::node::{DataType, DataValue, Node};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn message(role: MessageRole, content: &str) -> OpenAIMessage {
        OpenAIMessage {
            role,
            content: Some(content.to_string()),
            tool_calls: Vec::new(),
        }
    }

    fn input(sender_id: &str, messages: Vec<OpenAIMessage>) -> HashMap<String, DataValue> {
        HashMap::from([
            (
                "messages".to_string(),
                DataValue::Vec(
                    Box::new(DataType::OpenAIMessage),
                    messages.into_iter().map(DataValue::OpenAIMessage).collect(),
                ),
            ),
            (
                "sender_id".to_string(),
                DataValue::String(sender_id.to_string()),
            ),
        ])
    }

    fn extract_cache_ref(outputs: &HashMap<String, DataValue>) -> Arc<OpenAIMessageSessionCacheRef> {
        match outputs.get("cache_ref") {
            Some(DataValue::OpenAIMessageSessionCacheRef(cache_ref)) => cache_ref.clone(),
            other => panic!("unexpected cache_ref output: {:?}", other),
        }
    }

    fn load_contents(cache_ref: &Arc<OpenAIMessageSessionCacheRef>, sender_id: &str) -> Result<Vec<String>> {
        let runtime = tokio::runtime::Runtime::new()?;
        let messages = runtime.block_on(cache_ref.get_messages(sender_id))?;
        Ok(messages
            .into_iter()
            .filter_map(|message| message.content)
            .collect())
    }

    #[test]
    fn accumulates_messages_for_same_sender_within_one_run() -> Result<()> {
        let mut node = OpenAIMessageSessionCacheNode::new("cache", "Cache");

        let first_outputs = node.execute(input(
            "user-1",
            vec![message(MessageRole::User, "你好")],
        ))?;
        let cache_ref = extract_cache_ref(&first_outputs);
        assert_eq!(load_contents(&cache_ref, "user-1")?, vec!["你好"]);

        let second_outputs = node.execute(input(
            "user-1",
            vec![message(MessageRole::Assistant, "你好呀")],
        ))?;
        let second_cache_ref = extract_cache_ref(&second_outputs);
        assert!(Arc::ptr_eq(&cache_ref, &second_cache_ref));
        assert_eq!(load_contents(&second_cache_ref, "user-1")?, vec!["你好", "你好呀"]);

        let third_outputs = node.execute(input(
            "user-2",
            vec![message(MessageRole::User, "另一位用户")],
        ))?;
        let third_cache_ref = extract_cache_ref(&third_outputs);
        assert_eq!(load_contents(&third_cache_ref, "user-2")?, vec!["另一位用户"]);

        Ok(())
    }

    #[test]
    fn clears_history_when_graph_restarts() -> Result<()> {
        let mut node = OpenAIMessageSessionCacheNode::new("cache", "Cache");

        let _ = node.execute(input(
            "user-1",
            vec![message(MessageRole::User, "第一条")],
        ))?;
        let before_reset = node.execute(input(
            "user-1",
            vec![message(MessageRole::Assistant, "第二条")],
        ))?;
        let before_reset_ref = extract_cache_ref(&before_reset);
        assert_eq!(load_contents(&before_reset_ref, "user-1")?, vec!["第一条", "第二条"]);

        node.on_graph_start()?;

        let after_reset = node.execute(input(
            "user-1",
            vec![message(MessageRole::User, "重启后")],
        ))?;
        let after_reset_ref = extract_cache_ref(&after_reset);
        assert_eq!(load_contents(&after_reset_ref, "user-1")?, vec!["重启后"]);

        Ok(())
    }
}