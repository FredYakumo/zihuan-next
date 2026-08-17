use std::future::Future;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::pin::Pin;

use serde_json::Value;
use sqlx::Connection;

use super::super::{plugin_from_request, save_plugin_record, InstallPluginRequest, PluginRecord};
use super::SpecialPluginInstaller;
use zihuan_core::storage::{self, ConnectionConfig, ConnectionKind, SqliteConnection};

pub struct SqliteSpecialPluginInstaller;

/// SQLite is an embedded special installation mode: it creates and initializes a plugin-owned
/// database in the application data directory, then removes that database during plugin uninstall.
impl SpecialPluginInstaller for SqliteSpecialPluginInstaller {
    fn component_type(&self) -> &'static str {
        "sqlite"
    }

    fn install<'a>(
        &'a self,
        request: &'a InstallPluginRequest,
    ) -> Pin<Box<dyn Future<Output = Result<PluginRecord, String>> + Send + 'a>> {
        Box::pin(async move {
            let mut plugin = plugin_from_request(request, "installing");
            plugin.installation_method = "embedded".to_string();
            let database_path = sqlite_database_path(&plugin.id);
            let parent = database_path.parent().ok_or_else(|| {
                format!("SQLite database path '{}' has no parent directory", database_path.display())
            })?;
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| format!("Failed to create SQLite data directory '{}': {error}", parent.display()))?;
            let mut database = sqlx::SqliteConnection::connect(&format!("sqlite://{}?mode=rwc", database_path.display()))
                .await
                .map_err(|error| format!("Failed to create SQLite database: {error}"))?;
            zihuan_core::database::ensure_tables_sqlite(&mut database)
                .await
                .map_err(|error| format!("Failed to initialize SQLite database: {error}"))?;
            let connection = ConnectionConfig {
                id: format!("plugin-sqlite-{}", plugin.id),
                config_id: format!("plugin-sqlite-{}", plugin.id),
                name: format!("{} SQLite", plugin.name),
                enabled: true,
                kind: ConnectionKind::Sqlite(SqliteConnection { path: database_path.to_string_lossy().to_string() }),
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            storage::upsert_connection(connection.clone()).map_err(|error| error.to_string())?;
            plugin.connection_ids = vec![connection.config_id];
            plugin.status = "installed".to_string();
            plugin.updated_at = chrono::Utc::now().to_rfc3339();
            plugin.extra_install_metadata["database_path"] = Value::String(database_path.to_string_lossy().to_string());
            save_plugin_record(&plugin)?;
            Ok(plugin)
        })
    }

    fn uninstall<'a>(
        &'a self,
        plugin: &'a PluginRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let Some(path) = plugin.extra_install_metadata.get("database_path").and_then(Value::as_str) else {
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

fn sqlite_database_path(plugin_id: &str) -> PathBuf {
    zihuan_core::system_config::application_data_dir()
        .join("data")
        .join(format!("plugin-{plugin_id}.db"))
}
