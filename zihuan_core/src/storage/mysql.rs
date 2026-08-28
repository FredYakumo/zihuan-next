use std::sync::Arc;
use std::time::Duration;

use crate::data_refs::MySqlConfig;
use crate::error::Result;

use crate::storage::{DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS, DEFAULT_MYSQL_MAX_CONNECTIONS};

pub async fn build_mysql_ref(url: &str) -> Result<Arc<MySqlConfig>> {
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(DEFAULT_MYSQL_MAX_CONNECTIONS)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS))
        .connect(url)
        .await?;

    Ok(Arc::new(MySqlConfig {
        url: Some(url.to_string()),
        reconnect_max_attempts: None,
        reconnect_interval_secs: None,
        pool: Some(pool),
        runtime_handle: tokio::runtime::Handle::try_current().ok(),
    }))
}

pub fn get_pool(mysql_ref: &Arc<MySqlConfig>) -> Option<&sqlx::mysql::MySqlPool> {
    mysql_ref.pool.as_ref()
}
