pub mod ddl;

use sqlx::mysql::MySqlConnection;
use sqlx::sqlite::SqliteConnection;
use sqlx::Row;

use crate::error::{Error, Result};

/// Execute DDL and index creation statements against a MySQL connection.
pub async fn ensure_tables_mysql(conn: &mut MySqlConnection) -> Result<()> {
    for (ddl, indexes) in ddl::MYSQL_TABLES {
        sqlx::query(ddl).execute(&mut *conn).await.map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "MySQL DDL failed: {} — statement: {}",
                e,
                &ddl[..ddl.len().min(120)]
            )))
        })?;
        for idx in *indexes {
            if let Err(e) = sqlx::query(idx).execute(&mut *conn).await {
                let msg = e.to_string();
                if msg.contains("Duplicate key name") || msg.contains("1061") {
                    log::debug!("MySQL index already exists, skipping: {}", &idx[..idx.len().min(120)]);
                    continue;
                }
                return Err(Error::Database(sqlx::Error::Protocol(format!(
                    "MySQL index creation failed: {} — statement: {}",
                    msg, idx
                ))));
            }
        }
    }
    ensure_privilege_auth_columns_mysql(conn).await?;
    ensure_message_rate_limit_schema_mysql(conn).await?;
    Ok(())
}

/// Execute DDL and index creation statements against a SQLite connection.
///
/// Enables `PRAGMA foreign_keys = ON` before creating tables so that
/// FOREIGN KEY constraints are enforced.
pub async fn ensure_tables_sqlite(conn: &mut SqliteConnection) -> Result<()> {
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *conn)
        .await
        .map_err(|e| Error::Database(sqlx::Error::Protocol(format!("SQLite PRAGMA failed: {}", e))))?;

    for (ddl, indexes) in ddl::SQLITE_TABLES {
        sqlx::query(ddl).execute(&mut *conn).await.map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "SQLite DDL failed: {} — statement: {}",
                e,
                &ddl[..ddl.len().min(120)]
            )))
        })?;
        for idx in *indexes {
            sqlx::query(idx).execute(&mut *conn).await.map_err(|e| {
                Error::Database(sqlx::Error::Protocol(format!(
                    "SQLite index creation failed: {} — statement: {}",
                    e, idx
                )))
            })?;
        }
    }
    ensure_privilege_auth_columns_sqlite(conn).await?;
    ensure_message_rate_limit_schema_sqlite(conn).await?;
    Ok(())
}

async fn ensure_privilege_auth_columns_mysql(conn: &mut MySqlConnection) -> Result<()> {
    let columns = [
        (
            "pending_task_id",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_task_id VARCHAR(64) NULL",
        ),
        (
            "pending_target_id",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_target_id VARCHAR(128) NULL",
        ),
        (
            "pending_group_id",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_group_id BIGINT NULL",
        ),
        (
            "pending_is_group",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_is_group TINYINT(1) NOT NULL DEFAULT 0",
        ),
        (
            "pending_args_json",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_args_json TEXT NULL",
        ),
    ];

    for (column_name, alter_sql) in columns {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'qq_chat_agent_service_privilege_auth' AND column_name = ?",
        )
        .bind(column_name)
        .fetch_one(&mut *conn)
        .await
        .map_err(Error::Database)?;
        if exists == 0 {
            sqlx::query(alter_sql).execute(&mut *conn).await.map_err(|e| {
                Error::Database(sqlx::Error::Protocol(format!(
                    "MySQL ALTER TABLE failed for column '{}': {} — statement: {}",
                    column_name, e, alter_sql
                )))
            })?;
        }
    }

    Ok(())
}

async fn ensure_privilege_auth_columns_sqlite(conn: &mut SqliteConnection) -> Result<()> {
    let rows = sqlx::query("PRAGMA table_info('qq_chat_agent_service_privilege_auth')")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| Error::Database(sqlx::Error::Protocol(format!("SQLite PRAGMA table_info failed: {}", e))))?;
    let mut existing = std::collections::HashSet::new();
    for row in rows {
        let name: String = row
            .try_get("name")
            .map_err(|e| Error::Database(sqlx::Error::Protocol(format!("SQLite PRAGMA row parse failed: {}", e))))?;
        existing.insert(name);
    }

    let columns = [
        (
            "pending_task_id",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_task_id TEXT NULL",
        ),
        (
            "pending_target_id",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_target_id TEXT NULL",
        ),
        (
            "pending_group_id",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_group_id INTEGER NULL",
        ),
        (
            "pending_is_group",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_is_group INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "pending_args_json",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN pending_args_json TEXT NULL",
        ),
    ];

    for (column_name, alter_sql) in columns {
        if !existing.contains(column_name) {
            sqlx::query(alter_sql).execute(&mut *conn).await.map_err(|e| {
                Error::Database(sqlx::Error::Protocol(format!(
                    "SQLite ALTER TABLE failed for column '{}': {} — statement: {}",
                    column_name, e, alter_sql
                )))
            })?;
        }
    }

    Ok(())
}

/// Migrates the rate-limit bucket table to the per-user keying + `window_size` schema.
///
/// The counter unique index changed from
/// `(agent_id, sender_id, group_id, scope_type, scope_key, window_unit)` to
/// `(agent_id, sender_id, scope_type, scope_key, window_unit, window_size)` — `group_id`
/// dropped (limit now follows the user across groups/private), `window_size` added
/// (configurable N-unit window). Counters are ephemeral usage state, so legacy rows are
/// cleared before recreating the index to avoid upsert conflicts from the old key shape.
async fn ensure_message_rate_limit_schema_mysql(conn: &mut MySqlConnection) -> Result<()> {
    let window_size_exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'qq_chat_agent_service_message_rate_limit' AND column_name = 'window_size'",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(Error::Database)?;
    if window_size_exists == 0 {
        sqlx::query(
            "ALTER TABLE qq_chat_agent_service_message_rate_limit ADD COLUMN window_size BIGINT NOT NULL DEFAULT 1",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "MySQL ALTER TABLE failed for column 'window_size': {}",
                e
            )))
        })?;
    }

    let first_block_reply_sent_exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'qq_chat_agent_service_message_rate_limit' AND column_name = 'first_block_reply_sent'",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(Error::Database)?;
    if first_block_reply_sent_exists == 0 {
        sqlx::query(
            "ALTER TABLE qq_chat_agent_service_message_rate_limit ADD COLUMN first_block_reply_sent TINYINT(1) NOT NULL DEFAULT 0",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "MySQL ALTER TABLE failed for column 'first_block_reply_sent': {}",
                e
            )))
        })?;
    }

    // Clear legacy counters so the new unique index can be created without conflicts.
    let _ = sqlx::query("DELETE FROM qq_chat_agent_service_message_rate_limit")
        .execute(&mut *conn)
        .await;

    let drop_result = sqlx::query(
        "DROP INDEX idx_qq_chat_agent_service_message_rate_limit_bucket ON qq_chat_agent_service_message_rate_limit",
    )
    .execute(&mut *conn)
    .await;
    if let Err(e) = drop_result {
        let msg = e.to_string();
        if !msg.contains("1091") && !msg.contains("ER_CANT_DROP_FIELD_OR_KEY") && !msg.contains("check that") {
            log::debug!("MySQL rate-limit index drop skipped: {}", msg);
        }
    }

    let create_result = sqlx::query(
        "CREATE UNIQUE INDEX idx_qq_chat_agent_service_message_rate_limit_bucket ON qq_chat_agent_service_message_rate_limit (agent_id, sender_id, scope_type, scope_key, window_unit, window_size)",
    )
    .execute(&mut *conn)
    .await;
    if let Err(e) = create_result {
        let msg = e.to_string();
        if !msg.contains("Duplicate key name") && !msg.contains("1061") {
            return Err(Error::Database(sqlx::Error::Protocol(format!(
                "MySQL rate-limit index creation failed: {}",
                msg
            ))));
        }
    }

    Ok(())
}

async fn ensure_message_rate_limit_schema_sqlite(conn: &mut SqliteConnection) -> Result<()> {
    let rows = sqlx::query("PRAGMA table_info('qq_chat_agent_service_message_rate_limit')")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| Error::Database(sqlx::Error::Protocol(format!("SQLite PRAGMA table_info failed: {}", e))))?;
    let mut existing = std::collections::HashSet::new();
    for row in rows {
        let name: String = row
            .try_get("name")
            .map_err(|e| Error::Database(sqlx::Error::Protocol(format!("SQLite PRAGMA row parse failed: {}", e))))?;
        existing.insert(name);
    }

    if !existing.contains("window_size") {
        sqlx::query(
            "ALTER TABLE qq_chat_agent_service_message_rate_limit ADD COLUMN window_size INTEGER NOT NULL DEFAULT 1",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "SQLite ALTER TABLE failed for column 'window_size': {}",
                e
            )))
        })?;
    }

    if !existing.contains("first_block_reply_sent") {
        sqlx::query(
            "ALTER TABLE qq_chat_agent_service_message_rate_limit ADD COLUMN first_block_reply_sent INTEGER NOT NULL DEFAULT 0",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "SQLite ALTER TABLE failed for column 'first_block_reply_sent': {}",
                e
            )))
        })?;
    }

    // Clear legacy counters so the new unique index can be created without conflicts.
    let _ = sqlx::query("DELETE FROM qq_chat_agent_service_message_rate_limit")
        .execute(&mut *conn)
        .await;

    let _ = sqlx::query("DROP INDEX IF EXISTS idx_qq_chat_agent_service_message_rate_limit_bucket")
        .execute(&mut *conn)
        .await;

    sqlx::query(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_qq_chat_agent_service_message_rate_limit_bucket ON qq_chat_agent_service_message_rate_limit (agent_id, sender_id, scope_type, scope_key, window_unit, window_size)",
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| Error::Database(sqlx::Error::Protocol(format!("SQLite rate-limit index creation failed: {}", e))))?;

    Ok(())
}
