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
            .field("runtime_handle", &self.runtime_handle.as_ref().map(|_| "<Handle>"))
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
            .field("runtime_handle", &self.runtime_handle.as_ref().map(|_| "<Handle>"))
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum RelationalDbConnection {
    MySql(Arc<MySqlConfig>),
    Sqlite(Arc<SqliteConfig>),
}

#[async_trait::async_trait]
impl crate::connection::Connection for MySqlConfig {
    async fn connect(&mut self) -> crate::error::Result<()> {
        if self.is_closed() {
            let pool = if let Some(pool) = &self.pool {
                pool.options().clone().connect_with((*pool.connect_options()).clone()).await?
            } else {
                sqlx::mysql::MySqlPoolOptions::new()
                    .max_connections(crate::storage::DEFAULT_MYSQL_MAX_CONNECTIONS)
                    .connect(
                        self.url
                            .as_deref()
                            .ok_or_else(|| crate::string_error!("MySQL URL is missing"))?,
                    )
                    .await?
            };
            self.pool = Some(pool);
            self.runtime_handle = Some(tokio::runtime::Handle::current());
        }
        if let Err(error) = self.health_check().await {
            self.close().await?;
            return Err(error);
        }
        Ok(())
    }

    async fn health_check(&self) -> crate::error::Result<()> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| crate::string_error!("MySQL connection is closed"))?;
        sqlx::query("SELECT 1").execute(pool).await?;
        Ok(())
    }

    async fn close(&self) -> crate::error::Result<()> {
        if let Some(pool) = &self.pool {
            pool.close().await;
        }
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.pool.as_ref().map_or(true, |pool| pool.is_closed())
    }
}

#[async_trait::async_trait]
impl crate::connection::Connection for SqliteConfig {
    async fn connect(&mut self) -> crate::error::Result<()> {
        if self.is_closed() {
            let pool = if let Some(pool) = &self.pool {
                pool.options().clone().connect_with((*pool.connect_options()).clone()).await?
            } else {
                sqlx::sqlite::SqlitePoolOptions::new()
                    .max_connections(4)
                    .connect(&format!("sqlite://{}?mode=rwc", self.path))
                    .await?
            };
            crate::database::ensure_tables_sqlite(&mut *pool.acquire().await?).await?;
            self.pool = Some(pool);
            self.runtime_handle = Some(tokio::runtime::Handle::current());
        }
        if let Err(error) = self.health_check().await {
            self.close().await?;
            return Err(error);
        }
        Ok(())
    }

    async fn health_check(&self) -> crate::error::Result<()> {
        let pool = self
            .pool
            .as_ref()
            .ok_or_else(|| crate::string_error!("SQLite connection is closed"))?;
        sqlx::query("SELECT 1").execute(pool).await?;
        Ok(())
    }

    async fn close(&self) -> crate::error::Result<()> {
        if let Some(pool) = &self.pool {
            pool.close().await;
        }
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.pool.as_ref().map_or(true, |pool| pool.is_closed())
    }
}

impl MySqlConfig {
    pub async fn from_config(
        config: &crate::storage::MysqlConnection,
    ) -> crate::error::Result<Self> {
        use crate::connection::Connection;
        let max_connections = if config.max_connections == 0 {
            crate::storage::DEFAULT_MYSQL_MAX_CONNECTIONS
        } else {
            config.max_connections
        };
        let timeout = if config.acquire_timeout_secs == 0 {
            crate::storage::DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS
        } else {
            config.acquire_timeout_secs
        };
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(timeout))
            .connect_lazy(&config.url)?;
        let mut connection = Self {
            url: Some(config.url.clone()),
            reconnect_max_attempts: None,
            reconnect_interval_secs: None,
            pool: Some(pool),
            runtime_handle: Some(tokio::runtime::Handle::current()),
        };
        connection.connect().await?;
        if let Err(error) = crate::database::ensure_tables_mysql(
            &mut *connection.pool.as_ref().unwrap().acquire().await?,
        )
        .await
        {
            log::warn!("[mysql] schema setup failed: {error}");
        }
        Ok(connection)
    }
}

impl SqliteConfig {
    pub async fn from_config(
        config: &crate::storage::SqliteConnection,
    ) -> crate::error::Result<Self> {
        use crate::connection::Connection;
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(4)
            .min_connections(1)
            .connect_lazy(&format!("sqlite://{}?mode=rwc", config.path))?;
        let mut connection = Self {
            path: config.path.clone(),
            pool: Some(pool),
            runtime_handle: Some(tokio::runtime::Handle::current()),
        };
        connection.connect().await?;
        if let Err(error) = crate::database::ensure_tables_sqlite(
            &mut *connection.pool.as_ref().unwrap().acquire().await?,
        )
        .await
        {
            log::warn!("[sqlite] schema setup failed: {error}");
        }
        Ok(connection)
    }
}
