use std::fmt;
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::url_utils::pct_encode;
use log::{debug, warn};
use redis::aio::Connection;
use redis::AsyncCommands;
use reqwest::Url;
use tokio::sync::Mutex as TokioMutex;

use crate::storage::{find_connection, load_connections, ConnectionKind};

/// Redis connection configuration, passed between nodes as a reference
#[derive(Clone)]
pub struct RedisConfig {
    pub url: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub reconnect_max_attempts: Option<u32>,
    pub reconnect_interval_secs: Option<u64>,
    /// Shared Redis connection pool maintained by the RedisNode.
    pub redis_cm: Arc<TokioMutex<Option<Connection>>>,
    /// Tracks the URL used to build the current pool.
    pub cached_redis_url: Arc<TokioMutex<Option<String>>>,
}

impl RedisConfig {
    pub fn new(
        url: Option<String>,
        username: Option<String>,
        password: Option<String>,
        reconnect_max_attempts: Option<u32>,
        reconnect_interval_secs: Option<u64>,
    ) -> Self {
        Self {
            url,
            username,
            password,
            reconnect_max_attempts,
            reconnect_interval_secs,
            redis_cm: Arc::new(TokioMutex::new(None)),
            cached_redis_url: Arc::new(TokioMutex::new(None)),
        }
    }
}

impl fmt::Debug for RedisConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RedisConfig")
            .field("url", &self.url)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("reconnect_max_attempts", &self.reconnect_max_attempts)
            .field("reconnect_interval_secs", &self.reconnect_interval_secs)
            .field("redis_cm", &"<TokioMutex<Option<Connection>>>")
            .field("cached_redis_url", &"<TokioMutex<Option<String>>>")
            .finish()
    }
}

pub async fn build_redis_ref(url: &str) -> Result<Arc<RedisConfig>> {
    let redis_ref = Arc::new(RedisConfig::new(Some(url.to_string()), None, None, None, None));
    {
        let mut redis_cm = redis_ref.redis_cm.lock().await;
        *redis_cm = Some(connect(url).await?);
        let mut cached_redis_url = redis_ref.cached_redis_url.lock().await;
        *cached_redis_url = Some(url.to_string());
    }
    Ok(redis_ref)
}

/// Run one Redis command on the shared connection, reconnecting once when it fails.
///
/// Every primitive below has the same shape: try once, drop the pooled connection on failure,
/// reconnect, and try once more before reporting the error. Writing that out per command was
/// six copies of the same block, so the shape lives here instead.
macro_rules! redis_with_retry {
    ($label:literal, $key:expr, $redis_ref:expr, |$conn:ident| $op:expr) => {{
        let first_error = {
            let mut redis_cm = $redis_ref.redis_cm.lock().await;
            let $conn = ensure_connection($redis_ref, &mut redis_cm).await?;
            match ($op).await {
                Ok(value) => return Ok(value),
                Err(err) => err,
            }
        };

        invalidate_connection($redis_ref).await;

        let mut redis_cm = $redis_ref.redis_cm.lock().await;
        let $conn = ensure_connection($redis_ref, &mut redis_cm).await?;
        match ($op).await {
            Ok(value) => {
                debug!(
                    "[storage_handler][redis] {} recovered after reconnect for key '{}': {}",
                    $label, $key, first_error
                );
                Ok(value)
            }
            Err(err) => {
                warn!(
                    "[storage_handler][redis] {} failed after reconnect for key '{}': first_error={}, retry_error={}",
                    $label, $key, first_error, err
                );
                Err(Error::from(err))
            }
        }
    }};
}

pub async fn set_value(redis_ref: &Arc<RedisConfig>, key: &str, value: &str) -> Result<()> {
    redis_with_retry!("SET", key, redis_ref, |conn| conn.set::<_, _, ()>(key, value))
}

pub async fn get_value(redis_ref: &Arc<RedisConfig>, key: &str) -> Result<Option<String>> {
    redis_with_retry!("GET", key, redis_ref, |conn| conn.get(key))
}

/// Read a string and delete the key in one round trip, for one-shot migrations.
pub async fn take_value(redis_ref: &Arc<RedisConfig>, key: &str) -> Result<Option<String>> {
    redis_with_retry!("GETDEL", key, redis_ref, |conn| conn.get_del(key))
}

pub async fn del_value(redis_ref: &Arc<RedisConfig>, key: &str) -> Result<i32> {
    redis_with_retry!("DEL", key, redis_ref, |conn| conn.del(key))
}

pub async fn set_add(redis_ref: &Arc<RedisConfig>, key: &str, member: &str) -> Result<()> {
    redis_with_retry!("SADD", key, redis_ref, |conn| conn.sadd::<_, _, ()>(key, member))
}

pub async fn set_members(redis_ref: &Arc<RedisConfig>, key: &str) -> Result<Vec<String>> {
    redis_with_retry!("SMEMBERS", key, redis_ref, |conn| conn.smembers(key))
}

pub async fn set_remove(redis_ref: &Arc<RedisConfig>, key: &str, member: &str) -> Result<()> {
    redis_with_retry!("SREM", key, redis_ref, |conn| conn.srem::<_, _, ()>(key, member))
}

pub async fn rpush_value(redis_ref: &Arc<RedisConfig>, key: &str, value: &str) -> Result<()> {
    redis_with_retry!("RPUSH", key, redis_ref, |conn| conn.rpush::<_, _, ()>(key, value))
}

pub async fn blpop_value(
    redis_ref: &Arc<RedisConfig>,
    key: &str,
    timeout_secs: usize,
) -> Result<Option<(String, String)>> {
    redis_with_retry!("BLPOP", key, redis_ref, |conn| conn.blpop(key, timeout_secs as f64))
}

pub struct RedisBlockingPopConnection {
    redis_ref: Arc<RedisConfig>,
    conn: Option<Connection>,
}

impl RedisBlockingPopConnection {
    pub fn new(redis_ref: Arc<RedisConfig>) -> Self {
        Self { redis_ref, conn: None }
    }

    pub async fn blpop_value(
        &mut self,
        key: &str,
        timeout_secs: usize,
    ) -> Result<Option<(String, String)>> {
        let first_error = {
            let conn = self.ensure_connection().await?;
            match conn.blpop(key, timeout_secs as f64).await {
                Ok(value) => return Ok(value),
                Err(err) => err,
            }
        };

        self.invalidate_connection();

        let conn = self.ensure_connection().await?;
        match conn.blpop(key, timeout_secs as f64).await {
            Ok(value) => {
                debug!(
                    "[storage_handler][redis] BLPOP recovered after reconnect for key '{}': {}",
                    key, first_error
                );
                Ok(value)
            }
            Err(err) => {
                warn!(
                    "[storage_handler][redis] BLPOP failed after reconnect for key '{}': first_error={}, retry_error={}",
                    key, first_error, err
                );
                Err(Error::from(err))
            }
        }
    }

    async fn ensure_connection(&mut self) -> Result<&mut Connection> {
        if self.conn.is_none() {
            let url = self
                .redis_ref
                .url
                .clone()
                .ok_or_else(|| crate::string_error!("redis_ref missing url"))?;
            self.conn = Some(connect(&url).await?);
        }
        self.conn
            .as_mut()
            .ok_or_else(|| crate::string_error!("redis connection unavailable"))
    }

    fn invalidate_connection(&mut self) {
        self.conn = None;
    }
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
        // Two ways a reference learns its URL: a connection config seeds `url`, while a cache
        // pointed at a running service only knows `cached_redis_url`. The cached one is the
        // connection actually in use, so it wins when both are present.
        let url = {
            let cached = redis_ref.cached_redis_url.lock().await.clone();
            cached.or_else(|| redis_ref.url.clone())
        }
        .ok_or_else(|| crate::string_error!("redis_ref missing url"))?;
        *redis_cm = Some(connect(&url).await?);
        *redis_ref.cached_redis_url.lock().await = Some(url);
    }
    redis_cm
        .as_mut()
        .ok_or_else(|| crate::string_error!("redis connection unavailable"))
}

async fn invalidate_connection(redis_ref: &Arc<RedisConfig>) {
    let mut redis_cm = redis_ref.redis_cm.lock().await;
    *redis_cm = None;
}

pub fn build_redis_ref_for_connection(config_id: &str) -> Result<Arc<RedisConfig>> {
    let connections = load_connections()?;
    let connection = find_connection(&connections, config_id)?;
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
    crate::runtime::block_async(build_redis_ref(&build_redis_connection_url(
        &redis.url,
        redis.username.as_deref(),
        redis.password.as_deref(),
    )?))
}

pub fn build_redis_connection_url(
    base_url: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<String> {
    let username = username.map(str::trim).filter(|value| !value.is_empty());
    let password = password.map(str::trim).filter(|value| !value.is_empty());
    if username.is_none() && password.is_none() {
        return Ok(base_url.to_string());
    }

    let mut parsed = Url::parse(base_url).map_err(|err| {
        Error::ValidationError(format!("invalid redis url '{}': {}", base_url, err))
    })?;
    let encoded_username = username.map(pct_encode).unwrap_or_default();
    parsed.set_username(&encoded_username).map_err(|_| {
        Error::ValidationError(format!("failed to apply username to redis url '{}'", base_url))
    })?;
    parsed.set_password(password.map(pct_encode).as_deref()).map_err(|_| {
        Error::ValidationError(format!("failed to apply password to redis url '{}'", base_url))
    })?;
    Ok(parsed.to_string())
}
