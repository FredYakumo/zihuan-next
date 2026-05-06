use std::sync::Arc;

use sqlx::mysql::MySqlPool;
use zihuan_core::error::Result;
use zihuan_graph_engine::data_value::{MySqlConfig, RedisConfig};

pub struct ConnectionManager {
    mysql_ref: Arc<MySqlConfig>,
    redis_ref: Option<Arc<RedisConfig>>,
}

impl std::fmt::Debug for ConnectionManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionManager")
            .field("mysql_ref", &self.mysql_ref)
            .field("redis_ref", &self.redis_ref)
            .finish()
    }
}

impl ConnectionManager {
    pub fn new(mysql_ref: Arc<MySqlConfig>, redis_ref: Option<Arc<RedisConfig>>) -> Self {
        Self {
            mysql_ref,
            redis_ref,
        }
    }

    pub fn mysql_ref(&self) -> &Arc<MySqlConfig> {
        &self.mysql_ref
    }

    pub fn redis_ref(&self) -> Option<&Arc<RedisConfig>> {
        self.redis_ref.as_ref()
    }

    pub fn mysql_pool(&self) -> Option<&MySqlPool> {
        crate::mysql::get_pool(&self.mysql_ref)
    }

    pub async fn set_redis_value(&self, key: &str, value: &str) -> Result<()> {
        let redis_ref = self
            .redis_ref
            .as_ref()
            .ok_or_else(|| zihuan_core::string_error!("redis_ref not configured"))?;
        crate::redis::set_value(redis_ref, key, value).await
    }

    pub async fn get_redis_value(&self, key: &str) -> Result<Option<String>> {
        let Some(redis_ref) = self.redis_ref.as_ref() else {
            return Ok(None);
        };
        crate::redis::get_value(redis_ref, key).await
    }
}
