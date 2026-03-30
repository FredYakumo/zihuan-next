use crate::error::Result;
use crate::node::data_value::{OpenAIMessageSessionCacheRef, RedisConfig};
use crate::node::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use log::{debug, info, warn};
use redis::{aio::ConnectionManager, AsyncCommands};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;

pub struct OpenAIMessageSessionCacheProviderNode {
    id: String,
    name: String,
    cache_ref: Arc<OpenAIMessageSessionCacheRef>,
    run_initialized: bool,
}

impl OpenAIMessageSessionCacheProviderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        let id = id.into();

        Self {
            id: id.clone(),
            name: name.into(),
            cache_ref: Arc::new(OpenAIMessageSessionCacheRef::new(id)),
            run_initialized: false,
        }
    }

    fn redis_tracker_registry_key(&self) -> String {
        format!("openai_message_session:{}:tracker_sets", self.id)
    }

    fn normalize_bucket_name(bucket_name: Option<&str>) -> String {
        let bucket_name = bucket_name.unwrap_or("default").trim();
        if bucket_name.is_empty() {
            "default".to_string()
        } else {
            bucket_name.to_string()
        }
    }

    fn initialize_run(&mut self, redis_ref: Option<&Arc<RedisConfig>>) -> Result<()> {
        if self.run_initialized {
            return Ok(());
        }

        let cache_ref = self.cache_ref.clone();
        let clear_memory = async move {
            cache_ref.memory_cache.lock().await.clear();
            cache_ref.sender_bucket_map.lock().await.clear();
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(clear_memory));
        } else {
            tokio::runtime::Runtime::new()?.block_on(clear_memory);
        }

        debug!(
            "[OpenAIMessageSessionCacheProviderNode] Cleared in-memory cache for new graph run (node={})",
            self.id
        );

        if let Some(redis_config) = redis_ref {
            if let Some(url) = redis_config.url.as_deref() {
                let url = url.to_string();
                let cache_ref = self.cache_ref.clone();
                let tracker_registry_key = self.redis_tracker_registry_key();

                let cleanup = async move {
                    let mut cm_guard = cache_ref.redis_cm.lock().await;
                    let mut url_guard = cache_ref.cached_redis_url.lock().await;

                    if url_guard.as_deref() != Some(url.as_str()) {
                        *cm_guard = None;
                        *url_guard = Some(url.clone());
                    }

                    if cm_guard.is_none() {
                        let client = redis::Client::open(url.as_str())?;
                        match ConnectionManager::new(client).await {
                            Ok(cm) => {
                                info!(
                                    "[OpenAIMessageSessionCacheProviderNode] Connected to Redis at {}",
                                    url
                                );
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
                                "[OpenAIMessageSessionCacheProviderNode] Cleared {} Redis cache entr{} from previous graph run (node={})",
                                removed,
                                if removed == 1 { "y" } else { "ies" },
                                self.id
                            );
                        } else {
                            debug!(
                                "[OpenAIMessageSessionCacheProviderNode] No prior Redis cache entries to clear for node {}",
                                self.id
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            "[OpenAIMessageSessionCacheProviderNode] Failed to clear previous Redis cache for node {}: {}",
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

impl Node for OpenAIMessageSessionCacheProviderNode {
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
        Some("创建单次节点图运行期的 OpenAIMessage 会话暂存引用，支持 Redis 或内存回退")
    }

    fn on_graph_start(&mut self) -> Result<()> {
        self.run_initialized = false;
        Ok(())
    }

    node_input![
        port! { name = "redis_ref", ty = RedisRef, desc = "可选：Redis 连接配置引用（若不提供则使用内存缓存）", optional },
        port! { name = "bucket_name", ty = String, desc = "可选：缓存桶/命名空间名称，默认 default", optional },
    ];

    node_output![
        port! { name = "cache_ref", ty = OpenAIMessageSessionCacheRef, desc = "当前运行期的会话缓存引用，可供后续暂存/读取/覆写节点复用" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let redis_ref = inputs.get("redis_ref").and_then(|v| match v {
            DataValue::RedisRef(r) => Some(r.clone()),
            _ => None,
        });

        let bucket_name = inputs
            .get("bucket_name")
            .and_then(|v| match v {
                DataValue::String(s) => Some(Self::normalize_bucket_name(Some(s.as_str()))),
                _ => None,
            });

        self.initialize_run(redis_ref.as_ref())?;

        let cache_ref = self.cache_ref.clone();
        let bucket_name_for_task = bucket_name.clone();
        let configure_ref = async move {
            cache_ref
                .set_default_bucket_name(bucket_name_for_task.as_deref())
                .await;
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(configure_ref));
        } else {
            tokio::runtime::Runtime::new()?.block_on(configure_ref);
        }

        let mut outputs = HashMap::new();
        outputs.insert(
            "cache_ref".to_string(),
            DataValue::OpenAIMessageSessionCacheRef(self.cache_ref.clone()),
        );

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
