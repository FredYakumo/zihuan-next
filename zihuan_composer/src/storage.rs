use zihuan_core::storage::ConnectionConfig;

pub fn load_connections() -> Result<Vec<ConnectionConfig>, String> {
    crate::config::read_yaml(&crate::config::root().join("connections.yaml"))
}

pub fn save_connections(connections: Vec<ConnectionConfig>) -> Result<(), String> {
    crate::config::write_yaml(&crate::config::root().join("connections.yaml"), &connections)
}

pub fn upsert_connection(connection: ConnectionConfig) -> Result<(), String> {
    let _guard = crate::config::WRITE_LOCK.lock().map_err(|error| error.to_string())?;
    let mut connections = load_connections()?;
    connections.retain(|item| item.config_id != connection.config_id);
    connections.push(connection);
    crate::config::write_yaml_unlocked(
        &crate::config::root().join("connections.yaml"),
        &connections,
    )
}

pub fn delete_connection(id: &str) -> Result<(), String> {
    let _guard = crate::config::WRITE_LOCK.lock().map_err(|error| error.to_string())?;
    let mut connections = load_connections()?;
    connections.retain(|item| item.config_id != id);
    crate::config::write_yaml_unlocked(
        &crate::config::root().join("connections.yaml"),
        &connections,
    )
}
