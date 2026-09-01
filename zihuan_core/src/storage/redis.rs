use std::sync::Arc;

use crate::error::{Error, Result};
use crate::graph::data_value::RedisConfig;
use crate::url_utils::pct_encode;
use log::{debug, warn};
use redis::aio::Connection;
use redis::AsyncCommands;
use reqwest::Url;

use crate::storage::{find_connection, load_connections, ConnectionKind};

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

pub async fn set_value(redis_ref: &Arc<RedisConfig>, key: &str, value: &str) -> Result<()> {
    let first_error = {
        let mut redis_cm = redis_ref.redis_cm.lock().await;
        let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
        match conn.set::<_, _, ()>(key, value).await {
            Ok(()) => return Ok(()),
            Err(err) => err,
        }
    };

    invalidate_connection(redis_ref).await;

    let mut redis_cm = redis_ref.redis_cm.lock().await;
    let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
    match conn.set::<_, _, ()>(key, value).await {
        Ok(()) => {
            debug!(
                "[storage_handler][redis] SET recovered after reconnect for key '{}': {}",
                key, first_error
            );
            Ok(())
        }
        Err(err) => {
            warn!(
                "[storage_handler][redis] SET failed after reconnect for key '{}': first_error={}, retry_error={}",
                key, first_error, err
            );
            Err(Error::from(err))
        }
    }
}

pub async fn get_value(redis_ref: &Arc<RedisConfig>, key: &str) -> Result<Option<String>> {
    let first_error = {
        let mut redis_cm = redis_ref.redis_cm.lock().await;
        let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
        match conn.get(key).await {
            Ok(value) => return Ok(value),
            Err(err) => err,
        }
    };

    invalidate_connection(redis_ref).await;

    let mut redis_cm = redis_ref.redis_cm.lock().await;
    let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
    match conn.get(key).await {
        Ok(value) => {
            debug!(
                "[storage_handler][redis] GET recovered after reconnect for key '{}': {}",
                key, first_error
            );
            Ok(value)
        }
        Err(err) => {
            warn!(
                "[storage_handler][redis] GET failed after reconnect for key '{}': first_error={}, retry_error={}",
                key, first_error, err
            );
            Err(Error::from(err))
        }
    }
}

pub async fn rpush_value(redis_ref: &Arc<RedisConfig>, key: &str, value: &str) -> Result<()> {
    let first_error = {
        let mut redis_cm = redis_ref.redis_cm.lock().await;
        let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
        match conn.rpush::<_, _, ()>(key, value).await {
            Ok(()) => return Ok(()),
            Err(err) => err,
        }
    };

    invalidate_connection(redis_ref).await;

    let mut redis_cm = redis_ref.redis_cm.lock().await;
    let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
    match conn.rpush::<_, _, ()>(key, value).await {
        Ok(()) => {
            debug!(
                "[storage_handler][redis] RPUSH recovered after reconnect for key '{}': {}",
                key, first_error
            );
            Ok(())
        }
        Err(err) => {
            warn!(
                "[storage_handler][redis] RPUSH failed after reconnect for key '{}': first_error={}, retry_error={}",
                key, first_error, err
            );
            Err(Error::from(err))
        }
    }
}

pub async fn blpop_value(
    redis_ref: &Arc<RedisConfig>,
    key: &str,
    timeout_secs: usize,
) -> Result<Option<(String, String)>> {
    let first_error = {
        let mut redis_cm = redis_ref.redis_cm.lock().await;
        let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
        match conn.blpop(key, timeout_secs as f64).await {
            Ok(value) => return Ok(value),
            Err(err) => err,
        }
    };

    invalidate_connection(redis_ref).await;

    let mut redis_cm = redis_ref.redis_cm.lock().await;
    let conn = ensure_connection(redis_ref, &mut redis_cm).await?;
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
        let url = redis_ref
            .url
            .clone()
            .ok_or_else(|| crate::string_error!("redis_ref missing url"))?;
        *redis_cm = Some(connect(&url).await?);
        let mut cached_redis_url = redis_ref.cached_redis_url.lock().await;
        *cached_redis_url = Some(url.to_string());
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
