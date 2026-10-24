//! Migrates the legacy `agent_avatar` table to `role_avatar` (avatars belong to role
//! services, so the table and its owner column were renamed). Runs after `role_avatar`
//! is ensured: copies rows written before the rename, then drops the legacy table.

use sqlx::mysql::MySqlConnection;
use sqlx::sqlite::SqliteConnection;

use crate::error::{Error, Result};

pub(super) async fn migrate_table_mysql(conn: &mut MySqlConnection) -> Result<()> {
    let legacy_exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = 'agent_avatar'",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(Error::Database)?;
    if legacy_exists == 0 {
        return Ok(());
    }
    sqlx::query(
        "INSERT IGNORE INTO role_avatar (id, role_id, file_name, mime_type, image_data, created_at, updated_at)
         SELECT id, agent_id, file_name, mime_type, image_data, created_at, updated_at FROM agent_avatar",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        Error::Database(sqlx::Error::Protocol(format!(
            "MySQL agent_avatar -> role_avatar migration failed: {}",
            e
        )))
    })?;
    sqlx::query("DROP TABLE agent_avatar").execute(&mut *conn).await.map_err(|e| {
        Error::Database(sqlx::Error::Protocol(format!(
            "MySQL legacy agent_avatar drop failed: {}",
            e
        )))
    })?;
    log::info!("[db_schema] migrated legacy table agent_avatar -> role_avatar (MySQL)");
    Ok(())
}

pub(super) async fn migrate_table_sqlite(conn: &mut SqliteConnection) -> Result<()> {
    let legacy_exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'agent_avatar'",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(Error::Database)?;
    if legacy_exists == 0 {
        return Ok(());
    }
    sqlx::query(
        "INSERT OR IGNORE INTO role_avatar (id, role_id, file_name, mime_type, image_data, created_at, updated_at)
         SELECT id, agent_id, file_name, mime_type, image_data, created_at, updated_at FROM agent_avatar",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        Error::Database(sqlx::Error::Protocol(format!(
            "SQLite agent_avatar -> role_avatar migration failed: {}",
            e
        )))
    })?;
    sqlx::query("DROP TABLE agent_avatar").execute(&mut *conn).await.map_err(|e| {
        Error::Database(sqlx::Error::Protocol(format!(
            "SQLite legacy agent_avatar drop failed: {}",
            e
        )))
    })?;
    log::info!("[db_schema] migrated legacy table agent_avatar -> role_avatar (SQLite)");
    Ok(())
}
