use crate::data_value::MySqlConfig;
use crate::message_persistence::register_mysql_persistence_ref;
use crate::message_restore::register_mysql_ref;
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::{debug, info, warn};
use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::block_in_place;
use zihuan_core::error::Result;
use zihuan_core::url_utils::pct_encode;

const DEFAULT_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_ACQUIRE_TIMEOUT_SECS: u64 = 30;

/// MySQL node - builds a persistent connection pool from input ports and
/// passes it downstream via MySqlRef. The pool is cached keyed on the
/// connection URL; it is recreated only when the URL (or credentials) change.
pub struct MySqlNode {
    id: String,
    name: String,
    /// Cached pool, reused across graph executions while the URL stays unchanged.
    pool: Option<MySqlPool>,
    /// URL that produced `pool`; used to detect credential / host changes.
    last_url: Option<String>,
    /// Persistent runtime kept alive for SQLx pool background tasks.
    runtime: Option<tokio::runtime::Runtime>,
    /// Handle to the runtime that owns the pool.
    runtime_handle: Option<tokio::runtime::Handle>,
}

impl MySqlNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            pool: None,
            last_url: None,
            runtime: None,
            runtime_handle: None,
        }
    }

    fn ensure_runtime_handle(&mut self) -> Result<tokio::runtime::Handle> {
        if let Some(handle) = &self.runtime_handle {
            return Ok(handle.clone());
        }

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            self.runtime_handle = Some(handle.clone());
            return Ok(handle);
        }

        info!("[MySqlNode] Creating persistent tokio runtime for MySQL pool");
        let runtime = tokio::runtime::Runtime::new()?;
        let handle = runtime.handle().clone();
        self.runtime = Some(runtime);
        self.runtime_handle = Some(handle.clone());
        Ok(handle)
    }

    /// Return the cached pool when the URL is unchanged; otherwise create a
    /// new pool using the supplied options and cache it.
    fn get_or_create_pool(
        &mut self,
        url: &str,
        max_connections: u32,
        acquire_timeout_secs: u64,
    ) -> Result<MySqlPool> {
        if self.last_url.as_deref() == Some(url) {
            if let Some(ref pool) = self.pool {
                let size = pool.size();
                let idle = pool.num_idle();
                let in_use = size - idle as u32;
                debug!(
                    "[MySqlNode] Reusing existing pool (connections: {}/{} max, {} idle, {} in-use)",
                    size, max_connections, idle, in_use
                );
                if idle == 0 && in_use >= max_connections {
                    warn!(
                        "[MySqlNode] All {} connections are in-use — acquire may time out!",
                        max_connections
                    );
                }
                return Ok(pool.clone());
            }
        }

        info!(
            "[MySqlNode] Creating new connection pool (max_connections={}, acquire_timeout={}s)",
            max_connections, acquire_timeout_secs
        );
        let handle = self.ensure_runtime_handle()?;
        let url_str = url.to_string();
        // idle_timeout: close connections idle for 10 min before the server/NAT drops them.
        // max_lifetime: recycle every connection after 30 min regardless of activity.
        // min_connections: keep 1 warm connection so the first INSERT after a quiet
        //   period doesn't need a full TCP handshake.
        // test_before_acquire is intentionally NOT set: it counts against acquire_timeout
        //   and can cause a full 30-second stall when the stale connection is detected.
        //   Instead, failed queries automatically evict the bad connection; the persistence
        //   node retries once immediately on connection errors.
        let pool_opts = MySqlPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(acquire_timeout_secs))
            .idle_timeout(Duration::from_secs(600))
            .max_lifetime(Duration::from_secs(1800));
        let pool = if tokio::runtime::Handle::try_current().is_ok() {
            block_in_place(|| handle.block_on(pool_opts.connect(&url_str)))
        } else {
            handle.block_on(pool_opts.connect(&url_str))
        }
        .map_err(|e| zihuan_core::string_error!("[MySqlNode] Failed to connect to MySQL: {}", e))?;

        info!(
            "[MySqlNode] Pool ready (max_connections={}, min_connections=1, acquire_timeout={}s, \
             idle_timeout=600s, max_lifetime=1800s, initial size={})",
            max_connections,
            acquire_timeout_secs,
            pool.size()
        );
        self.pool = Some(pool.clone());
        self.last_url = Some(url_str);
        Ok(pool)
    }

    fn sanitize_max_connections(raw: Option<&DataValue>) -> u32 {
        match raw {
            Some(DataValue::Integer(value)) if *value > 0 => *value as u32,
            Some(DataValue::Integer(value)) => {
                warn!(
                    "[MySqlNode] Invalid max_connections={} — falling back to default {}",
                    value, DEFAULT_MAX_CONNECTIONS
                );
                DEFAULT_MAX_CONNECTIONS
            }
            _ => DEFAULT_MAX_CONNECTIONS,
        }
    }

    fn sanitize_acquire_timeout_secs(raw: Option<&DataValue>) -> u64 {
        match raw {
            Some(DataValue::Integer(value)) if *value > 0 => *value as u64,
            Some(DataValue::Integer(value)) => {
                warn!(
                    "[MySqlNode] Invalid acquire_timeout_secs={} — falling back to default {}s",
                    value, DEFAULT_ACQUIRE_TIMEOUT_SECS
                );
                DEFAULT_ACQUIRE_TIMEOUT_SECS
            }
            _ => DEFAULT_ACQUIRE_TIMEOUT_SECS,
        }
    }
}

impl Node for MySqlNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("MySQL连接配置 - 构建MySQL连接URL并维持持久连接池")
    }

    node_input![
        port! { name = "mysql_host", ty = String, desc = "MySQL主机地址" },
        port! { name = "mysql_port", ty = Integer, desc = "MySQL端口号" },
        port! { name = "mysql_user", ty = String, desc = "MySQL用户名" },
        port! { name = "mysql_password", ty = String, desc = "MySQL密码" },
        port! { name = "mysql_database", ty = String, desc = "MySQL数据库名" },
        port! { name = "max_connections", ty = Integer, desc = "连接池最大连接数 (默认: 10)", optional },
        port! { name = "acquire_timeout_secs", ty = Integer, desc = "获取连接超时秒数 (默认: 30)", optional },
        port! { name = "reconnect_max_attempts", ty = Integer, desc = "最大重连次数 (默认: 3)", optional },
        port! { name = "reconnect_interval_secs", ty = Integer, desc = "重连间隔秒数 (默认: 60)", optional },
    ];

    node_output![port! { name = "mysql_ref", ty = MySqlRef, desc = "MySQL连接配置引用" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        // Extract required parameters
        let host = inputs
            .get("mysql_host")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("mysql_host is required".to_string())
            })?;

        let port = inputs
            .get("mysql_port")
            .and_then(|v| match v {
                DataValue::Integer(i) => Some(*i as u16),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("mysql_port is required".to_string())
            })?;

        let user = inputs
            .get("mysql_user")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("mysql_user is required".to_string())
            })?;

        let password = inputs
            .get("mysql_password")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput(
                    "mysql_password is required".to_string(),
                )
            })?;

        let database = inputs
            .get("mysql_database")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput(
                    "mysql_database is required".to_string(),
                )
            })?;

        // Build URL from components
        let url = if !password.is_empty() {
            let enc = pct_encode(&password);
            Some(format!(
                "mysql://{}:{}@{}:{}/{}",
                user, enc, host, port, database
            ))
        } else {
            Some(format!("mysql://{}@{}:{}/{}", user, host, port, database))
        };

        let max_connections = Self::sanitize_max_connections(inputs.get("max_connections"));

        let acquire_timeout_secs =
            Self::sanitize_acquire_timeout_secs(inputs.get("acquire_timeout_secs"));

        let max_attempts = inputs.get("reconnect_max_attempts").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u32),
            _ => None,
        });
        let interval_secs = inputs.get("reconnect_interval_secs").and_then(|v| match v {
            DataValue::Integer(i) => Some(*i as u64),
            _ => None,
        });

        let url_str = url.as_ref().map(|s| s.as_str()).unwrap_or("");
        let pool = self.get_or_create_pool(url_str, max_connections, acquire_timeout_secs)?;

        let size = pool.size();
        let idle = pool.num_idle();
        debug!(
            "[MySqlNode] Passing pool to downstream node (size={}, idle={}, in-use={})",
            size,
            idle,
            size.saturating_sub(idle as u32)
        );

        let config = Arc::new(MySqlConfig {
            url,
            reconnect_max_attempts: max_attempts,
            reconnect_interval_secs: interval_secs,
            pool: Some(pool),
            runtime_handle: self.runtime_handle.clone(),
        });
        register_mysql_persistence_ref(config.clone());
        register_mysql_ref(config.clone());

        let mut outputs = HashMap::new();
        outputs.insert("mysql_ref".to_string(), DataValue::MySqlRef(config));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
