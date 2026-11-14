//! Weaviate install logic for the detailed setup wizard: Compose service
//! rendering, connection building, retrieval collection initialization, the
//! official binary installer, and reachability checks.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::setup_orchestrator::config_factory;
use crate::setup_wizard::{
    access_endpoint, compose_bind_mount, endpoint_host_port, wait_for_tcp, yaml_quote,
    DetailedComponentSource, DetailedSearchAuthMethod, DetailedSearchSetupConfig,
    DetailedSetupConfig, RETRIEVAL_SCHEMAS,
};
use zihuan_core::storage::{
    ensure_collection_schema, ConnectionAuthMethod, ConnectionConfig, ConnectionKind,
    WeaviateConnection,
};
use zihuan_core::weaviate::WeaviateRef;

/// Startup budget for [`verify`].
const WEAVIATE_VERIFY_TIMEOUT: Duration = Duration::from_secs(120);

const WEAVIATE_BINARY_VERSION: &str = "1.30.5";

pub(crate) fn install_service(config: &DetailedSetupConfig) -> Option<&'static str> {
    (config.search.enabled
        && config.search.source == DetailedComponentSource::Install
        && config.search.search_type == "weaviate")
        .then_some("weaviate")
}

pub(crate) fn data_dir(config: &DetailedSetupConfig) -> Option<String> {
    (config.search.enabled
        && config.search.source == DetailedComponentSource::Install
        && config.search.search_type == "weaviate")
        .then(|| config.search.deployment.data_dir.clone())
}

pub(crate) fn compose_service(config: &DetailedSetupConfig) -> String {
    if !(config.search.enabled
        && config.search.source == DetailedComponentSource::Install
        && config.search.search_type == "weaviate")
    {
        return String::new();
    }
    let d = &config.search.deployment;
    let authentication = format!("      AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED: 'false'\n      AUTHENTICATION_APIKEY_ENABLED: 'true'\n      AUTHENTICATION_APIKEY_ALLOWED_KEYS: {}\n", yaml_quote(config.search.api_key.as_deref().unwrap_or_default()));
    format!("  weaviate:\n    image: {}\n    container_name: {}\n    restart: {}\n    ports: [\"{}:8080\"]\n    volumes: [{}]\n    environment:\n{}      DEFAULT_VECTORIZER_MODULE: none\n      CLUSTER_HOSTNAME: node1\n", yaml_quote(&d.image), yaml_quote(&d.container_name), yaml_quote(&d.restart_policy), d.port, compose_bind_mount(&d.data_dir, "/var/lib/weaviate"), authentication)
}

pub(crate) fn connection_kind(
    base_url: String,
    search: &DetailedSearchSetupConfig,
) -> ConnectionKind {
    ConnectionKind::Weaviate(WeaviateConnection {
        base_url,
        username: if search.auth_method == DetailedSearchAuthMethod::Password {
            search.username.clone()
        } else {
            None
        },
        password: if search.auth_method == DetailedSearchAuthMethod::Password {
            search.password.clone()
        } else {
            None
        },
        api_key: if search.auth_method == DetailedSearchAuthMethod::ApiKey {
            search.api_key.clone()
        } else {
            None
        },
        auth_method: match search.auth_method {
            DetailedSearchAuthMethod::Password => ConnectionAuthMethod::Password,
            DetailedSearchAuthMethod::ApiKey => ConnectionAuthMethod::ApiKey,
        },
    })
}

/// The connection as published by the generated install command, pointing at
/// the target machine address when public access was requested.
pub(crate) fn remote_connection(config: &DetailedSetupConfig) -> Option<ConnectionConfig> {
    if !(config.search.enabled && config.search.search_type == "weaviate") {
        return None;
    }
    let base_url =
        access_endpoint(config, &config.search.base_url, "http", config.search.deployment.port);
    Some(config_factory::build_connection(
        "setup-detailed-weaviate",
        "weaviate",
        connection_kind(base_url, &config.search),
    ))
}

/// Initializes every retrieval collection on the freshly installed Weaviate.
pub(crate) fn ensure_collections(weaviate: WeaviateConnection) -> Result<(), String> {
    for schema in RETRIEVAL_SCHEMAS {
        let reference = WeaviateRef::new(
            weaviate.base_url.clone(),
            schema.weaviate_class_name(),
            weaviate.username.clone(),
            weaviate.password.clone(),
            weaviate.api_key.clone(),
            Duration::from_secs(30),
        )
        .map_err(|err| err.to_string())?;
        ensure_collection_schema(&reference, schema, true).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub(crate) async fn verify(config: &DetailedSetupConfig) -> Result<(), String> {
    if config.search.enabled && config.search.search_type == "weaviate" {
        let (host, port) = endpoint_host_port(&config.search.base_url, 8080)?;
        wait_for_tcp(&host, port, "search database", WEAVIATE_VERIFY_TIMEOUT).await?;
    }
    Ok(())
}

pub(crate) fn binary_package(package_manager: &str) -> Result<&'static str, String> {
    Err(format!(
        "weaviate is unavailable from {package_manager}; use Docker or install its official release before choosing '使用现有配置'."
    ))
}

/// The copyable install command for a Weaviate binary install on a remote machine.
pub(crate) fn binary_install_command(config: &DetailedSetupConfig) -> String {
    let api_key = config.search.api_key.as_deref().unwrap_or_default();
    let port = config.search.deployment.port;
    let version = WEAVIATE_BINARY_VERSION;
    format!(
        "# Run on the target Linux x86_64 machine (arm64: replace amd64 with arm64 in the download URL)\n\
sudo mkdir -p /opt/zihuan-weaviate/data && cd /opt/zihuan-weaviate\n\
sudo curl -fL -o weaviate.tar.gz 'https://github.com/weaviate/weaviate/releases/download/v{version}/weaviate-v{version}-linux-amd64.tar.gz'\n\
sudo tar -xzf weaviate.tar.gz\n\
sudo sh -c 'BIN=$(find . -type f -name weaviate | head -n 1); chmod +x \"$BIN\"; AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED=false AUTHENTICATION_APIKEY_ENABLED=true AUTHENTICATION_APIKEY_ALLOWED_KEYS=\"{api_key}\" DEFAULT_VECTORIZER_MODULE=none CLUSTER_HOSTNAME=node1 PERSISTENCE_DATA_PATH=/opt/zihuan-weaviate/data GO_PORT={port} nohup \"$BIN\" > weaviate.log 2>&1 &'\n"
    )
}

/// Installs Weaviate from its official GitHub release binary. Weaviate does
/// not publish Windows binaries, so this path only exists for Linux and macOS
/// hosts.
pub(crate) async fn install_weaviate_binary(
    search: &DetailedSearchSetupConfig,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let _ = search;
        Err(
            "Weaviate does not publish an official Windows binary. Use the Docker install method, or run the binary install command on a Linux machine."
                .to_string(),
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let arch = match std::env::consts::ARCH {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            other => {
                return Err(format!("Weaviate binary install is unavailable for {other} machines"))
            }
        };
        let install_root =
            zihuan_core::system_config::application_data_dir().join("bin").join("weaviate");
        tokio::fs::create_dir_all(&install_root)
            .await
            .map_err(|err| format!("Failed to create Weaviate install directory: {err}"))?;

        let asset_name = if cfg!(target_os = "macos") {
            format!("weaviate-v{WEAVIATE_BINARY_VERSION}-darwin-all.zip")
        } else {
            format!("weaviate-v{WEAVIATE_BINARY_VERSION}-linux-{arch}.tar.gz")
        };
        let archive_path = install_root.join(&asset_name);
        if !archive_path.exists() {
            download_weaviate_archive(&asset_name, &archive_path).await?;
        }
        let binary_path = extract_weaviate_archive(&archive_path, &install_root).await?;

        let port = search.deployment.port;
        if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            log::info!(
                "[setup_orchestrator] port {port} is in use; assuming Weaviate is already running"
            );
            return Ok(());
        }

        let data_dir = install_root.join("data");
        tokio::fs::create_dir_all(&data_dir)
            .await
            .map_err(|err| format!("Failed to create Weaviate data directory: {err}"))?;

        let log_path = install_root.join("weaviate.log");
        let stdout = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|err| format!("Failed to open Weaviate log file: {err}"))?;
        let stderr = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|err| format!("Failed to open Weaviate log file: {err}"))?;

        let child = tokio::process::Command::new(&binary_path)
            .env("AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED", "false")
            .env("AUTHENTICATION_APIKEY_ENABLED", "true")
            .env("AUTHENTICATION_APIKEY_ALLOWED_KEYS", search.api_key.clone().unwrap_or_default())
            .env("DEFAULT_VECTORIZER_MODULE", "none")
            .env("CLUSTER_HOSTNAME", "node1")
            .env("PERSISTENCE_DATA_PATH", &data_dir)
            .env("GO_PORT", port.to_string())
            .current_dir(&install_root)
            .stdout(std::process::Stdio::from(stdout))
            .stderr(std::process::Stdio::from(stderr))
            .spawn()
            .map_err(|err| format!("Failed to start Weaviate: {err}"))?;
        if let Some(pid) = child.id() {
            let _ = tokio::fs::write(install_root.join("weaviate.pid"), pid.to_string()).await;
        }
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
async fn download_weaviate_archive(asset_name: &str, destination: &Path) -> Result<(), String> {
    let url = format!(
        "https://github.com/weaviate/weaviate/releases/download/v{WEAVIATE_BINARY_VERSION}/{asset_name}"
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {err}"))?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("Failed to download Weaviate binary: {err}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Weaviate binary download failed with status {}: {url}",
            response.status().as_u16()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| format!("Failed to read Weaviate binary download: {err}"))?;
    tokio::fs::write(destination, &bytes)
        .await
        .map_err(|err| format!("Failed to save Weaviate archive: {err}"))
}

#[cfg(not(target_os = "windows"))]
async fn extract_weaviate_archive(
    archive_path: &Path,
    install_root: &Path,
) -> Result<PathBuf, String> {
    let archive = archive_path.to_path_buf();
    let target_dir = install_root.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
        if archive.extension().map(|ext| ext == "zip").unwrap_or(false) {
            let file = std::fs::File::open(&archive)
                .map_err(|err| format!("Failed to open Weaviate archive: {err}"))?;
            let mut zip = zip::ZipArchive::new(file)
                .map_err(|err| format!("Failed to read Weaviate archive: {err}"))?;
            zip.extract(&target_dir)
                .map_err(|err| format!("Failed to extract Weaviate archive: {err}"))?;
        } else {
            let output = std::process::Command::new("tar")
                .arg("-xzf")
                .arg(&archive)
                .arg("-C")
                .arg(&target_dir)
                .output()
                .map_err(|err| format!("Failed to extract Weaviate archive: {err}"))?;
            if !output.status.success() {
                return Err(format!(
                    "Failed to extract Weaviate archive: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
        let binary_path = find_weaviate_binary(&target_dir)
            .ok_or_else(|| "Weaviate binary was not found in the downloaded archive".to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary_path, std::fs::Permissions::from_mode(0o755))
                .map_err(|err| format!("Failed to mark Weaviate binary executable: {err}"))?;
        }
        Ok(binary_path)
    })
    .await
    .map_err(|err| format!("Weaviate binary extraction task failed: {err}"))?
}

#[cfg(not(target_os = "windows"))]
fn find_weaviate_binary(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_weaviate_binary(&path) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|name| name == "weaviate") {
            return Some(path);
        }
    }
    None
}
