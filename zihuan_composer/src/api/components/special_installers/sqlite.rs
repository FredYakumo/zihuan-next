use std::future::Future;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::pin::Pin;

use serde_json::Value;
use sqlx::Connection;

use super::super::{
    component_from_request, save_component_record, ComponentRecord, InstallComponentRequest,
};
use super::SpecialComponentInstaller;
use crate::storage;
use zihuan_core::storage::{ConnectionConfig, ConnectionKind, SqliteConnection};

pub struct SqliteSpecialComponentInstaller;

/// SQLite is an embedded special installation mode: it creates and initializes a component-owned
/// database in the application data directory, then removes that database during component uninstall.
impl SpecialComponentInstaller for SqliteSpecialComponentInstaller {
    fn component_type(&self) -> &'static str {
        "sqlite"
    }

    fn install<'a>(
        &'a self,
        request: &'a InstallComponentRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ComponentRecord, String>> + Send + 'a>> {
        Box::pin(async move {
            let mut component = component_from_request(request, "installing");
            component.installation_method = "embedded".to_string();
            let database_path = sqlite_database_path(&component.id);
            let parent = database_path.parent().ok_or_else(|| {
                format!(
                    "SQLite database path '{}' has no parent directory",
                    database_path.display()
                )
            })?;
            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                format!("Failed to create SQLite data directory '{}': {error}", parent.display())
            })?;
            let mut database = sqlx::SqliteConnection::connect(&format!(
                "sqlite://{}?mode=rwc",
                database_path.display()
            ))
            .await
            .map_err(|error| format!("Failed to create SQLite database: {error}"))?;
            zihuan_core::database::ensure_tables_sqlite(&mut database)
                .await
                .map_err(|error| format!("Failed to initialize SQLite database: {error}"))?;
            let connection = ConnectionConfig {
                id: format!("component-sqlite-{}", component.id),
                config_id: format!("component-sqlite-{}", component.id),
                name: format!("{} SQLite", component.name),
                enabled: true,
                kind: ConnectionKind::Sqlite(SqliteConnection {
                    path: database_path.to_string_lossy().to_string(),
                }),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            storage::upsert_connection(connection.clone()).map_err(|error| error.to_string())?;
            component.connection_ids = vec![connection.config_id];
            component.status = "installed".to_string();
            component.updated_at = chrono::Utc::now().to_rfc3339();
            component.extra_install_metadata["database_path"] =
                Value::String(database_path.to_string_lossy().to_string());
            save_component_record(&component)?;
            Ok(component)
        })
    }

    fn uninstall<'a>(
        &'a self,
        component: &'a ComponentRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let Some(path) =
                component.extra_install_metadata.get("database_path").and_then(Value::as_str)
            else {
                return Ok(());
            };
            match tokio::fs::remove_file(path).await {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("Failed to remove SQLite database '{path}': {error}")),
            }
        })
    }
}

fn sqlite_database_path(component_id: &str) -> PathBuf {
    crate::config::data_dir()
        .join("data")
        .join(format!("component-{component_id}.db"))
}
