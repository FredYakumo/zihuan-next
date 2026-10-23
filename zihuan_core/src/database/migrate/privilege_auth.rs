//! Adds the pending-task / snapshot columns the privilege-auth flow writes.

use sqlx::mysql::MySqlConnection;
use sqlx::sqlite::SqliteConnection;
use sqlx::Row;

use crate::error::{Error, Result};

pub(super) async fn ensure_columns_mysql(conn: &mut MySqlConnection) -> Result<()> {
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
        (
            "snapshot_json",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN snapshot_json TEXT NULL",
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

pub(super) async fn ensure_columns_sqlite(conn: &mut SqliteConnection) -> Result<()> {
    let rows = sqlx::query("PRAGMA table_info('qq_chat_agent_service_privilege_auth')")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "SQLite PRAGMA table_info failed: {}",
                e
            )))
        })?;
    let mut existing = std::collections::HashSet::new();
    for row in rows {
        let name: String = row.try_get("name").map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!("SQLite PRAGMA row parse failed: {}", e)))
        })?;
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
        (
            "snapshot_json",
            "ALTER TABLE qq_chat_agent_service_privilege_auth ADD COLUMN snapshot_json TEXT NULL",
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
