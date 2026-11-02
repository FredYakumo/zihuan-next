//! Schema and data migrations for databases created by earlier releases.
//!
//! Each functional migration lives in its own submodule; [`run_mysql`] / [`run_sqlite`]
//! run the schema-ensure-time pipeline after the base DDL in [`super::ddl`] created
//! missing tables. Every step checks what already exists, so fresh databases skip
//! through cheaply and partially-migrated ones converge.
//!
//! [`dream_memory`] is a runtime migration instead: it needs the caller's memory store
//! to receive each row, so the role service drives it on start rather than the
//! schema-ensure path.

mod context_turn;
mod dream_memory;
mod privilege_auth;
mod rate_limit;
mod role_avatar;

pub use dream_memory::migrate_legacy_dream_memory;

use sqlx::mysql::MySqlConnection;
use sqlx::sqlite::SqliteConnection;

use crate::error::Result;

/// Runs every MySQL schema/data migration, in order, after the base DDL is ensured.
pub(crate) async fn run_mysql(conn: &mut MySqlConnection) -> Result<()> {
    privilege_auth::ensure_columns_mysql(conn).await?;
    sqlx::query("INSERT IGNORE INTO user_permission (agent_id,user_id,elevated_level,elevated_until) SELECT agent_id,sender_id,1,MAX(elevated_until) FROM qq_chat_agent_service_privilege_auth WHERE elevated_until IS NOT NULL GROUP BY agent_id,sender_id").execute(&mut *conn).await?;
    rate_limit::ensure_schema_mysql(conn).await?;
    context_turn::ensure_columns_mysql(conn).await?;
    role_avatar::migrate_table_mysql(conn).await?;
    Ok(())
}

/// Runs every SQLite schema/data migration, in order, after the base DDL is ensured.
pub(crate) async fn run_sqlite(conn: &mut SqliteConnection) -> Result<()> {
    privilege_auth::ensure_columns_sqlite(conn).await?;
    sqlx::query("INSERT OR IGNORE INTO user_permission (agent_id,user_id,elevated_level,elevated_until) SELECT agent_id,sender_id,1,MAX(elevated_until) FROM qq_chat_agent_service_privilege_auth WHERE elevated_until IS NOT NULL GROUP BY agent_id,sender_id").execute(&mut *conn).await?;
    rate_limit::ensure_schema_sqlite(conn).await?;
    context_turn::ensure_columns_sqlite(conn).await?;
    role_avatar::migrate_table_sqlite(conn).await?;
    Ok(())
}
