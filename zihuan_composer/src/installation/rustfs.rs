//! RustFS object storage install logic for the detailed component installer:
//! validation, Compose service rendering, connection building, saving, and
//! reachability checks.

use std::time::Duration;

use crate::installation::{
    access_endpoint, compose_bind_mount, endpoint_host_port, wait_for_tcp, yaml_quote,
    DetailedComponentSource, DetailedSetupConfig,
};
use crate::orchestrator::config_factory;
use zihuan_core::storage::{ConnectionConfig, ConnectionKind, RustfsConnection};

/// Startup budget for [`verify`].
const RUSTFS_VERIFY_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn validate(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.rustfs.enabled
        && (config.rustfs.endpoint.trim().is_empty() || config.rustfs.bucket.trim().is_empty())
    {
        return Err("RustFS endpoint and bucket are required".to_string());
    }
    if config.expose_public_access
        && config.rustfs.enabled
        && (config.rustfs.access_key.is_empty() || config.rustfs.secret_key.is_empty())
    {
        return Err(
            "RustFS access and secret keys are required when exposing public access".to_string()
        );
    }
    Ok(())
}

pub(crate) fn install_service(config: &DetailedSetupConfig) -> Option<&'static str> {
    (config.rustfs.enabled && config.rustfs.source == DetailedComponentSource::Install)
        .then_some("rustfs")
}

pub(crate) fn data_dir(config: &DetailedSetupConfig) -> Option<String> {
    (config.rustfs.enabled && config.rustfs.source == DetailedComponentSource::Install)
        .then(|| config.rustfs.deployment.data_dir.clone())
}

pub(crate) fn compose_service(config: &DetailedSetupConfig) -> String {
    if !(config.rustfs.enabled && config.rustfs.source == DetailedComponentSource::Install) {
        return String::new();
    }
    let d = &config.rustfs.deployment;
    format!("  rustfs:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:9000\"]\n    volumes: [{}]\n    environment:\n      RUSTFS_ACCESS_KEY: {}\n      RUSTFS_SECRET_KEY: {}\n    command: [\"--console-enable\", \"/data\"]\n", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, compose_bind_mount(&d.data_dir, "/data"), yaml_quote(&config.rustfs.access_key), yaml_quote(&config.rustfs.secret_key))
}

/// The connection as published by the generated install command, pointing at
/// the target machine address when public access was requested.
pub(crate) fn remote_connection(config: &DetailedSetupConfig) -> Option<ConnectionConfig> {
    if !config.rustfs.enabled {
        return None;
    }
    let endpoint =
        access_endpoint(config, &config.rustfs.endpoint, "http", config.rustfs.deployment.port);
    Some(config_factory::build_connection(
        "setup-detailed-rustfs",
        "RustFS",
        ConnectionKind::Rustfs(RustfsConnection {
            endpoint,
            bucket: config.rustfs.bucket.clone(),
            region: config.rustfs.region.clone(),
            access_key: config.rustfs.access_key.clone(),
            secret_key: config.rustfs.secret_key.clone(),
            public_base_url: config.rustfs.public_base_url.clone(),
            path_style: config.rustfs.path_style,
        }),
    ))
}

pub(crate) fn save_connection(config: &DetailedSetupConfig) -> Result<(), String> {
    if !config.rustfs.enabled {
        return Ok(());
    }
    config_factory::save_connection(config_factory::build_connection(
        "setup-detailed-rustfs",
        "RustFS",
        ConnectionKind::Rustfs(RustfsConnection {
            endpoint: config.rustfs.endpoint.clone(),
            bucket: config.rustfs.bucket.clone(),
            region: config.rustfs.region.clone(),
            access_key: config.rustfs.access_key.clone(),
            secret_key: config.rustfs.secret_key.clone(),
            public_base_url: config.rustfs.public_base_url.clone(),
            path_style: config.rustfs.path_style,
        }),
    ))
}

pub(crate) async fn verify(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.rustfs.enabled {
        let (host, port) = endpoint_host_port(&config.rustfs.endpoint, 9000)?;
        wait_for_tcp(&host, port, "RustFS", RUSTFS_VERIFY_TIMEOUT).await?;
    }
    Ok(())
}

pub(crate) fn binary_package(package_manager: &str) -> Result<&'static str, String> {
    Err(format!(
        "rustfs is unavailable from {package_manager}; use Docker or install its official release before choosing '使用现有配置'."
    ))
}
