//! Adds columns introduced after the first `qq_chat_context_turn` release.

use sqlx::mysql::MySqlConnection;
use sqlx::sqlite::SqliteConnection;
use sqlx::Row;

use crate::error::{Error, Result};

pub(super) async fn ensure_columns_mysql(conn: &mut MySqlConnection) -> Result<()> {
    let precision: Option<i64> = sqlx::query_scalar("SELECT CAST(DATETIME_PRECISION AS SIGNED) FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'qq_chat_context_turn' AND column_name = 'created_at'")
        .fetch_one(&mut *conn).await?;
    if precision.unwrap_or(0) < 3 {
        sqlx::query(
            "ALTER TABLE qq_chat_context_turn MODIFY COLUMN created_at DATETIME(3) NOT NULL",
        )
        .execute(&mut *conn)
        .await?;
    }
    for (name, definition) in [
        ("group_id", "VARCHAR(64) NULL"),
        ("status", "VARCHAR(32) NOT NULL DEFAULT 'completed'"),
        ("updated_at", "VARCHAR(40) NULL"),
        ("next_step_sequence", "BIGINT NOT NULL DEFAULT 0"),
    ] {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'qq_chat_context_turn' AND column_name = ?")
            .bind(name).fetch_one(&mut *conn).await?;
        if exists == 0 {
            sqlx::query(&format!(
                "ALTER TABLE qq_chat_context_turn ADD COLUMN {name} {definition}"
            ))
            .execute(&mut *conn)
            .await?;
        }
    }
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'qq_chat_context_turn' AND column_name = 'continues_previous'",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(Error::Database)?;
    if exists == 0 {
        sqlx::query(
            "ALTER TABLE qq_chat_context_turn ADD COLUMN continues_previous TINYINT(1) NOT NULL DEFAULT 0",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "MySQL ALTER TABLE failed for column 'continues_previous': {}",
                e
            )))
        })?;
    }
    let has_quoted_json = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM information_schema.columns WHERE table_schema = DATABASE() AND table_name = 'qq_chat_context_turn' AND column_name = 'quoted_json'",
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(Error::Database)?;
    if has_quoted_json == 0 {
        sqlx::query("ALTER TABLE qq_chat_context_turn ADD COLUMN quoted_json MEDIUMTEXT NULL")
            .execute(&mut *conn)
            .await
            .map_err(|e| {
                Error::Database(sqlx::Error::Protocol(format!(
                    "MySQL ALTER TABLE failed for column 'quoted_json': {}",
                    e
                )))
            })?;
    }
    super::super::migration_task::tracked("回填旧上下文会话摘要（MySQL）", async {
        sqlx::query(&crate::storage::context_store::session_backfill_sql(true, false))
            .execute(&mut *conn)
            .await?;
        Ok(())
    })
    .await?;
    Ok(())
}

pub(super) async fn ensure_columns_sqlite(conn: &mut SqliteConnection) -> Result<()> {
    let columns = sqlx::query("PRAGMA table_info('qq_chat_context_turn')")
        .fetch_all(&mut *conn)
        .await?;
    for (name, definition) in [
        ("group_id", "TEXT"),
        ("status", "TEXT NOT NULL DEFAULT 'completed'"),
        ("updated_at", "TEXT"),
        ("next_step_sequence", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !columns.iter().any(|row| row.get::<String, _>("name") == name) {
            sqlx::query(&format!(
                "ALTER TABLE qq_chat_context_turn ADD COLUMN {name} {definition}"
            ))
            .execute(&mut *conn)
            .await?;
        }
    }
    let rows = sqlx::query("PRAGMA table_info('qq_chat_context_turn')")
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "SQLite PRAGMA table_info failed: {}",
                e
            )))
        })?;
    let has_column = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == "continues_previous")
            .unwrap_or(false)
    });
    if !has_column {
        sqlx::query(
            "ALTER TABLE qq_chat_context_turn ADD COLUMN continues_previous INTEGER NOT NULL DEFAULT 0",
        )
        .execute(&mut *conn)
        .await
        .map_err(|e| {
            Error::Database(sqlx::Error::Protocol(format!(
                "SQLite ALTER TABLE failed for column 'continues_previous': {}",
                e
            )))
        })?;
    }
    let has_quoted_json = rows.iter().any(|row| {
        row.try_get::<String, _>("name")
            .map(|name| name == "quoted_json")
            .unwrap_or(false)
    });
    if !has_quoted_json {
        sqlx::query("ALTER TABLE qq_chat_context_turn ADD COLUMN quoted_json TEXT NULL")
            .execute(&mut *conn)
            .await
            .map_err(|e| {
                Error::Database(sqlx::Error::Protocol(format!(
                    "SQLite ALTER TABLE failed for column 'quoted_json': {}",
                    e
                )))
            })?;
    }
    super::super::migration_task::tracked("回填旧上下文会话摘要（SQLite）", async {
        sqlx::query(&crate::storage::context_store::session_backfill_sql(false, false))
            .execute(&mut *conn)
            .await?;
        Ok(())
    })
    .await?;
    Ok(())
}
