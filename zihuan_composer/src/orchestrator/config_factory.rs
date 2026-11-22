use zihuan_core::storage::{ConnectionConfig, ConnectionKind};

pub fn build_connection(id: &str, name: &str, kind: ConnectionKind) -> ConnectionConfig {
    ConnectionConfig {
        id: crate::config::connection_id(id),
        config_id: crate::config::connection_id(id),
        name: name.to_string(),
        enabled: true,
        kind,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

pub fn save_connection(connection: ConnectionConfig) -> Result<(), String> {
    crate::storage::upsert_connection(connection)
}
