use crate::data_value::RedisConfig;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::info;
use redis::aio::ConnectionManager;
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
}

impl RedisNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            redis_cm: Arc::new(TokioMutex::new(None)),
            cached_redis_url: Arc::new(TokioMutex::new(None)),
        }
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

        let config = RedisConfig {
            url: url.clone(),
            reconnect_max_attempts: max_attempts,
            reconnect_interval_secs: interval_secs,
            redis_cm: self.redis_cm.clone(),
            cached_redis_url: self.cached_redis_url.clone(),
        };

        if let Some(ref url) = url {
            let url = url.clone();
            let redis_cm = self.redis_cm.clone();
            let cached_redis_url = self.cached_redis_url.clone();

            let connect = async move {
                let mut cm_guard = redis_cm.lock().await;
                let mut url_guard = cached_redis_url.lock().await;

                if url_guard.as_deref() != Some(url.as_str()) {
                    *cm_guard = None;
                    *url_guard = Some(url.clone());
                }

                if cm_guard.is_none() {
                    let client = redis::Client::open(url.as_str())?;
                    let cm = ConnectionManager::new(client).await?;
                    info!("[RedisNode] Connected to Redis at {}", url);
                    *cm_guard = Some(cm);
                }

                Ok::<(), redis::RedisError>(())
            };

            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                block_in_place(|| handle.block_on(connect))?;
            } else {
                tokio::runtime::Runtime::new()?.block_on(connect)?;
            }
        }

        let mut outputs = HashMap::new();
        outputs.insert(
            "redis_ref".to_string(),
            DataValue::RedisRef(Arc::new(config)),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
