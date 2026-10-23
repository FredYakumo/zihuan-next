//! One-time migration for the retired `dream_memory` table, driven at role service
//! start because each row must be handed to the caller's memory store.

use sqlx::Row;

use crate::data_refs::RelationalDbConnection;
use crate::error::{Error, Result};

/// Hands every stored memory of `agent_id` to `on_row` as `(sender_id, content)` and
/// deletes the migrated rows, dropping the table once it is empty. Best-effort by
/// design — when `on_row` fails, the remaining rows stay in place for the next start
/// to retry.
pub async fn migrate_legacy_dream_memory(
    connection: &RelationalDbConnection,
    agent_id: &str,
    mut on_row: impl FnMut(&str, &str) -> Result<()>,
) -> Result<usize> {
    let present: i64 = match connection {
        RelationalDbConnection::MySql(config) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = 'dream_memory'",
            )
            .fetch_one(config.pool.as_ref().ok_or_else(pool_missing)?)
            .await
            .map_err(Error::Database)?
        }
        RelationalDbConnection::Sqlite(config) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'dream_memory'")
                .fetch_one(config.pool.as_ref().ok_or_else(pool_missing)?)
                .await
                .map_err(Error::Database)?
        }
    };
    if present == 0 {
        return Ok(0);
    }
    let rows: Vec<(String, String, String)> = match connection {
        RelationalDbConnection::MySql(config) => sqlx::query(
            "SELECT id, sender_id, memory_content FROM dream_memory WHERE agent_id = ? ORDER BY created_at ASC",
        )
        .bind(agent_id)
        .fetch_all(config.pool.as_ref().ok_or_else(pool_missing)?)
        .await
        .map_err(Error::Database)?
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get("id").map_err(Error::Database)?,
                row.try_get("sender_id").map_err(Error::Database)?,
                row.try_get("memory_content").map_err(Error::Database)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?,
        RelationalDbConnection::Sqlite(config) => sqlx::query(
            "SELECT id, sender_id, memory_content FROM dream_memory WHERE agent_id = ? ORDER BY created_at ASC",
        )
        .bind(agent_id)
        .fetch_all(config.pool.as_ref().ok_or_else(pool_missing)?)
        .await
        .map_err(Error::Database)?
        .into_iter()
        .map(|row| {
            Ok((
                row.try_get("id").map_err(Error::Database)?,
                row.try_get("sender_id").map_err(Error::Database)?,
                row.try_get("memory_content").map_err(Error::Database)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?,
    };
    let mut migrated = 0;
    for (id, sender_id, content) in rows {
        on_row(&sender_id, &content)?;
        match connection {
            RelationalDbConnection::MySql(config) => {
                sqlx::query("DELETE FROM dream_memory WHERE id = ?")
                    .bind(&id)
                    .execute(config.pool.as_ref().ok_or_else(pool_missing)?)
                    .await
                    .map_err(Error::Database)?;
            }
            RelationalDbConnection::Sqlite(config) => {
                sqlx::query("DELETE FROM dream_memory WHERE id = ?")
                    .bind(&id)
                    .execute(config.pool.as_ref().ok_or_else(pool_missing)?)
                    .await
                    .map_err(Error::Database)?;
            }
        }
        migrated += 1;
    }
    let remaining: i64 = match connection {
        RelationalDbConnection::MySql(config) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM dream_memory")
                .fetch_one(config.pool.as_ref().ok_or_else(pool_missing)?)
                .await
                .map_err(Error::Database)?
        }
        RelationalDbConnection::Sqlite(config) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM dream_memory")
                .fetch_one(config.pool.as_ref().ok_or_else(pool_missing)?)
                .await
                .map_err(Error::Database)?
        }
    };
    if remaining == 0 {
        match connection {
            RelationalDbConnection::MySql(config) => {
                sqlx::query("DROP TABLE IF EXISTS dream_memory")
                    .execute(config.pool.as_ref().ok_or_else(pool_missing)?)
                    .await
                    .map_err(Error::Database)?;
            }
            RelationalDbConnection::Sqlite(config) => {
                sqlx::query("DROP TABLE IF EXISTS dream_memory")
                    .execute(config.pool.as_ref().ok_or_else(pool_missing)?)
                    .await
                    .map_err(Error::Database)?;
            }
        }
    }
    Ok(migrated)
}

fn pool_missing() -> Error {
    crate::string_error!("database pool is unavailable for migration")
}
