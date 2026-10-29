//! Isolated relational mock using the production SQLite schema and SQL stores.
use crate::data_refs::SqliteConfig;
use crate::error::Result;
use std::sync::Arc;

pub(super) async fn connect() -> Result<Arc<SqliteConfig>> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect("sqlite::memory:")
        .await?;
    crate::database::ensure_tables_sqlite(&mut *pool.acquire().await?).await?;
    Ok(Arc::new(SqliteConfig {
        path: ":memory:".to_string(),
        pool: Some(pool),
        runtime_handle: Some(tokio::runtime::Handle::current()),
    }))
}
