use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use log::{info, warn};
use redis::aio::Connection;
use redis::AsyncCommands;
use tokio::sync::Mutex as TokioMutex;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::data_value::RedisConfig;
use zihuan_graph_engine::{DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

use crate::{find_connection, load_connections, ConnectionKind};

const CONNECTION_ID_FIELD: &str = "connection_id";

pub async fn build_redis_ref(url: &str) -> Result<Arc<RedisConfig>> {
    let redis_ref = Arc::new(RedisConfig::new(Some(url.to_string()), None, None));
    {
        let mut redis_cm = redis_ref.redis_cm.lock().await;
        *redis_cm = Some(connect(url).await?);
        let mut cached_redis_url = redis_ref.cached_redis_url.lock().await;
        *cached_redis_url = Some(url.to_string());
    }
    Ok(redis_ref)
}

pub async fn set_value(redis_ref: &Arc<RedisConfig>, key: &str, value: &str) -> Result<()> {
    let mut redis_cm = redis_ref.redis_cm.lock().await;
    let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
    conn.set::<_, _, ()>(key, value).await?;
    Ok(())
}

pub async fn get_value(redis_ref: &Arc<RedisConfig>, key: &str) -> Result<Option<String>> {
    let mut redis_cm = redis_ref.redis_cm.lock().await;
    let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
    conn.get(key).await.map_err(Error::from)
}

async fn connect(url: &str) -> Result<Connection> {
    let client = redis::Client::open(url)?;
    client.get_tokio_connection().await.map_err(Error::from)
}

async fn ensure_connection<'a>(
    redis_ref: &Arc<RedisConfig>,
    redis_cm: &'a mut Option<Connection>,
) -> Result<&'a mut Connection> {
    if redis_cm.is_none() {
        let url = redis_ref
            .url
            .clone()
            .ok_or_else(|| zihuan_core::string_error!("redis_ref missing url"))?;
        *redis_cm = Some(connect(&url).await?);
        let mut cached_redis_url = redis_ref.cached_redis_url.lock().await;
        *cached_redis_url = Some(url.to_string());
    }
    redis_cm
        .as_mut()
        .ok_or_else(|| zihuan_core::string_error!("redis connection unavailable"))
}

pub struct RedisNode {
    id: String,
    name: String,
    redis_cm: Arc<TokioMutex<Option<Connection>>>,
    cached_redis_url: Arc<TokioMutex<Option<String>>>,
    run_initialized: bool,
    connection_id: Option<String>,
}

impl RedisNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            redis_cm: Arc::new(TokioMutex::new(None)),
            cached_redis_url: Arc::new(TokioMutex::new(None)),
            run_initialized: false,
            connection_id: None,
        }
    }

    fn connection_select_field() -> NodeConfigField {
        NodeConfigField::new(
            CONNECTION_ID_FIELD,
            DataType::String,
            NodeConfigWidget::ConnectionSelect,
        )
        .with_connection_kind("redis")
        .with_description("选择系统中的 Redis 连接配置")
    }

    fn cleanup_patterns() -> [&'static str; 2] {
        ["message_cache:*", "openai_message_session:*"]
    }

    fn run_cleanup_once(
        redis_cm: Arc<TokioMutex<Option<Connection>>>,
        cached_redis_url: Arc<TokioMutex<Option<String>>>,
        url: String,
        force_reconnect: bool,
    ) -> Result<usize> {
        let cleanup = async move {
            let mut cm_guard = redis_cm.lock().await;
            let mut url_guard = cached_redis_url.lock().await;

            if force_reconnect {
                *cm_guard = None;
            }

            if url_guard.as_deref() != Some(url.as_str()) {
                *cm_guard = None;
                *url_guard = Some(url.clone());
            }

            if cm_guard.is_none() {
                let client = redis::Client::open(url.as_str())?;
                let cm = client.get_tokio_connection().await?;
                info!("[RedisNode] Connected to Redis at {}", url);
                *cm_guard = Some(cm);
            }

            if let Some(cm) = cm_guard.as_mut() {
                if let Err(err) = redis::cmd("PING").query_async::<_, String>(cm).await {
                    warn!(
                        "[RedisNode] Existing Redis connection became unavailable, reconnecting: {}",
                        err
                    );
                    *cm_guard = None;
                }
            }

            if cm_guard.is_none() {
                let client = redis::Client::open(url.as_str())?;
                let cm = client.get_tokio_connection().await?;
                *cm_guard = Some(cm);
            }

            let cm = cm_guard.as_mut().expect("cm must be initialized");
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

        let execute = || zihuan_core::runtime::block_async(cleanup);
        match catch_unwind(AssertUnwindSafe(execute)) {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(Error::StringError(
                "Redis connection task terminated unexpectedly".to_string(),
            )),
        }
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

        let removed = match Self::run_cleanup_once(
            self.redis_cm.clone(),
            self.cached_redis_url.clone(),
            url.clone(),
            false,
        ) {
            Ok(removed) => removed,
            Err(err) => {
                warn!(
                    "[RedisNode] Existing Redis connection became unhealthy: {}. Reconnecting once.",
                    err
                );
                Self::run_cleanup_once(
                    self.redis_cm.clone(),
                    self.cached_redis_url.clone(),
                    url,
                    true,
                )?
            }
        };

        if removed > 0 {
            info!(
                "[RedisNode] Cleared {} Redis cache entr{} from previous graph run (node={})",
                removed,
                if removed == 1 { "y" } else { "ies" },
                self.id
            );
        }

        self.run_initialized = true;
        Ok(())
    }

    fn selected_url(&self) -> Result<String> {
        let connection_id = self
            .connection_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ValidationError("connection_id is required".to_string()))?;
        let connections = load_connections()?;
        let connection = find_connection(&connections, connection_id)?;
        let ConnectionKind::Redis(redis) = &connection.kind else {
            return Err(Error::ValidationError(format!(
                "connection '{}' is not a redis connection",
                connection.name
            )));
        };
        if !connection.enabled {
            return Err(Error::ValidationError(format!(
                "connection '{}' is disabled",
                connection.name
            )));
        }
        Ok(redis.url.clone())
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
        Some("Redis连接配置 - 从系统连接中选择并输出 RedisRef")
    }

    fn on_graph_start(&mut self) -> Result<()> {
        self.run_initialized = false;
        Ok(())
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![Port::new("redis_ref", DataType::RedisRef).with_description("Redis连接配置引用")]
    }

    fn config_fields(&self) -> Vec<NodeConfigField> {
        vec![Self::connection_select_field()]
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        self.connection_id = inline_values
            .get(CONNECTION_ID_FIELD)
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.clone()),
                _ => None,
            });
        Ok(())
    }

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let url = self.selected_url()?;
        let config = Arc::new(RedisConfig {
            url: Some(url),
            reconnect_max_attempts: None,
            reconnect_interval_secs: None,
            redis_cm: self.redis_cm.clone(),
            cached_redis_url: self.cached_redis_url.clone(),
        });
        self.initialize_run(Some(&config))?;
        Ok(HashMap::from([(
            "redis_ref".to_string(),
            DataValue::RedisRef(config),
        )]))
    }
}
