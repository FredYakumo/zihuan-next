use std::sync::Arc;

use crate::data_refs::SqliteConfig;

pub fn get_pool(sqlite_ref: &Arc<SqliteConfig>) -> Option<&sqlx::sqlite::SqlitePool> {
    sqlite_ref.pool.as_ref()
}
