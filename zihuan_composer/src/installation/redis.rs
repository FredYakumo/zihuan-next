//! Redis install logic for the detailed component installer: validation, Compose
//! service rendering (including ACL auth), connection building, saving, and
//! reachability checks.

use std::time::Duration;

use crate::installation::{
    access_endpoint, compose_bind_mount, endpoint_host_port, wait_for_tcp, yaml_quote,
    DetailedComponentSource, DetailedSetupConfig,
};
use crate::orchestrator::config_factory;
use zihuan_core::storage::{ConnectionConfig, ConnectionKind, RedisConnection};

/// Startup budget for [`verify`].
const REDIS_VERIFY_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn validate(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.redis.enabled && config.redis.url.trim().is_empty() {
        return Err("Redis URL is required".to_string());
    }
    if config.redis.enabled {
        let username = config.redis.username.as_deref().unwrap_or_default().trim();
        let password = config.redis.password.as_deref().unwrap_or_default();
        if !username.is_empty() && username != "default" && password.trim().is_empty() {
            return Err("A Redis password is required when a Redis username is set".to_string());
        }
        // The password becomes a single ACL rule token, so whitespace would break auth.
        if password.split_whitespace().count() > 1 {
            return Err("A Redis password must not contain whitespace".to_string());
        }
    }
    if config.expose_public_access
        && config.redis.enabled
        && config.redis.password.as_deref().unwrap_or_default().is_empty()
    {
        return Err("A Redis password is required when exposing public access".to_string());
    }
    Ok(())
}

pub(crate) fn install_service(config: &DetailedSetupConfig) -> Option<&'static str> {
    (config.redis.enabled && config.redis.source == DetailedComponentSource::Install)
        .then_some("redis")
}

pub(crate) fn data_dir(config: &DetailedSetupConfig) -> Option<String> {
    (config.redis.enabled && config.redis.source == DetailedComponentSource::Install)
        .then(|| config.redis.deployment.data_dir.clone())
}

pub(crate) fn compose_service(config: &DetailedSetupConfig) -> String {
    if !(config.redis.enabled && config.redis.source == DetailedComponentSource::Install) {
        return String::new();
    }
    let d = &config.redis.deployment;
    // The Redis connection created for this setup always carries the stored
    // username/password, so the container must be deployed with matching auth
    // even when it is only reachable on localhost. redis-server consumes every
    // following non-`--` token as one ACL rule, so the `user` pieces are
    // separate compose command elements.
    let password = config.redis.password.as_deref().unwrap_or_default().trim().to_string();
    let username = config.redis.username.as_deref().unwrap_or_default().trim().to_string();
    let command = if password.is_empty() {
        String::new()
    } else if username.is_empty() || username == "default" {
        format!(
            "    command: [\"redis-server\", \"--requirepass\", {}]\n",
            yaml_quote(&password)
        )
    } else {
        format!(
            "    command: [\"redis-server\", \"--user\", {}, \"on\", {}, \"~*\", \"&*\", \"+@all\", \"--user\", \"default\", \"off\", \"nopass\"]\n",
            yaml_quote(&username),
            yaml_quote(&format!(">{password}"))
        )
    };
    format!("  redis:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:6379\"]\n    volumes: [{}]\n{}", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, compose_bind_mount(&d.data_dir, "/data"), command)
}

/// The connection as published by the generated install command, pointing at
/// the target machine address when public access was requested.
pub(crate) fn remote_connection(config: &DetailedSetupConfig) -> Option<ConnectionConfig> {
    if !config.redis.enabled {
        return None;
    }
    let url = access_endpoint(config, &config.redis.url, "redis", config.redis.deployment.port);
    Some(config_factory::build_connection(
        "setup-detailed-redis",
        "Redis",
        ConnectionKind::Redis(RedisConnection {
            url,
            username: config.redis.username.clone(),
            password: config.redis.password.clone(),
        }),
    ))
}

pub(crate) fn save_connection(config: &DetailedSetupConfig) -> Result<(), String> {
    if !config.redis.enabled {
        return Ok(());
    }
    config_factory::save_connection(config_factory::build_connection(
        "setup-detailed-redis",
        "Redis",
        ConnectionKind::Redis(RedisConnection {
            url: config.redis.url.clone(),
            username: config.redis.username.clone(),
            password: config.redis.password.clone(),
        }),
    ))
}

pub(crate) async fn verify(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.redis.enabled {
        let (host, port) = endpoint_host_port(&config.redis.url, 6379)?;
        wait_for_tcp(&host, port, "Redis", REDIS_VERIFY_TIMEOUT).await?;
    }
    Ok(())
}

pub(crate) fn binary_package(_package_manager: &str) -> Result<&'static str, String> {
    Ok("redis")
}
