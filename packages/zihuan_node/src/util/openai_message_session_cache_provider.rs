use crate::data_value::{OpenAIMessageSessionCacheRef, RedisConfig};
use crate::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use log::debug;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;
use zihuan_core::error::Result;

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

    fn normalize_bucket_name(bucket_name: Option<&str>) -> String {
        let bucket_name = bucket_name.unwrap_or("default").trim();
        if bucket_name.is_empty() {
            "default".to_string()
        } else {
            bucket_name.to_string()
        }
    }

    fn initialize_run(&mut self, _redis_ref: Option<&Arc<RedisConfig>>) -> Result<()> {
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

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let redis_ref = inputs.get("redis_ref").and_then(|v| match v {
            DataValue::RedisRef(r) => Some(r.clone()),
            _ => None,
        });

        let bucket_name = inputs.get("bucket_name").and_then(|v| match v {
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
