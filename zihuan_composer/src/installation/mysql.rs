//! Relational database (MySQL / SQLite) install logic for the detailed setup
//! wizard: validation, Compose service rendering, connection building, saving,
//! and reachability checks.

use std::path::Path;
use std::time::Duration;

use sqlx::Connection;

use crate::installation::{
    compose_bind_mount, detailed_connection_host, wait_for_tcp, yaml_quote,
    DetailedComponentSource, DetailedSetupConfig,
};
use crate::orchestrator::config_factory;
use zihuan_core::storage::{ConnectionConfig, ConnectionKind, MysqlConnection, SqliteConnection};

/// Startup budget for [`verify`]. Containers report a running state long before
/// their protocol port answers, so MySQL gets a budget that matches its real
/// cold start.
const MYSQL_VERIFY_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) fn validate(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.relational.enabled && config.relational.database_type == "mysql" {
        if config.relational.host.trim().is_empty() || config.relational.database.trim().is_empty()
        {
            return Err("MySQL host and database are required".to_string());
        }
        if config.relational.source == DetailedComponentSource::Install
            && config.relational.password.is_empty()
        {
            return Err("A MySQL root password is required when installing MySQL".to_string());
        }
    }
    if config.relational.enabled
        && config.relational.database_type == "sqlite"
        && config.relational.sqlite_path.trim().is_empty()
    {
        return Err("SQLite database path is required".to_string());
    }
    if config.expose_public_access
        && config.relational.enabled
        && config.relational.database_type == "mysql"
        && config.relational.password.is_empty()
    {
        return Err("A MySQL password is required when exposing public access".to_string());
    }
    Ok(())
}

pub(crate) fn install_service(config: &DetailedSetupConfig) -> Option<&'static str> {
    (config.relational.enabled
        && config.relational.source == DetailedComponentSource::Install
        && config.relational.database_type == "mysql")
        .then_some("mysql")
}

pub(crate) fn data_dir(config: &DetailedSetupConfig) -> Option<String> {
    (config.relational.enabled
        && config.relational.source == DetailedComponentSource::Install
        && config.relational.database_type == "mysql")
        .then(|| config.relational.deployment.data_dir.clone())
}

pub(crate) fn compose_service(config: &DetailedSetupConfig) -> String {
    if !(config.relational.enabled
        && config.relational.source == DetailedComponentSource::Install
        && config.relational.database_type == "mysql")
    {
        return String::new();
    }
    let d = &config.relational.deployment;
    format!("  mysql:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:3306\"]\n    volumes: [{}]\n    environment:\n      MYSQL_ROOT_PASSWORD: {}\n      MYSQL_DATABASE: {}\n", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, compose_bind_mount(&d.data_dir, "/var/lib/mysql"), yaml_quote(&config.relational.password), yaml_quote(&config.relational.database))
}

/// The connection as published by the generated install command, pointing at
/// the target machine address when public access was requested.
pub(crate) fn remote_connection(config: &DetailedSetupConfig) -> Option<ConnectionConfig> {
    if !config.relational.enabled {
        return None;
    }
    if config.relational.database_type == "sqlite" {
        return Some(config_factory::build_connection(
            "setup-detailed-sqlite",
            "SQLite",
            ConnectionKind::Sqlite(SqliteConnection {
                path: config.relational.sqlite_path.clone(),
            }),
        ));
    }
    let host = detailed_connection_host(config, &config.relational.host);
    let url = format!(
        "mysql://{}:{}@{}:{}/{}",
        config.relational.username,
        config.relational.password,
        host,
        config.relational.deployment.port,
        config.relational.database,
    );
    Some(config_factory::build_connection(
        "setup-detailed-mysql",
        "MySQL",
        ConnectionKind::Mysql(MysqlConnection {
            url,
            max_connections: config.relational.max_connections,
            acquire_timeout_secs: config.relational.acquire_timeout_secs,
        }),
    ))
}

pub(crate) async fn save_connection(config: &DetailedSetupConfig) -> Result<(), String> {
    if !config.relational.enabled {
        return Ok(());
    }
    if config.relational.database_type == "sqlite" {
        let path = Path::new(&config.relational.sqlite_path);
        if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            tokio::fs::create_dir_all(parent).await.map_err(|err| err.to_string())?;
        }
        let mut connection =
            sqlx::SqliteConnection::connect(&format!("sqlite://{}?mode=rwc", path.display()))
                .await
                .map_err(|err| format!("Failed to create SQLite database: {err}"))?;
        zihuan_core::database::ensure_tables_sqlite(&mut connection)
            .await
            .map_err(|err| format!("Failed to initialize SQLite database: {err}"))?;
        config_factory::save_connection(config_factory::build_connection(
            "setup-detailed-sqlite",
            "SQLite",
            ConnectionKind::Sqlite(SqliteConnection {
                path: config.relational.sqlite_path.clone(),
            }),
        ))?;
    } else {
        let url = format!(
            "mysql://{}:{}@{}:{}/{}",
            config.relational.username,
            config.relational.password,
            config.relational.host,
            config.relational.deployment.port,
            config.relational.database
        );
        config_factory::save_connection(config_factory::build_connection(
            "setup-detailed-mysql",
            "MySQL",
            ConnectionKind::Mysql(MysqlConnection {
                url,
                max_connections: config.relational.max_connections,
                acquire_timeout_secs: config.relational.acquire_timeout_secs,
            }),
        ))?;
    }
    Ok(())
}

pub(crate) async fn verify(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.relational.enabled && config.relational.database_type == "mysql" {
        wait_for_tcp(
            &config.relational.host,
            config.relational.deployment.port,
            "MySQL",
            MYSQL_VERIFY_TIMEOUT,
        )
        .await?;
    }
    Ok(())
}

pub(crate) fn binary_package(package_manager: &str) -> Result<&'static str, String> {
    Ok(match package_manager {
        "brew" => "mysql",
        _ => "mysql-server",
    })
}
