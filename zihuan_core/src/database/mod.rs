pub mod ddl;

use sqlx::mysql::MySqlConnection;
use sqlx::sqlite::SqliteConnection;

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
                    log::debug!(
                        "MySQL index already exists, skipping: {}",
                        &idx[..idx.len().min(120)]
                    );
                    continue;
                }
                return Err(Error::Database(sqlx::Error::Protocol(format!(
                    "MySQL index creation failed: {} — statement: {}",
                    msg, idx
                ))));
            }
        }
    }
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
    Ok(())
}
