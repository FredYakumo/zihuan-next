use sqlx::mysql::MySqlPool;
use sqlx::sqlite::SqlitePool;
use std::fmt;
use std::sync::Arc;

#[derive(Clone)]
pub struct MySqlConfig {
    pub url: Option<String>,
    pub reconnect_max_attempts: Option<u32>,
    pub reconnect_interval_secs: Option<u64>,
    pub pool: Option<MySqlPool>,
    pub runtime_handle: Option<tokio::runtime::Handle>,
}

impl fmt::Debug for MySqlConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MySqlConfig")
            .field("url", &self.url)
            .field("reconnect_max_attempts", &self.reconnect_max_attempts)
            .field("reconnect_interval_secs", &self.reconnect_interval_secs)
            .field("pool", &self.pool.as_ref().map(|_| "<MySqlPool>"))
            .field(
                "runtime_handle",
                &self.runtime_handle.as_ref().map(|_| "<Handle>"),
            )
            .finish()
    }
}

#[derive(Clone)]
pub struct SqliteConfig {
    pub path: String,
    pub pool: Option<SqlitePool>,
    pub runtime_handle: Option<tokio::runtime::Handle>,
}

impl fmt::Debug for SqliteConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqliteConfig")
            .field("path", &self.path)
            .field("pool", &self.pool.as_ref().map(|_| "<SqlitePool>"))
            .field(
                "runtime_handle",
                &self.runtime_handle.as_ref().map(|_| "<Handle>"),
            )
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum RelationalDbConnection {
    MySql(Arc<MySqlConfig>),
    Sqlite(Arc<SqliteConfig>),
}
