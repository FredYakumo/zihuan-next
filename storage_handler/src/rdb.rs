use std::sync::Arc;

use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};

use crate::{find_connection, ConnectionConfig, ConnectionKind, RuntimeStorageConnectionManager};

pub async fn build_relational_db_connection_for_connection(
    connection_id: &str,
    connections: &[ConnectionConfig],
) -> Result<RelationalDbConnection> {
    let connection = find_connection(connections, connection_id)?;
    build_relational_db_connection_for_kind(connection_id, &connection.kind).await
}

pub async fn build_relational_db_connection_for_kind(
    connection_id: &str,
    kind: &ConnectionKind,
) -> Result<RelationalDbConnection> {
    match kind {
        ConnectionKind::Mysql(_) => Ok(RelationalDbConnection::MySql(
            RuntimeStorageConnectionManager::shared()
                .get_or_create_mysql_ref(connection_id)
                .await?,
        )),
        ConnectionKind::Sqlite(_) => Ok(RelationalDbConnection::Sqlite(
            RuntimeStorageConnectionManager::shared()
                .get_or_create_sqlite_ref(connection_id)
                .await?,
        )),
        _ => Err(Error::ValidationError(format!(
            "connection '{}' is not a relational database connection",
            connection_id
        ))),
    }
}
