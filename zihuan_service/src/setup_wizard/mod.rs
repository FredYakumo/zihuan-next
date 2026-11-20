//! Detailed component installation for the setup wizard: validation, install
//! command generation, Docker Compose / binary installers, connection saving,
//! and TCP reachability checks. The per-component install logic lives in
//! [`mysql`], [`rustfs`], [`redis`], [`weaviate`], [`elasticsearch`], and
//! [`napcat`].

pub mod elasticsearch;
pub mod mysql;
pub mod napcat;
pub mod redis;
pub mod rustfs;
pub mod weaviate;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use zihuan_core::retrieval::RetrievalSchema;
use zihuan_core::storage::{ConnectionConfig, ConnectionKind};

use crate::setup_orchestrator::config_factory;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailedInstallMethod {
    Docker,
    Binary,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailedComponentSource {
    Install,
    Existing,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailedSearchAuthMethod {
    Password,
    ApiKey,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DetailedDeploymentConfig {
    pub image: String,
    pub port: u16,
    pub data_dir: String,
    pub container_name: String,
    pub restart_policy: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DetailedRelationalSetupConfig {
    pub enabled: bool,
    pub source: DetailedComponentSource,
    #[serde(rename = "type")]
    pub database_type: String,
    pub deployment: DetailedDeploymentConfig,
    pub host: String,
    pub username: String,
    pub password: String,
    pub database: String,
    pub sqlite_path: String,
    pub max_connections: u32,
    pub acquire_timeout_secs: u64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DetailedRustfsSetupConfig {
    pub enabled: bool,
    pub source: DetailedComponentSource,
    pub deployment: DetailedDeploymentConfig,
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub public_base_url: Option<String>,
    pub path_style: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DetailedSearchSetupConfig {
    pub enabled: bool,
    pub source: DetailedComponentSource,
    #[serde(rename = "type")]
    pub search_type: String,
    pub deployment: DetailedDeploymentConfig,
    pub base_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub api_key: Option<String>,
    pub auth_method: DetailedSearchAuthMethod,
    pub vector_dimensions: usize,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DetailedRedisSetupConfig {
    pub enabled: bool,
    pub source: DetailedComponentSource,
    pub deployment: DetailedDeploymentConfig,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct DetailedSetupConfig {
    pub install_method: DetailedInstallMethod,
    pub target_machine_address: String,
    #[serde(default)]
    pub expose_public_access: bool,
    #[serde(default)]
    pub use_target_machine_address: bool,
    pub relational: DetailedRelationalSetupConfig,
    pub rustfs: DetailedRustfsSetupConfig,
    pub search: DetailedSearchSetupConfig,
    pub redis: DetailedRedisSetupConfig,
}

#[derive(Serialize)]
pub struct DetailedInstallCommand {
    pub install_command: String,
    pub connections: Vec<ConnectionConfig>,
}

/// Retrieval schemas initialized on a newly installed search database. One
/// retrieval connection serves every schema, so installers set them all up.
pub(crate) const RETRIEVAL_SCHEMAS: [RetrievalSchema; 5] = [
    RetrievalSchema::ImageSemantic,
    RetrievalSchema::ConceptMemory,
    RetrievalSchema::PersonMemory,
    RetrievalSchema::BehavioralMemory,
    RetrievalSchema::QqMessage,
];

pub fn validate_detailed_config(config: &DetailedSetupConfig) -> Result<(), String> {
    if !config.relational.enabled
        && !config.rustfs.enabled
        && !config.search.enabled
        && !config.redis.enabled
    {
        return Err("Select at least one component to configure".to_string());
    }
    mysql::validate(config)?;
    rustfs::validate(config)?;
    validate_search(config)?;
    redis::validate(config)?;
    Ok(())
}

fn validate_search(config: &DetailedSetupConfig) -> Result<(), String> {
    if !config.search.enabled {
        return Ok(());
    }
    if config.search.base_url.trim().is_empty() {
        return Err("Search database Base URL is required".to_string());
    }
    let username = config.search.username.as_deref().unwrap_or_default().trim();
    let password = config.search.password.as_deref().unwrap_or_default().trim();
    let api_key = config.search.api_key.as_deref().unwrap_or_default().trim();
    match config.search.auth_method {
        DetailedSearchAuthMethod::Password => {
            if password.is_empty() {
                return Err("A search database password is required".to_string());
            }
            if !api_key.is_empty() {
                return Err(
                    "Configure either a password or an API key for the search database, not both"
                        .to_string(),
                );
            }
            if config.search.source == DetailedComponentSource::Existing && username.is_empty() {
                return Err(
                    "A search database username is required when using password authentication"
                        .to_string(),
                );
            }
        }
        DetailedSearchAuthMethod::ApiKey => {
            if api_key.is_empty() {
                return Err("A search database API key is required".to_string());
            }
            if !username.is_empty() || !password.is_empty() {
                return Err(
                    "Configure either a password or an API key for the search database, not both"
                        .to_string(),
                );
            }
        }
    }

    match (config.search.search_type.as_str(), config.search.source.clone()) {
        ("elasticsearch", DetailedComponentSource::Install)
            if config.search.auth_method != DetailedSearchAuthMethod::Password =>
        {
            return Err(
                "Wizard-installed Elasticsearch requires password authentication".to_string()
            );
        }
        ("weaviate", DetailedComponentSource::Install)
            if config.search.auth_method != DetailedSearchAuthMethod::ApiKey =>
        {
            return Err("Wizard-installed Weaviate requires API key authentication".to_string());
        }
        ("elasticsearch" | "weaviate", _) => {}
        _ => return Err("Unsupported search database type".to_string()),
    }
    Ok(())
}

pub fn generate_detailed_install_command(
    config: &DetailedSetupConfig,
) -> Result<DetailedInstallCommand, String> {
    validate_detailed_config(config)?;

    let compose = detailed_compose(config);
    let install_command = match &config.install_method {
        DetailedInstallMethod::Docker => docker_install_command(config, &compose),
        DetailedInstallMethod::Binary => binary_install_command(config, &compose),
    };

    Ok(DetailedInstallCommand {
        install_command,
        connections: detailed_connection_configs(config),
    })
}

/// Generates a copyable install command that only sets up the retrieval
/// database described by `search`.
pub fn generate_retrieval_install_command(
    search: &DetailedSearchSetupConfig,
    install_method: &DetailedInstallMethod,
) -> Result<DetailedInstallCommand, String> {
    let config = build_search_only_config(search.clone(), install_method.clone());
    validate_detailed_config(&config)?;

    let install_command = match install_method {
        DetailedInstallMethod::Docker => {
            docker_install_command(&config, &detailed_compose(&config))
        }
        DetailedInstallMethod::Binary => match config.search.search_type.as_str() {
            "weaviate" => weaviate::binary_install_command(&config),
            "elasticsearch" => elasticsearch::binary_install_command(),
            _ => return Err("Unsupported search database type".to_string()),
        },
    };

    Ok(DetailedInstallCommand {
        install_command,
        connections: detailed_connection_configs(&config),
    })
}

/// Builds a [`DetailedSetupConfig`] where only the search component is
/// enabled and marked for installation, so the single-service compose
/// generation and connection saving can be reused from the setup wizard.
pub fn build_search_only_config(
    mut search: DetailedSearchSetupConfig,
    install_method: DetailedInstallMethod,
) -> DetailedSetupConfig {
    search.enabled = true;
    search.source = DetailedComponentSource::Install;
    DetailedSetupConfig {
        install_method,
        target_machine_address: String::new(),
        expose_public_access: false,
        use_target_machine_address: false,
        relational: DetailedRelationalSetupConfig {
            enabled: false,
            source: DetailedComponentSource::Install,
            database_type: "sqlite".to_string(),
            deployment: placeholder_deployment_config(),
            host: "127.0.0.1".to_string(),
            username: String::new(),
            password: String::new(),
            database: String::new(),
            sqlite_path: String::new(),
            max_connections: 32,
            acquire_timeout_secs: 30,
        },
        rustfs: DetailedRustfsSetupConfig {
            enabled: false,
            source: DetailedComponentSource::Install,
            deployment: placeholder_deployment_config(),
            endpoint: "http://127.0.0.1:9000".to_string(),
            bucket: "zihuan".to_string(),
            region: "us-east-1".to_string(),
            access_key: String::new(),
            secret_key: String::new(),
            public_base_url: None,
            path_style: true,
        },
        search,
        redis: DetailedRedisSetupConfig {
            enabled: false,
            source: DetailedComponentSource::Install,
            deployment: placeholder_deployment_config(),
            url: "redis://127.0.0.1:6379".to_string(),
            username: None,
            password: None,
        },
    }
}

fn placeholder_deployment_config() -> DetailedDeploymentConfig {
    DetailedDeploymentConfig {
        image: String::new(),
        port: 0,
        data_dir: String::new(),
        container_name: String::new(),
        restart_policy: "unless-stopped".to_string(),
    }
}

pub(crate) fn detailed_install_services(config: &DetailedSetupConfig) -> Vec<&'static str> {
    let mut services = Vec::new();
    services.extend(mysql::install_service(config));
    services.extend(rustfs::install_service(config));
    services.extend(weaviate::install_service(config));
    services.extend(elasticsearch::install_service(config));
    services.extend(redis::install_service(config));
    services
}

fn detailed_data_dirs(config: &DetailedSetupConfig) -> Vec<String> {
    let mut data_dirs = Vec::new();
    data_dirs.extend(mysql::data_dir(config));
    data_dirs.extend(rustfs::data_dir(config));
    data_dirs.extend(weaviate::data_dir(config));
    data_dirs.extend(elasticsearch::data_dir(config));
    data_dirs.extend(redis::data_dir(config));
    data_dirs
}

fn detailed_compose(config: &DetailedSetupConfig) -> String {
    let mut services = String::from("services:\n");
    services.push_str(&mysql::compose_service(config));
    services.push_str(&rustfs::compose_service(config));
    services.push_str(&weaviate::compose_service(config));
    services.push_str(&elasticsearch::compose_service(config));
    services.push_str(&redis::compose_service(config));
    services
}

fn docker_install_command(config: &DetailedSetupConfig, compose: &str) -> String {
    let compose_base64 = base64_encode(compose.as_bytes());
    let create_data_dirs = docker_data_directory_command(config);
    format!(
        "# Run on the target Linux machine\nmkdir -p ~/zihuan-next-install && cd ~/zihuan-next-install\n{create_data_dirs}\nprintf '%s' '{compose_base64}' | base64 -d > docker-compose.yaml\ndocker compose -f docker-compose.yaml up -d"
    )
}

fn binary_install_command(config: &DetailedSetupConfig, compose: &str) -> String {
    let compose_base64 = base64_encode(compose.as_bytes());
    let services = detailed_install_services(config).join(" ");
    let create_data_dirs = docker_data_directory_command(config);
    format!(
        "# Run on the target Linux x86_64 machine\nsudo apt-get update\nsudo apt-get install -y build-essential pkg-config libssl-dev git nodejs npm docker.io docker-compose-plugin\ngit clone --recurse-submodules https://github.com/FredYakumo/zihuan-next.git ~/zihuan-next\ncd ~/zihuan-next\ncorepack enable && corepack prepare pnpm@10.22.0 --activate\ncd webui && pnpm install --frozen-lockfile && pnpm run build\ncd .. && cargo build --release\nmkdir -p ~/zihuan-next-install && cd ~/zihuan-next-install\n{create_data_dirs}\nprintf '%s' '{compose_base64}' | base64 -d > docker-compose.yaml\ndocker compose -f docker-compose.yaml up -d {services}\n# Start Zihuan Next after importing the generated connections JSON:\n~/zihuan-next/target/release/zihuan_next --host 0.0.0.0 --port 9951"
    )
}

pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[((first & 0b0000_0011) << 4 | second >> 4) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((second & 0b0000_1111) << 2 | third >> 6) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(third & 0b0011_1111) as usize] as char
        } else {
            '='
        });
    }
    output
}

/// The connection host for a component: the target machine address when public
/// access through that machine was requested, otherwise the component default.
pub(crate) fn detailed_connection_host<'a>(
    config: &'a DetailedSetupConfig,
    default: &'a str,
) -> &'a str {
    if config.expose_public_access
        && config.use_target_machine_address
        && !config.target_machine_address.trim().is_empty()
    {
        config.target_machine_address.trim()
    } else {
        default
    }
}

/// Rewrites a component endpoint to point at the target machine address when
/// public access through that machine was requested, otherwise returns the
/// configured endpoint unchanged.
pub(crate) fn access_endpoint(
    config: &DetailedSetupConfig,
    default: &str,
    scheme: &str,
    port: u16,
) -> String {
    if config.expose_public_access
        && config.use_target_machine_address
        && !config.target_machine_address.trim().is_empty()
    {
        format!("{scheme}://{}:{}", config.target_machine_address.trim(), port)
    } else {
        default.to_string()
    }
}

fn detailed_connection_configs(config: &DetailedSetupConfig) -> Vec<ConnectionConfig> {
    let mut connections = Vec::new();
    connections.extend(mysql::remote_connection(config));
    connections.extend(rustfs::remote_connection(config));
    connections.extend(redis::remote_connection(config));
    connections.extend(weaviate::remote_connection(config));
    connections.extend(elasticsearch::remote_connection(config));
    connections
}

pub(crate) async fn run_detailed_docker(
    config: &DetailedSetupConfig,
    services: &[&str],
) -> Result<(), String> {
    if !check_command("docker", &["compose", "version"]).await {
        return Err(
            "Docker Compose is unavailable. Install Docker Desktop or Docker Compose, then retry."
                .to_string(),
        );
    }
    let compose_path = detailed_compose_path();
    let compose_dir = compose_path
        .parent()
        .ok_or_else(|| "Detailed Docker Compose path has no parent directory".to_string())?;
    tokio::fs::create_dir_all(compose_dir)
        .await
        .map_err(|err| format!("Failed to create Docker Compose directory: {err}"))?;
    for data_dir in detailed_data_dirs(config) {
        let data_dir = resolve_compose_data_dir(compose_dir, &data_dir);
        tokio::fs::create_dir_all(&data_dir).await.map_err(|err| {
            format!("Failed to create container data directory {}: {err}", data_dir.display())
        })?;
    }
    tokio::fs::write(&compose_path, detailed_compose(config))
        .await
        .map_err(|err| err.to_string())?;
    let output = tokio::process::Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(&compose_path)
        .arg("up")
        .arg("-d")
        .args(services)
        .output()
        .await
        .map_err(|err| format!("Failed to run Docker Compose: {err}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(())
}

pub(crate) async fn run_detailed_binary(
    _config: &DetailedSetupConfig,
    services: &[&str],
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let output = tokio::process::Command::new("wsl").args(["--status"]).output().await;
        if !matches!(output, Ok(ref result) if result.status.success()) {
            return Err("Native installation on Windows requires WSL. Install WSL, then retry, or choose Docker.".to_string());
        }
        return Err(format!(
            "WSL was detected. Run the setup from your WSL distribution to install: {}",
            services.join(", ")
        ));
    }
    #[cfg(not(target_os = "windows"))]
    {
        let package_manager = if check_command("brew", &["--version"]).await {
            "brew"
        } else if check_command("apt-get", &["--version"]).await {
            "apt-get"
        } else if check_command("dnf", &["--version"]).await {
            "dnf"
        } else if check_command("pacman", &["--version"]).await {
            "pacman"
        } else {
            return Err("No supported package manager found. Install Homebrew, apt, dnf, or pacman, or choose Docker.".to_string());
        };
        for service in services {
            let package = match *service {
                "mysql" => mysql::binary_package(package_manager),
                "redis" => redis::binary_package(package_manager),
                "elasticsearch" => elasticsearch::binary_package(package_manager),
                "weaviate" => weaviate::binary_package(package_manager),
                "rustfs" => rustfs::binary_package(package_manager),
                _ => unreachable!(),
            }?;
            run_package_manager_install(package_manager, package, service).await?;
        }
        Ok(())
    }
}

async fn run_package_manager_install(
    package_manager: &str,
    package: &str,
    service: &str,
) -> Result<(), String> {
    let mut command = tokio::process::Command::new(package_manager);
    match package_manager {
        "brew" => {
            command.args(["install", package]);
        }
        "apt-get" => {
            command.args(["install", "-y", package]);
        }
        "dnf" => {
            command.args(["install", "-y", package]);
        }
        "pacman" => {
            command.args(["-S", "--noconfirm", package]);
        }
        _ => {}
    }
    let output = command
        .output()
        .await
        .map_err(|err| format!("Failed to start {package_manager}: {err}"))?;
    if !output.status.success() {
        return Err(format!("Failed to install {service}. Run the required package-manager command with administrator privileges, then retry using '使用现有配置'."));
    }
    Ok(())
}

pub(crate) async fn check_command(program: &str, args: &[&str]) -> bool {
    match tokio::process::Command::new(program).args(args).output().await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

fn detailed_compose_path() -> PathBuf {
    zihuan_core::system_config::application_data_dir().join("detailed-compose.yaml")
}

fn compose_bind_source(data_dir: &str) -> String {
    let data_dir = data_dir.trim();
    if data_dir.starts_with('.') || Path::new(data_dir).is_absolute() {
        return data_dir.to_string();
    }
    format!("./{data_dir}")
}

fn resolve_compose_data_dir(compose_dir: &Path, data_dir: &str) -> PathBuf {
    let bind_source = compose_bind_source(data_dir);
    let path = Path::new(&bind_source);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    compose_dir.join(path.strip_prefix(".").unwrap_or(path))
}

fn docker_data_directory_command(config: &DetailedSetupConfig) -> String {
    let data_dirs = detailed_data_dirs(config);
    if data_dirs.is_empty() {
        return String::new();
    }
    let data_dirs = data_dirs
        .iter()
        .map(|data_dir| shell_quote(&compose_bind_source(data_dir)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("mkdir -p -- {data_dirs}")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\\"'\\\"'"))
}

pub(crate) fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(crate) fn compose_bind_mount(data_dir: &str, container_path: &str) -> String {
    yaml_quote(&format!("{}:{container_path}", compose_bind_source(data_dir)))
}

pub(crate) async fn save_detailed_connections(config: &DetailedSetupConfig) -> Result<(), String> {
    verify_detailed_connections(config).await?;
    mysql::save_connection(config).await?;
    rustfs::save_connection(config)?;
    redis::save_connection(config)?;
    if config.search.enabled {
        save_search_connection(config).await?;
    }
    Ok(())
}

pub(crate) async fn save_search_connection(
    config: &DetailedSetupConfig,
) -> Result<ConnectionConfig, String> {
    let id = format!("setup-detailed-{}", config.search.search_type);
    let name = config.search.search_type.clone();
    let kind = if config.search.search_type == "elasticsearch" {
        elasticsearch::connection_kind(config.search.base_url.clone(), &config.search)
    } else {
        weaviate::connection_kind(config.search.base_url.clone(), &config.search)
    };
    let initialization_kind = kind.clone();

    // Index setup uses blocking HTTP clients; keep their runtime lifetime
    // entirely inside the blocking pool rather than an async worker.
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        match initialization_kind {
            ConnectionKind::Elasticsearch(elasticsearch) => {
                elasticsearch::ensure_indices(elasticsearch)
            }
            ConnectionKind::Weaviate(weaviate) => weaviate::ensure_collections(weaviate),
            _ => Ok(()),
        }
    })
    .await
    .map_err(|err| format!("search database initialization task failed: {err}"))??;
    let connection = config_factory::build_connection(&id, &name, kind);
    config_factory::save_connection(connection.clone())?;
    Ok(connection)
}

pub(crate) async fn verify_detailed_connections(
    config: &DetailedSetupConfig,
) -> Result<(), String> {
    mysql::verify(config).await?;
    rustfs::verify(config).await?;
    weaviate::verify(config).await?;
    elasticsearch::verify(config).await?;
    redis::verify(config).await?;
    Ok(())
}

pub(crate) fn endpoint_host_port(value: &str, default_port: u16) -> Result<(String, u16), String> {
    let without_scheme = value.split_once("://").map(|(_, rest)| rest).unwrap_or(value);
    let authority = without_scheme
        .split('/')
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default();
    if authority.is_empty() {
        return Err(format!("Invalid endpoint: {value}"));
    }
    if let Some((host, port)) = authority.rsplit_once(':') {
        return Ok((
            host.to_string(),
            port.parse::<u16>().map_err(|_| format!("Invalid endpoint port: {value}"))?,
        ));
    }
    Ok((authority.to_string(), default_port))
}

pub(crate) async fn wait_for_tcp(
    host: &str,
    port: u16,
    service: &str,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if tokio::net::TcpStream::connect((host, port)).await.is_ok() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(format!(
        "Could not connect to {service} at {host}:{port} within {} seconds. Ensure the service is running and the address and port are correct.",
        timeout.as_secs()
    ))
}
