use log::info;
use sqlx::mysql::MySqlConnection;
use sqlx::sqlite::SqliteConnection;
use sqlx::Connection;
use zihuan_core::database;
use zihuan_core::error::Result;

use crate::ConnectionKind;

/// Open a temporary connection to the database described by `kind`,
/// ensure all required tables and indexes exist, then close the connection.
///
/// For MySQL and SQLite connections, this is called when a connection is
/// created or updated via the REST API, and at startup for all existing
/// connections. Non-database connection kinds are silently ignored.
pub async fn ensure_tables_for_connection(kind: &ConnectionKind) -> Result<()> {
    match kind {
        ConnectionKind::Mysql(mysql) => {
            info!("[db_schema] ensuring MySQL tables for url={}", mask_url(&mysql.url));
            let mut conn = MySqlConnection::connect(&mysql.url).await.map_err(|e| {
                zihuan_core::error::Error::Database(sqlx::Error::Configuration(Box::new(std::io::Error::other(
                    format!(
                        "failed to connect to MySQL for schema setup (url={}): {}",
                        mask_url(&mysql.url),
                        e
                    ),
                ))))
            })?;
            database::ensure_tables_mysql(&mut conn).await?;
            conn.close().await.map_err(|e| {
                zihuan_core::error::Error::Database(sqlx::Error::Protocol(format!(
                    "failed to close MySQL schema connection: {}",
                    e
                )))
            })?;
            info!("[db_schema] MySQL tables ensured successfully");
        }
        ConnectionKind::Sqlite(sqlite) => {
            let db_url = format!("sqlite://{}?mode=rwc", sqlite.path);
            info!("[db_schema] ensuring SQLite tables for path={}", sqlite.path);
            let mut conn = SqliteConnection::connect(&db_url).await.map_err(|e| {
                zihuan_core::error::Error::Database(sqlx::Error::Configuration(Box::new(std::io::Error::other(
                    format!("failed to connect to SQLite for schema setup (path={}): {}", sqlite.path, e),
                ))))
            })?;
            database::ensure_tables_sqlite(&mut conn).await?;
            conn.close().await.map_err(|e| {
                zihuan_core::error::Error::Database(sqlx::Error::Protocol(format!(
                    "failed to close SQLite schema connection: {}",
                    e
                )))
            })?;
            info!("[db_schema] SQLite tables ensured successfully");
        }
        _ => {}
    }
    Ok(())
}

fn mask_url(url: &str) -> String {
    if let Some(at_pos) = url.find('@') {
        if let Some(protocol_end) = url.find("://") {
            let prefix = &url[..=protocol_end + 2];
            let creds = &url[protocol_end + 3..at_pos];
            if creds.contains(':') {
                return format!("{}***@{}", prefix, &url[at_pos + 1..]);
            }
        }
    }
    url.to_string()
}
