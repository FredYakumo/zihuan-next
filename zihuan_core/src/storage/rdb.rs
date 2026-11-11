use crate::connection_manager::ConnectionManager;
use crate::data_refs::RelationalDbConnection;
use crate::error::{Error, Result};
use crate::storage::ConnectionHandle;

use crate::storage::{find_connection, ConnectionConfig, ConnectionKind, RuntimeConnectionManager};

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
    if !matches!(
        kind,
        ConnectionKind::Mysql(_) | ConnectionKind::Sqlite(_) | ConnectionKind::MockRdb
    ) {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a relational database connection",
            connection_id
        )));
    }
    match RuntimeConnectionManager::shared().get_or_create(connection_id).await? {
        ConnectionHandle::MySql(value) => Ok(RelationalDbConnection::MySql(value)),
        ConnectionHandle::Sqlite(value) => Ok(RelationalDbConnection::Sqlite(value)),
        _ => Err(crate::string_error!(
            "connection '{}' is not a relational database connection",
            connection_id
        )),
    }
}
