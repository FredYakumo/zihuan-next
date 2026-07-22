use serde::{Deserialize, Serialize};
use sqlx::Connection;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::broadcast;

use zihuan_core::setup_wizard::{load_setup_wizard_state, save_setup_wizard_state};
use storage_handler::{
    ensure_collection_schema, ensure_elasticsearch_index, ConnectionConfig, ConnectionKind, ElasticsearchConnection, ElasticsearchRef,
    MysqlConnection, RedisConnection, RustfsConnection, SqliteConnection, WeaviateConnection,
};
use zihuan_core::weaviate::{WeaviateCollectionSchema, WeaviateRef};

pub mod config_factory;

#[derive(Serialize, Clone)]
pub struct SetupProgressEvent {
    pub step: String,
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct SetupOrchestrator {
    progress_tx: broadcast::Sender<SetupProgressEvent>,
    #[allow(dead_code)]
    task_id: String,
}

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

impl SetupOrchestrator {
    pub fn new(task_id: String, progress_tx: broadcast::Sender<SetupProgressEvent>) -> Self {
        Self { progress_tx, task_id }
    }

    pub async fn run(
        &self,
        role: SetupRole,
        options: SetupOptions,
        llm_config: LlmSetupConfig,
        ims_bot_adapter_config: Option<ImsBotAdapterSetupConfig>,
    ) -> Result<(), String> {
        self.emit("detecting_environment", "running", "Detecting environment...", Some(5));

        let env = detect_environment().await;
        self.emit(
            "detecting_environment",
            "success",
            &format!("OS: {}, Docker: {}", env.os, env.docker_available),
            Some(10),
        );

        let required_services = match &role {
            SetupRole::ChatAssistant | SetupRole::CodeDevAssistant => Vec::new(),
            SetupRole::QqChatBot => vec!["redis", "weaviate", "rustfs", "napcat"],
            SetupRole::AiButler => Vec::new(),
        };

        // Track native NapCat install path so we can persist it in the bot adapter config.
        let mut napcat_native_path: Option<String> = None;

        // Use auto-detected proxy as fallback when none was explicitly provided.
        let effective_proxy = options.http_proxy.as_deref().or(env.proxy.as_deref());

        if let Some(proxy) = effective_proxy {
            self.emit("detecting_environment", "success", &format!("Using proxy: {proxy}"), Some(12));
        }

        if !required_services.is_empty() {
            self.emit("installing_dependencies", "running", "Installing dependencies...", Some(15));

            if env.docker_available {
                for (i, service) in required_services.iter().enumerate() {
                    let pct = 15 + ((i + 1) as u8 * 50 / required_services.len() as u8);
                    self.emit(
                        &format!("installing_{service}"),
                        "running",
                        &format!("Starting {service} via Docker..."),
                        Some(pct),
                    );

                    match run_docker_compose_for_service(service, effective_proxy).await {
                        Ok(_) => {
                            self.emit(
                                &format!("installing_{service}"),
                                "success",
                                &format!("{service} started successfully"),
                                Some(pct),
                            );
                        }
                        Err(e) => {
                            self.emit(
                                &format!("installing_{service}"),
                                "error",
                                &format!("Failed to start {service}: {e}"),
                                Some(pct),
                            );
                            return Err(e);
                        }
                    }
                }
            } else {
                self.emit(
                    "installing_dependencies",
                    "running",
                    "Docker not available, installing dependencies natively...",
                    Some(15),
                );

                for (i, service) in required_services.iter().enumerate() {
                    let pct = 15 + ((i + 1) as u8 * 50 / required_services.len() as u8);
                    if *service == "napcat" {
                        let qq_id = ims_bot_adapter_config.as_ref().and_then(|c| c.qq_id.as_deref());
                        self.emit(
                            "installing_napcat",
                            "running",
                            "Downloading and installing NapCat...",
                            Some(pct - 5),
                        );
                        match install_napcat_native(effective_proxy, qq_id).await {
                            Ok(install_path) => {
                                napcat_native_path = Some(install_path.clone());
                                self.emit(
                                    "installing_napcat",
                                    "success",
                                    &format!("NapCat installed at {install_path}"),
                                    Some(pct),
                                );
                            }
                            Err(e) => {
                                self.emit(
                                    "installing_napcat",
                                    "error",
                                    &format!("Failed to install NapCat: {e}"),
                                    Some(pct),
                                );
                                return Err(e);
                            }
                        }
                    } else {
                        self.emit(
                            &format!("installing_{service}"),
                            "skipped",
                            &format!("{service} requires Docker; please install Docker for full functionality"),
                            Some(pct),
                        );
                    }
                }
            }

            self.emit(
                "installing_dependencies",
                "success",
                "Dependency installation complete",
                Some(65),
            );
        }

        self.emit("creating_configs", "running", "Creating system configurations...", Some(70));

        match self
            .create_configs(
                &role,
                &llm_config,
                ims_bot_adapter_config.as_ref(),
                napcat_native_path.as_deref(),
            )
            .await
        {
            Ok(_) => {
                self.emit("creating_configs", "success", "Configurations created successfully", Some(90));
            }
            Err(e) => {
                self.emit("creating_configs", "error", &format!("Failed to create configs: {e}"), Some(90));
                return Err(e);
            }
        }

        self.emit("finished", "success", "Setup complete!", Some(100));

        let mut state = load_setup_wizard_state().unwrap_or_default();
        state.completed = true;
        state.mode = Some(format!("role_{:?}", role).to_lowercase());
        state.completed_at = Some(chrono::Utc::now().to_rfc3339());
        state.last_step = Some("finished".to_string());
        let _ = save_setup_wizard_state(&state);

        Ok(())
    }

    async fn create_configs(
        &self,
        role: &SetupRole,
        llm_config: &LlmSetupConfig,
        ims_bot_adapter_config: Option<&ImsBotAdapterSetupConfig>,
        napcat_native_path: Option<&str>,
    ) -> Result<(), String> {
        match role {
            SetupRole::ChatAssistant => config_factory::create_chat_assistant_stack(llm_config).await,
            SetupRole::CodeDevAssistant => {
                config_factory::create_workspace_agent_service_stack(llm_config, "Project Dev Assistant").await
            }
            SetupRole::QqChatBot => {
                let ims_config =
                    ims_bot_adapter_config.ok_or("IMS Bot Adapter configuration is required for QQ Chat Bot")?;
                config_factory::create_qq_bot_stack(llm_config, ims_config, napcat_native_path).await
            }
            SetupRole::AiButler => config_factory::create_butler_stack(llm_config).await,
        }
    }

    pub async fn run_detailed(&self, config: DetailedSetupConfig) -> Result<(), String> {
        self.emit("validating_detailed_config", "running", "Validating component configuration...", Some(5));
        validate_detailed_config(&config)?;

        let install_services = detailed_install_services(&config);
        if !install_services.is_empty() {
            self.emit("installing_dependencies", "running", "Installing selected components...", Some(15));
            match config.install_method {
                DetailedInstallMethod::Docker => run_detailed_docker(&config, &install_services).await?,
                DetailedInstallMethod::Binary => run_detailed_binary(&config, &install_services).await?,
            }
            self.emit("installing_dependencies", "success", "Selected components are running", Some(55));
        }

        self.emit("verifying_connections", "running", "Verifying component connections...", Some(65));
        save_detailed_connections(&config).await?;
        self.emit("verifying_connections", "success", "Component connections saved", Some(90));

        let mut state = load_setup_wizard_state().unwrap_or_default();
        state.completed = true;
        state.mode = Some("detailed".to_string());
        state.completed_at = Some(chrono::Utc::now().to_rfc3339());
        state.last_step = Some("finished".to_string());
        save_setup_wizard_state(&state).map_err(|err| err.to_string())?;
        self.emit("finished", "success", "Detailed setup complete!", Some(100));
        Ok(())
    }

    pub fn emit(&self, step: &str, status: &str, message: &str, progress_percent: Option<u8>) {
        let event = SetupProgressEvent {
            step: step.to_string(),
            status: status.to_string(),
            message: message.to_string(),
            progress_percent,
            error: if status == "error" {
                Some(message.to_string())
            } else {
                None
            },
        };
        let _ = self.progress_tx.send(event);
    }
}

fn validate_detailed_config(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.target_machine_address.trim().is_empty() {
        return Err("Target machine address is required".to_string());
    }
    if !config.relational.enabled && !config.rustfs.enabled && !config.search.enabled && !config.redis.enabled {
        return Err("Select at least one component to configure".to_string());
    }
    if config.relational.enabled && config.relational.database_type == "mysql" {
        if config.relational.host.trim().is_empty() || config.relational.database.trim().is_empty() {
            return Err("MySQL host and database are required".to_string());
        }
        if config.relational.source == DetailedComponentSource::Install && config.relational.password.is_empty() {
            return Err("A MySQL root password is required when installing MySQL".to_string());
        }
    }
    if config.relational.enabled && config.relational.database_type == "sqlite" && config.relational.sqlite_path.trim().is_empty() {
        return Err("SQLite database path is required".to_string());
    }
    if config.rustfs.enabled && (config.rustfs.endpoint.trim().is_empty() || config.rustfs.bucket.trim().is_empty()) {
        return Err("RustFS endpoint and bucket are required".to_string());
    }
    if config.search.enabled && config.search.base_url.trim().is_empty() {
        return Err("Search database Base URL is required".to_string());
    }
    if config.search.enabled && config.search.search_type == "elasticsearch" && config.search.password.as_deref().unwrap_or_default().is_empty() {
        return Err("An Elasticsearch password is required".to_string());
    }
    if config.redis.enabled && config.redis.url.trim().is_empty() {
        return Err("Redis URL is required".to_string());
    }
    if config.expose_public_access {
        if config.relational.enabled
            && config.relational.database_type == "mysql"
            && config.relational.password.is_empty()
        {
            return Err("A MySQL password is required when exposing public access".to_string());
        }
        if config.rustfs.enabled
            && (config.rustfs.access_key.is_empty() || config.rustfs.secret_key.is_empty())
        {
            return Err("RustFS access and secret keys are required when exposing public access".to_string());
        }
        if config.search.enabled {
            if config.search.search_type == "elasticsearch" && config.search.password.as_deref().unwrap_or_default().is_empty() {
                return Err("An Elasticsearch password is required when exposing public access".to_string());
            }
            if config.search.search_type == "weaviate" && config.search.api_key.as_deref().unwrap_or_default().is_empty() {
                return Err("A Weaviate API key is required when exposing public access".to_string());
            }
        }
        if config.redis.enabled && config.redis.password.as_deref().unwrap_or_default().is_empty() {
            return Err("A Redis password is required when exposing public access".to_string());
        }
    }
    Ok(())
}

pub fn generate_detailed_install_command(config: &DetailedSetupConfig) -> Result<DetailedInstallCommand, String> {
    validate_detailed_config(config)?;

    let compose = detailed_compose(config);
    let install_command = match &config.install_method {
        DetailedInstallMethod::Docker => docker_install_command(&compose),
        DetailedInstallMethod::Binary => binary_install_command(config, &compose),
    };

    Ok(DetailedInstallCommand {
        install_command,
        connections: detailed_connection_configs(config),
    })
}

fn docker_install_command(compose: &str) -> String {
    let compose_base64 = base64_encode(compose.as_bytes());
    format!(
        "# Run on the target Linux machine\nmkdir -p ~/zihuan-next-install && cd ~/zihuan-next-install\nprintf '%s' '{compose_base64}' | base64 -d > docker-compose.yaml\ndocker compose -f docker-compose.yaml up -d"
    )
}

fn binary_install_command(config: &DetailedSetupConfig, compose: &str) -> String {
    let compose_base64 = base64_encode(compose.as_bytes());
    let services = detailed_install_services(config).join(" ");
    format!(
        "# Run on the target Linux x86_64 machine\nsudo apt-get update\nsudo apt-get install -y build-essential pkg-config libssl-dev git nodejs npm docker.io docker-compose-plugin\ngit clone --recurse-submodules https://github.com/FredYakumo/zihuan-next.git ~/zihuan-next\ncd ~/zihuan-next\ncorepack enable && corepack prepare pnpm@10.22.0 --activate\ncd webui && pnpm install --frozen-lockfile && pnpm run build\ncd .. && cargo build --release\nmkdir -p ~/zihuan-next-install && cd ~/zihuan-next-install\nprintf '%s' '{compose_base64}' | base64 -d > docker-compose.yaml\ndocker compose -f docker-compose.yaml up -d {services}\n# Start Zihuan Next after importing the generated connections JSON:\n~/zihuan-next/target/release/zihuan_next --host 0.0.0.0 --port 9951"
    )
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[((first & 0b0000_0011) << 4 | second >> 4) as usize] as char);
        output.push(if chunk.len() > 1 { TABLE[((second & 0b0000_1111) << 2 | third >> 6) as usize] as char } else { '=' });
        output.push(if chunk.len() > 2 { TABLE[(third & 0b0011_1111) as usize] as char } else { '=' });
    }
    output
}

fn detailed_connection_host(config: &DetailedSetupConfig) -> &str {
    if config.expose_public_access {
        config.target_machine_address.trim()
    } else {
        "127.0.0.1"
    }
}

fn detailed_connection_configs(config: &DetailedSetupConfig) -> Vec<ConnectionConfig> {
    let host = detailed_connection_host(config);
    let mut connections = Vec::new();

    if config.relational.enabled {
        let connection = if config.relational.database_type == "sqlite" {
            config_factory::build_connection(
                "setup-detailed-sqlite",
                "SQLite",
                ConnectionKind::Sqlite(SqliteConnection { path: config.relational.sqlite_path.clone() }),
            )
        } else {
            let url = format!(
                "mysql://{}:{}@{}:{}/{}",
                config.relational.username,
                config.relational.password,
                host,
                config.relational.deployment.port,
                config.relational.database,
            );
            config_factory::build_connection(
                "setup-detailed-mysql",
                "MySQL",
                ConnectionKind::Mysql(MysqlConnection {
                    url,
                    max_connections: config.relational.max_connections,
                    acquire_timeout_secs: config.relational.acquire_timeout_secs,
                }),
            )
        };
        connections.push(connection);
    }
    if config.rustfs.enabled {
        connections.push(config_factory::build_connection(
            "setup-detailed-rustfs",
            "RustFS",
            ConnectionKind::Rustfs(RustfsConnection {
                endpoint: format!("http://{}:{}", host, config.rustfs.deployment.port),
                bucket: config.rustfs.bucket.clone(),
                region: config.rustfs.region.clone(),
                access_key: config.rustfs.access_key.clone(),
                secret_key: config.rustfs.secret_key.clone(),
                public_base_url: config.rustfs.public_base_url.clone(),
                path_style: config.rustfs.path_style,
            }),
        ));
    }
    if config.redis.enabled {
        connections.push(config_factory::build_connection(
            "setup-detailed-redis",
            "Redis",
            ConnectionKind::Redis(RedisConnection {
                url: format!("redis://{}:{}", host, config.redis.deployment.port),
                username: config.redis.username.clone(),
                password: config.redis.password.clone(),
            }),
        ));
    }
    if config.search.enabled {
        for (suffix, schema) in [("memory", WeaviateCollectionSchema::AgentMemory), ("image", WeaviateCollectionSchema::ImageSemantic)] {
            let id = format!("setup-detailed-{}-{suffix}", config.search.search_type);
            let name = format!("{} {suffix}", config.search.search_type);
            let kind = if config.search.search_type == "elasticsearch" {
                ConnectionKind::Elasticsearch(ElasticsearchConnection {
                    base_url: format!("http://{}:{}", host, config.search.deployment.port),
                    index_name: format!("zihuan_{suffix}"),
                    username: config.search.username.clone(),
                    password: config.search.password.clone(),
                    api_key: config.search.api_key.clone(),
                    collection_schema: schema,
                    vector_dimensions: config.search.vector_dimensions,
                })
            } else {
                ConnectionKind::Weaviate(WeaviateConnection {
                    base_url: format!("http://{}:{}", host, config.search.deployment.port),
                    class_name: if suffix == "memory" { "AgentMemory".to_string() } else { "ImageSemantic".to_string() },
                    username: config.search.username.clone(),
                    password: config.search.password.clone(),
                    api_key: config.search.api_key.clone(),
                    collection_schema: schema,
                })
            };
            connections.push(config_factory::build_connection(&id, &name, kind));
        }
    }
    connections
}

fn detailed_install_services(config: &DetailedSetupConfig) -> Vec<&'static str> {
    let mut services = Vec::new();
    if config.relational.enabled && config.relational.source == DetailedComponentSource::Install && config.relational.database_type == "mysql" { services.push("mysql"); }
    if config.rustfs.enabled && config.rustfs.source == DetailedComponentSource::Install { services.push("rustfs"); }
    if config.search.enabled && config.search.source == DetailedComponentSource::Install { services.push(if config.search.search_type == "elasticsearch" { "elasticsearch" } else { "weaviate" }); }
    if config.redis.enabled && config.redis.source == DetailedComponentSource::Install { services.push("redis"); }
    services
}

async fn run_detailed_docker(config: &DetailedSetupConfig, services: &[&str]) -> Result<(), String> {
    if !check_command("docker", &["compose", "version"]).await {
        return Err("Docker Compose is unavailable. Install Docker Desktop or Docker Compose, then retry.".to_string());
    }
    let compose_path = detailed_compose_path();
    if let Some(parent) = compose_path.parent() { tokio::fs::create_dir_all(parent).await.map_err(|err| err.to_string())?; }
    tokio::fs::write(&compose_path, detailed_compose(config)).await.map_err(|err| err.to_string())?;
    let output = tokio::process::Command::new("docker")
        .arg("compose").arg("-f").arg(&compose_path).arg("up").arg("-d").args(services)
        .output().await.map_err(|err| format!("Failed to run Docker Compose: {err}"))?;
    if !output.status.success() { return Err(String::from_utf8_lossy(&output.stderr).trim().to_string()); }
    Ok(())
}

async fn run_detailed_binary(_config: &DetailedSetupConfig, services: &[&str]) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let output = tokio::process::Command::new("wsl").args(["--status"]).output().await;
        if !matches!(output, Ok(ref result) if result.status.success()) {
            return Err("Native installation on Windows requires WSL. Install WSL, then retry, or choose Docker.".to_string());
        }
        return Err(format!("WSL was detected. Run the setup from your WSL distribution to install: {}", services.join(", ")));
    }
    #[cfg(not(target_os = "windows"))]
    {
        let package_manager = if check_command("brew", &["--version"]).await { "brew" }
            else if check_command("apt-get", &["--version"]).await { "apt-get" }
            else if check_command("dnf", &["--version"]).await { "dnf" }
            else if check_command("pacman", &["--version"]).await { "pacman" }
            else { return Err("No supported package manager found. Install Homebrew, apt, dnf, or pacman, or choose Docker.".to_string()); };
        for service in services {
            let package = match (*service, package_manager) {
                ("mysql", "brew") => "mysql", ("mysql", _) => "mysql-server",
                ("redis", _) => "redis", ("elasticsearch", _) => "elasticsearch",
                ("weaviate", _) | ("rustfs", _) => return Err(format!("{service} is unavailable from {package_manager}; use Docker or install its official release before choosing '使用现有配置'.")),
                _ => unreachable!(),
            };
            let mut command = tokio::process::Command::new(package_manager);
            match package_manager { "brew" => { command.args(["install", package]); }, "apt-get" => { command.args(["install", "-y", package]); }, "dnf" => { command.args(["install", "-y", package]); }, "pacman" => { command.args(["-S", "--noconfirm", package]); }, _ => {} }
            let output = command.output().await.map_err(|err| format!("Failed to start {package_manager}: {err}"))?;
            if !output.status.success() { return Err(format!("Failed to install {service}. Run the required package-manager command with administrator privileges, then retry using '使用现有配置'.")); }
        }
        Ok(())
    }
}

fn detailed_compose_path() -> PathBuf { zihuan_core::system_config::app_data_dir().join("zihuan-next_aibot").join("detailed-compose.yaml") }

fn detailed_compose(config: &DetailedSetupConfig) -> String {
    let mut services = String::from("services:\n");
    if config.relational.enabled && config.relational.source == DetailedComponentSource::Install && config.relational.database_type == "mysql" {
        let d = &config.relational.deployment;
        services.push_str(&format!("  mysql:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:3306\"]\n    volumes: [\"{}:/var/lib/mysql\"]\n    environment:\n      MYSQL_ROOT_PASSWORD: {}\n      MYSQL_DATABASE: {}\n", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, yaml_quote(&d.data_dir), yaml_quote(&config.relational.password), yaml_quote(&config.relational.database)));
    }
    if config.rustfs.enabled && config.rustfs.source == DetailedComponentSource::Install { let d = &config.rustfs.deployment; services.push_str(&format!("  rustfs:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:9000\"]\n    volumes: [\"{}:/data\"]\n    environment:\n      RUSTFS_ACCESS_KEY: {}\n      RUSTFS_SECRET_KEY: {}\n    command: [\"--console-enable\", \"/data\"]\n", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, yaml_quote(&d.data_dir), yaml_quote(&config.rustfs.access_key), yaml_quote(&config.rustfs.secret_key))); }
    if config.search.enabled && config.search.source == DetailedComponentSource::Install {
        let d = &config.search.deployment;
        if config.search.search_type == "elasticsearch" { services.push_str(&format!("  elasticsearch:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:9200\"]\n    volumes: [\"{}:/usr/share/elasticsearch/data\"]\n    environment:\n      discovery.type: single-node\n      xpack.security.enabled: 'true'\n      ELASTIC_PASSWORD: {}\n", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, yaml_quote(&d.data_dir), yaml_quote(config.search.password.as_deref().unwrap_or_default()))); }
        else {
            let authentication = if config.expose_public_access {
                format!("      AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED: 'false'\n      AUTHENTICATION_APIKEY_ENABLED: 'true'\n      AUTHENTICATION_APIKEY_ALLOWED_KEYS: {}\n", yaml_quote(config.search.api_key.as_deref().unwrap_or_default()))
            } else {
                "      AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED: 'true'\n".to_string()
            };
            services.push_str(&format!("  weaviate:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:8080\"]\n    volumes: [\"{}:/var/lib/weaviate\"]\n    environment:\n{}      DEFAULT_VECTORIZER_MODULE: none\n      CLUSTER_HOSTNAME: node1\n", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, yaml_quote(&d.data_dir), authentication));
        }
    }
    if config.redis.enabled && config.redis.source == DetailedComponentSource::Install {
        let d = &config.redis.deployment;
        let command = if config.expose_public_access { format!("    command: [\"redis-server\", \"--requirepass\", {}]\n", yaml_quote(config.redis.password.as_deref().unwrap_or_default())) } else { String::new() };
        services.push_str(&format!("  redis:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:6379\"]\n    volumes: [\"{}:/data\"]\n{}", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, yaml_quote(&d.data_dir), command));
    }
    services
}

fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn save_detailed_connections(config: &DetailedSetupConfig) -> Result<(), String> {
    verify_detailed_connections(config).await?;
    if config.relational.enabled {
        if config.relational.database_type == "sqlite" {
            let path = Path::new(&config.relational.sqlite_path);
            if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
                tokio::fs::create_dir_all(parent).await.map_err(|err| err.to_string())?;
            }
            let mut connection = sqlx::SqliteConnection::connect(&format!("sqlite://{}?mode=rwc", path.display()))
                .await.map_err(|err| format!("Failed to create SQLite database: {err}"))?;
            zihuan_core::database::ensure_tables_sqlite(&mut connection)
                .await.map_err(|err| format!("Failed to initialize SQLite database: {err}"))?;
            config_factory::save_connection(config_factory::build_connection("setup-detailed-sqlite", "SQLite", ConnectionKind::Sqlite(SqliteConnection { path: config.relational.sqlite_path.clone() })))?;
        } else {
            let url = format!("mysql://{}:{}@{}:{}/{}", config.relational.username, config.relational.password, config.relational.host, config.relational.deployment.port, config.relational.database);
            config_factory::save_connection(config_factory::build_connection("setup-detailed-mysql", "MySQL", ConnectionKind::Mysql(MysqlConnection { url, max_connections: config.relational.max_connections, acquire_timeout_secs: config.relational.acquire_timeout_secs })))?;
        }
    }
    if config.rustfs.enabled { config_factory::save_connection(config_factory::build_connection("setup-detailed-rustfs", "RustFS", ConnectionKind::Rustfs(RustfsConnection { endpoint: config.rustfs.endpoint.clone(), bucket: config.rustfs.bucket.clone(), region: config.rustfs.region.clone(), access_key: config.rustfs.access_key.clone(), secret_key: config.rustfs.secret_key.clone(), public_base_url: config.rustfs.public_base_url.clone(), path_style: config.rustfs.path_style })))?; }
    if config.redis.enabled { config_factory::save_connection(config_factory::build_connection("setup-detailed-redis", "Redis", ConnectionKind::Redis(RedisConnection { url: config.redis.url.clone(), username: config.redis.username.clone(), password: config.redis.password.clone() })))?; }
    if config.search.enabled {
        for (suffix, schema) in [("memory", WeaviateCollectionSchema::AgentMemory), ("image", WeaviateCollectionSchema::ImageSemantic)] {
            let id = format!("setup-detailed-{}-{suffix}", config.search.search_type);
            let name = format!("{} {}", config.search.search_type, suffix);
            let kind = if config.search.search_type == "elasticsearch" { ConnectionKind::Elasticsearch(ElasticsearchConnection { base_url: config.search.base_url.clone(), index_name: format!("zihuan_{suffix}"), username: config.search.username.clone(), password: config.search.password.clone(), api_key: config.search.api_key.clone(), collection_schema: schema, vector_dimensions: config.search.vector_dimensions }) } else { ConnectionKind::Weaviate(WeaviateConnection { base_url: config.search.base_url.clone(), class_name: if suffix == "memory" { "AgentMemory".to_string() } else { "ImageSemantic".to_string() }, username: config.search.username.clone(), password: config.search.password.clone(), api_key: config.search.api_key.clone(), collection_schema: schema }) };
            match &kind {
                ConnectionKind::Elasticsearch(elasticsearch) => {
                    let reference = ElasticsearchRef::new(elasticsearch.clone()).map_err(|err| err.to_string())?;
                    ensure_elasticsearch_index(&reference, true).map_err(|err| err.to_string())?;
                }
                ConnectionKind::Weaviate(weaviate) => {
                    let reference = WeaviateRef::new(
                        weaviate.base_url.clone(), weaviate.class_name.clone(), weaviate.username.clone(),
                        weaviate.password.clone(), weaviate.api_key.clone(), Duration::from_secs(30),
                    ).map_err(|err| err.to_string())?;
                    ensure_collection_schema(&reference, schema, true).map_err(|err| err.to_string())?;
                }
                _ => {}
            }
            config_factory::save_connection(config_factory::build_connection(&id, &name, kind))?;
        }
    }
    Ok(())
}

async fn verify_detailed_connections(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.relational.enabled && config.relational.database_type == "mysql" {
        wait_for_tcp(&config.relational.host, config.relational.deployment.port, "MySQL").await?;
    }
    if config.rustfs.enabled {
        let (host, port) = endpoint_host_port(&config.rustfs.endpoint, 9000)?;
        wait_for_tcp(&host, port, "RustFS").await?;
    }
    if config.search.enabled {
        let default_port = if config.search.search_type == "elasticsearch" { 9200 } else { 8080 };
        let (host, port) = endpoint_host_port(&config.search.base_url, default_port)?;
        wait_for_tcp(&host, port, "search database").await?;
    }
    if config.redis.enabled {
        let (host, port) = endpoint_host_port(&config.redis.url, 6379)?;
        wait_for_tcp(&host, port, "Redis").await?;
    }
    Ok(())
}

fn endpoint_host_port(value: &str, default_port: u16) -> Result<(String, u16), String> {
    let without_scheme = value.split_once("://").map(|(_, rest)| rest).unwrap_or(value);
    let authority = without_scheme.split('/').next().unwrap_or_default().rsplit('@').next().unwrap_or_default();
    if authority.is_empty() { return Err(format!("Invalid endpoint: {value}")); }
    if let Some((host, port)) = authority.rsplit_once(':') {
        return Ok((host.to_string(), port.parse::<u16>().map_err(|_| format!("Invalid endpoint port: {value}"))?));
    }
    Ok((authority.to_string(), default_port))
}

async fn wait_for_tcp(host: &str, port: u16, service: &str) -> Result<(), String> {
    for _ in 0..15 {
        if tokio::net::TcpStream::connect((host, port)).await.is_ok() { return Ok(()); }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(format!("Could not connect to {service} at {host}:{port}. Start the service and retry using '使用现有配置'."))
}

#[derive(Clone, Deserialize, Default)]
pub struct SetupOptions {
    #[serde(default)]
    pub http_proxy: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub docker_compose_path: Option<String>,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum SetupRole {
    ChatAssistant,
    CodeDevAssistant,
    QqChatBot,
    AiButler,
}

#[derive(Clone, Deserialize)]
pub struct LlmSetupConfig {
    pub mode: String,
    pub model_name: String,
    #[serde(default)]
    pub model_id: Option<String>,
    pub api_endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_style: String,
}

#[derive(Clone, Deserialize)]
pub struct ImsBotAdapterSetupConfig {
    pub platform: String,
    pub ws_url: String,
    #[serde(default)]
    pub qq_id: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Serialize)]
pub struct EnvironmentInfo {
    pub os: String,
    pub os_detail: String,
    pub docker_available: bool,
    pub docker_compose_available: bool,
    pub binary_install_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binary_install_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wsl_available: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wsl_docker_available: Option<bool>,
    pub cuda_version: Option<String>,
    pub compiler_version: Option<String>,
    pub proxy: Option<String>,
    pub services: Vec<ServiceDetectionResult>,
}

#[derive(Serialize)]
pub struct ServiceDetectionResult {
    pub service: String,
    pub detected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_test_result: Option<String>,
}

pub async fn detect_environment() -> EnvironmentInfo {
    let os = std::env::consts::OS.to_string();
    let os_detail = detailed_os_name().await;
    let docker_available = check_command("docker", &["--version"]).await;
    let docker_compose_available = check_command("docker", &["compose", "version"]).await;
    let (binary_install_available, binary_install_reason) = detect_binary_installation_support().await;
    let (wsl_available, wsl_docker_available) = detect_windows_wsl().await;
    let cuda_version = detect_cuda_version().await;
    let compiler_version = detect_compiler_version().await;
    let proxy = detect_proxy().await;

    let services = vec![
        check_service_port("redis", "127.0.0.1", 6379).await,
        check_service_port("weaviate", "127.0.0.1", 8080).await,
        check_service_port("rustfs", "127.0.0.1", 9000).await,
        check_service_port("mysql", "127.0.0.1", 3306).await,
        check_service_port("napcat", "127.0.0.1", 3001).await,
    ];

    EnvironmentInfo {
        os,
        os_detail,
        docker_available,
        docker_compose_available,
        binary_install_available,
        binary_install_reason,
        wsl_available,
        wsl_docker_available,
        cuda_version,
        compiler_version,
        proxy,
        services,
    }
}

#[cfg(target_os = "windows")]
async fn detect_binary_installation_support() -> (bool, Option<String>) {
    (
        false,
        Some("Windows 上的二进制安装需在 WSL 发行版内运行本程序".to_string()),
    )
}

#[cfg(not(target_os = "windows"))]
async fn detect_binary_installation_support() -> (bool, Option<String>) {
    for package_manager in ["brew", "apt-get", "dnf", "pacman"] {
        if check_command(package_manager, &["--version"]).await {
            return (true, None);
        }
    }

    (
        false,
        Some("未检测到 Homebrew、apt、dnf 或 pacman 包管理器".to_string()),
    )
}

#[cfg(target_os = "windows")]
async fn detect_windows_wsl() -> (Option<bool>, Option<bool>) {
    let wsl_available = check_command("wsl", &["--status"]).await;
    let wsl_docker_available = if wsl_available {
        check_command("wsl", &["docker", "compose", "version"]).await
    } else {
        false
    };
    (Some(wsl_available), Some(wsl_docker_available))
}

#[cfg(not(target_os = "windows"))]
async fn detect_windows_wsl() -> (Option<bool>, Option<bool>) {
    (None, None)
}

async fn detect_cuda_version() -> Option<String> {
    if let Some(out) = command_output("nvcc", &["--version"]).await {
        for line in out.lines() {
            if let Some(idx) = line.find("release ") {
                let rest = &line[idx + 8..];
                let ver = rest.split(',').next().unwrap_or(rest).trim();
                return Some(format!("CUDA {}", ver));
            }
        }
        return out.lines().next().map(|s| s.trim().to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let cuda_path = std::path::PathBuf::from("C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA");
        if let Ok(mut entries) = tokio::fs::read_dir(&cuda_path).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                if let Ok(ft) = entry.file_type().await {
                    if ft.is_dir() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with('v') {
                            return Some(format!("CUDA {}", &name[1..]));
                        }
                        return Some(format!("CUDA {}", name));
                    }
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if std::path::PathBuf::from("/usr/local/cuda").exists() {
            return Some("CUDA (version unknown)".to_string());
        }
    }
    None
}

async fn detect_compiler_version() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(out) = command_output("cl", &[]).await {
            let line = out.lines().next()?.trim();
            return Some(format!("MSVC {}", line));
        }
        if let Some(out) = command_output("gcc", &["--version"]).await {
            let line = out.lines().next()?.trim();
            return Some(format!("GCC {}", line));
        }
        if let Some(out) = command_output("clang", &["--version"]).await {
            let line = out.lines().next()?.trim();
            return Some(format!("Clang {}", line));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(out) = command_output("gcc", &["--version"]).await {
            let line = out.lines().next()?.trim();
            return Some(format!("GCC {}", line));
        }
        if let Some(out) = command_output("clang", &["--version"]).await {
            let line = out.lines().next()?.trim();
            return Some(format!("Clang {}", line));
        }
    }
    None
}

async fn command_output(program: &str, args: &[&str]) -> Option<String> {
    match tokio::process::Command::new(program).args(args).output().await {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout);
            Some(text.to_string())
        }
        _ => None,
    }
}

async fn detect_proxy() -> Option<String> {
    let ports = [7890, 1080, 7897, 10808, 8080];
    for port in &ports {
        if tokio::net::TcpStream::connect(("127.0.0.1", *port)).await.is_ok() {
            return Some(format!("http://127.0.0.1:{}", port));
        }
    }
    None
}

async fn detailed_os_name() -> String {
    #[cfg(target_os = "windows")]
    if let Some(detail) = windows_os_detail().await {
        return detail;
    }

    let long = sysinfo::System::long_os_version().unwrap_or_default();
    let kernel = sysinfo::System::kernel_version().unwrap_or_default();
    if long.is_empty() {
        friendly_os_name(std::env::consts::OS)
    } else if kernel.is_empty() {
        long
    } else {
        format!("{} {}", long, kernel)
    }
}

#[cfg(target_os = "windows")]
async fn windows_os_detail() -> Option<String> {
    let output = tokio::process::Command::new("wmic")
        .args(["os", "get", "Caption,BuildNumber", "/value"])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut caption = None;
    let mut build = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Caption=") {
            caption = Some(v.trim().to_string());
        } else if let Some(v) = line.strip_prefix("BuildNumber=") {
            build = Some(v.trim().to_string());
        }
    }
    Some(format!("{} {}", caption?, build?))
}

fn friendly_os_name(os: &str) -> String {
    match os {
        "windows" => "Windows",
        "linux" => "Linux",
        "macos" => "macOS",
        "freebsd" => "FreeBSD",
        "dragonfly" => "DragonFly BSD",
        "openbsd" => "OpenBSD",
        "netbsd" => "NetBSD",
        "solaris" => "Solaris",
        _ => os,
    }
    .to_string()
}

async fn check_command(program: &str, args: &[&str]) -> bool {
    match tokio::process::Command::new(program).args(args).output().await {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

async fn check_service_port(service: &str, host: &str, port: u16) -> ServiceDetectionResult {
    let detected = tokio::net::TcpStream::connect((host, port)).await.is_ok();
    ServiceDetectionResult {
        service: service.to_string(),
        detected,
        connection_test_result: None,
    }
}

async fn run_docker_compose_for_service(service: &str, http_proxy: Option<&str>) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new("docker");
    cmd.arg("compose")
        .arg("-f")
        .arg("docker/docker-compose.yaml")
        .arg("up")
        .arg("-d")
        .arg(service);

    if let Some(proxy) = http_proxy {
        cmd.env("HTTP_PROXY", proxy).env("HTTPS_PROXY", proxy);
    }

    let output = cmd.output().await.map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

const NAPCAT_WIN_ONEKEY_URL: &str =
    "https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.Windows.OneKey.zip";

/// Installs NapCat natively on Windows using the OneKey package.
/// Returns the install directory path on success.
async fn install_napcat_native(http_proxy: Option<&str>, qq_id: Option<&str>) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let data_dir = zihuan_core::system_config::app_data_dir();
        let install_root = data_dir.join("zihuan-next_aibot").join("napcat_install");
        let zip_path = install_root.join("NapCat.OneKey.zip");
        let extract_dir = install_root.join("NapCat");

        tokio::fs::create_dir_all(&install_root)
            .await
            .map_err(|e| format!("Failed to create install directory: {e}"))?;

        // Download
        let mut client_builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(600));
        if let Some(proxy) = http_proxy {
            if let Ok(p) = reqwest::Proxy::all(proxy) {
                client_builder = client_builder.proxy(p);
            }
        }
        let client = client_builder
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

        let response = client
            .get(NAPCAT_WIN_ONEKEY_URL)
            .send()
            .await
            .map_err(|e| format!("Failed to download NapCat OneKey: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "Download failed with status {}: {}. \
                 Please download NapCat manually from https://github.com/NapNeko/NapCatQQ/releases",
                response.status().as_u16(),
                response.status().canonical_reason().unwrap_or("Unknown")
            ));
        }

        let total_size = response.content_length().unwrap_or(0);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read download response: {e}"))?;

        tokio::fs::write(&zip_path, &bytes)
            .await
            .map_err(|e| format!("Failed to save downloaded zip: {e}"))?;

        // Extract
        let file = std::fs::File::open(&zip_path).map_err(|e| format!("Failed to open zip: {e}"))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {e}"))?;
        archive
            .extract(&extract_dir)
            .map_err(|e| format!("Failed to extract NapCat: {e}"))?;

        // Find and run NapCatInstaller.exe
        let installer = find_napcat_installer(&extract_dir)?;
        let output = tokio::process::Command::new(&installer)
            .current_dir(&extract_dir)
            .output()
            .await
            .map_err(|e| format!("Failed to run NapCatInstaller.exe: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("NapCatInstaller failed: {stderr}"));
        }

        // Find napcat.bat for the installed shell
        let shell_dir = find_napcat_shell_dir(&extract_dir).unwrap_or_else(|| extract_dir.clone());

        // Quick login: run napcat.bat with QQ number
        if let Some(qq) = qq_id {
            let napcat_bat = shell_dir.join("napcat.bat");
            if napcat_bat.exists() {
                let _ = tokio::process::Command::new("cmd")
                    .args(["/c", "start", "NapCat QQ"])
                    .arg(napcat_bat.to_string_lossy().as_ref())
                    .arg(qq)
                    .current_dir(&shell_dir)
                    .spawn();
            }
        }

        // Open NapCat web UI in browser
        open_url_in_browser("http://127.0.0.1:6099/webui/");

        let size_mb = total_size as f64 / (1024.0 * 1024.0);
        log::info!("NapCat installed ({size_mb:.1} MB) at {}", shell_dir.display());

        Ok(shell_dir.to_string_lossy().to_string())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = http_proxy;
        let _ = qq_id;
        Err(
            "Automatic NapCat installation is currently only supported on Windows with the OneKey package. \
             Please install NapCat manually: https://github.com/NapNeko/NapCatQQ/releases"
                .to_string(),
        )
    }
}

/// Open a URL in the system default browser.
fn open_url_in_browser(url: &str) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd").args(["/c", "start", url]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

#[cfg(target_os = "windows")]
fn find_napcat_installer(extract_dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let root_installer = extract_dir.join("NapCatInstaller.exe");
    if root_installer.exists() {
        return Ok(root_installer);
    }
    for entry in std::fs::read_dir(extract_dir).map_err(|e| format!("Cannot read extract dir: {e}"))? {
        let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("NapCat") && name.contains("Shell") {
                let installer = entry.path().join("NapCatInstaller.exe");
                if installer.exists() {
                    return Ok(installer);
                }
            }
            if let Ok(sub_entries) = std::fs::read_dir(entry.path()) {
                for sub in sub_entries.flatten() {
                    if sub.file_name().to_string_lossy().to_lowercase() == "napcatinstaller.exe" {
                        return Ok(sub.path());
                    }
                }
            }
        }
    }
    Err("Could not find NapCatInstaller.exe in the extracted package".to_string())
}

#[cfg(target_os = "windows")]
fn find_napcat_shell_dir(extract_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(extract_dir).ok()? {
        let entry = entry.ok()?;
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains("Shell") && entry.path().join("napcat.bat").exists() {
                return Some(entry.path());
            }
        }
    }
    None
}
