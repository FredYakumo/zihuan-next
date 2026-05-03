use crate::data_value::RedisConfig;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::info;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::task::block_in_place;
use zihuan_core::config::pct_encode;
use zihuan_core::error::Result;

/// Redis configuration node - builds Redis connection config from input ports
pub struct RedisNode {
    id: String,
    name: String,
    redis_cm: Arc<TokioMutex<Option<ConnectionManager>>>,
    cached_redis_url: Arc<TokioMutex<Option<String>>>,
    run_initialized: bool,
}

impl RedisNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            redis_cm: Arc::new(TokioMutex::new(None)),
            cached_redis_url: Arc::new(TokioMutex::new(None)),
            run_initialized: false,
        }
    }

    fn cleanup_patterns() -> [&'static str; 2] {
        ["message_cache:*", "openai_message_session:*"]
    }

    fn initialize_run(&mut self, redis_ref: Option<&Arc<RedisConfig>>) -> Result<()> {
        if self.run_initialized {
            return Ok(());
        }

        let Some(redis_config) = redis_ref else {
            self.run_initialized = true;
            return Ok(());
        };

        let Some(url) = redis_config.url.clone() else {
            self.run_initialized = true;
            return Ok(());
        };

        let redis_cm = self.redis_cm.clone();
        let cached_redis_url = self.cached_redis_url.clone();
        let url_for_task = url.clone();

        let cleanup = async move {
            let mut cm_guard = redis_cm.lock().await;
            let mut url_guard = cached_redis_url.lock().await;

            if url_guard.as_deref() != Some(url_for_task.as_str()) {
                *cm_guard = None;
                *url_guard = Some(url_for_task.clone());
            }

            if cm_guard.is_none() {
                let client = redis::Client::open(url_for_task.as_str())?;
                let cm = ConnectionManager::new(client).await?;
                info!("[RedisNode] Connected to Redis at {}", url_for_task);
                *cm_guard = Some(cm);
            }

            let cm = cm_guard.as_mut().unwrap();
            let mut removed = 0usize;

            for pattern in Self::cleanup_patterns() {
                let keys: Vec<String> = cm.keys(pattern).await?;
                if !keys.is_empty() {
                    removed += keys.len();
                    let _: () = cm.del(keys).await?;
                }
            }

            Ok::<usize, redis::RedisError>(removed)
        };

        let removed = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(cleanup))
        } else {
            tokio::runtime::Runtime::new()?.block_on(cleanup)
        }?;

        if removed > 0 {
            info!(
                "[RedisNode] Cleared {} Redis cache entr{} from previous graph run (node={})",
                removed,
                if removed == 1 { "y" } else { "ies" },
                self.id
            );
        } else {
            info!(
                "[RedisNode] Cleanup ran for new graph run but found no prior Redis cache entries (node={})",
                self.id
            );
        }

        self.run_initialized = true;
        Ok(())
    }
}

impl Node for RedisNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Redis连接配置 - 构建Redis连接URL并输出引用")
    }

    fn on_graph_start(&mut self) -> Result<()> {
        self.run_initialized = false;
        Ok(())
    }

    node_input![
        port! { name = "redis_host", ty = String, desc = "Redis主机地址" },
        port! { name = "redis_port", ty = Integer, desc = "Redis端口号" },
        port! { name = "redis_db", ty = Integer, desc = "Redis数据库编号 (默认: 0)", optional },
        port! { name = "redis_password", ty = String, desc = "Redis密码", optional },
        port! { name = "reconnect_max_attempts", ty = Integer, desc = "最大重连次数 (默认: 3)", optional },
        port! { name = "reconnect_interval_secs", ty = Integer, desc = "重连间隔秒数 (默认: 60)", optional },
    ];

    node_output![port! { name = "redis_ref", ty = RedisRef, desc = "Redis连接配置引用" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        // Extract required parameters
        let host = inputs
            .get("redis_host")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("redis_host is required".to_string())
            })?;

        let port = inputs
            .get("redis_port")
            .and_then(|v| match v {
                DataValue::Integer(i) => Some(*i as u16),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("redis_port is required".to_string())
            })?;

        let db = inputs
            .get("redis_db")
            .and_then(|v| match v {
                DataValue::Integer(i) => Some(*i as u8),
                _ => None,
            })
            .unwrap_or(0);

        let password = inputs.get("redis_password").and_then(|v| match v {
            DataValue::String(s) => Some(s.clone()),
            _ => None,
        });

        // Build URL from components
        let url = if let Some(pw) = password {
            if !pw.is_empty() {
                let enc = pct_encode(&pw);
                Some(format!("redis://:{}@{}:{}/{}", enc, host, port, db))
            } else {
                Some(format!("redis://{}:{}/{}", host, port, db))
            }
        } else {
            Some(format!("redis://{}:{}/{}", host, port, db))
        };

        let max_attempts = inputs.get("reconnect_max_attempts").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u32),
            _ => None,
        });
        let interval_secs = inputs.get("reconnect_interval_secs").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u64),
            _ => None,
        });

        let config = Arc::new(RedisConfig {
            url: url.clone(),
            reconnect_max_attempts: max_attempts,
            reconnect_interval_secs: interval_secs,
            redis_cm: self.redis_cm.clone(),
            cached_redis_url: self.cached_redis_url.clone(),
        });

        self.initialize_run(Some(&config))?;

        let mut outputs = HashMap::new();
        outputs.insert("redis_ref".to_string(), DataValue::RedisRef(config));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
