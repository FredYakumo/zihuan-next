use serde::Serialize;
use tokio::sync::broadcast;

use zihuan_core::storage::ConnectionConfig;

use crate::installation::napcat::detect_napcat_native_support;
use crate::installation::weaviate::install_weaviate_binary;
use crate::installation::{
    build_search_only_config, check_command, detailed_install_services, run_detailed_binary,
    run_detailed_docker, save_detailed_connections, save_search_connection,
    validate_detailed_config, verify_detailed_connections, DetailedInstallMethod,
    DetailedSearchSetupConfig, DetailedSetupConfig,
};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionConfig>,
}

/// SSE progress channel shared between the orchestrator and the standalone installers.
pub type SetupProgressTx = tokio::sync::broadcast::Sender<SetupProgressEvent>;

/// Emits one progress step, tolerating a channel without subscribers or `None` when the
/// caller runs without progress reporting. Every step is also written to the LogUtil
/// log files (the global logger is a LogUtil forwarder), so installs and uninstalls
/// leave the same trace on disk as they stream to the UI.
pub(crate) fn emit_progress(
    progress: Option<&SetupProgressTx>,
    step: &str,
    status: &str,
    message: String,
) {
    if status == "error" {
        log::error!("[{step}] {message}");
    } else {
        log::info!("[{step}] {message}");
    }
    if let Some(tx) = progress {
        let _ = tx.send(SetupProgressEvent {
            step: step.to_string(),
            status: status.to_string(),
            message,
            progress_percent: None,
            error: None,
            connection: None,
        });
    }
}

#[derive(Clone)]
pub struct SetupOrchestrator {
    progress_tx: broadcast::Sender<SetupProgressEvent>,
    #[allow(dead_code)]
    task_id: String,
}

impl SetupOrchestrator {
    pub fn new(task_id: String, progress_tx: broadcast::Sender<SetupProgressEvent>) -> Self {
        Self { progress_tx, task_id }
    }

    pub async fn run_detailed(&self, config: DetailedSetupConfig) -> Result<(), String> {
        self.emit(
            "validating_detailed_config",
            "running",
            "Validating component configuration...",
            Some(5),
        );
        validate_detailed_config(&config)?;

        let install_services = detailed_install_services(&config);
        if !install_services.is_empty() {
            self.emit(
                "installing_dependencies",
                "running",
                "Installing selected components...",
                Some(15),
            );
            match config.install_method {
                DetailedInstallMethod::Docker => {
                    run_detailed_docker(&config, &install_services).await?
                }
                DetailedInstallMethod::Binary => {
                    run_detailed_binary(&config, &install_services).await?
                }
            }
            self.emit(
                "installing_dependencies",
                "success",
                "Selected components are running",
                Some(55),
            );
        }

        self.emit(
            "verifying_connections",
            "running",
            "Verifying component connections...",
            Some(65),
        );
        save_detailed_connections(&config).await?;
        self.emit("verifying_connections", "success", "Component connections saved", Some(90));

        Ok(())
    }

    /// Installs only the retrieval database described by `search` and saves
    /// its connection. Unlike `run_detailed`, the component installer state is left
    /// untouched so this can run from the Service page at any time.
    pub async fn run_retrieval_database_install(
        &self,
        search: DetailedSearchSetupConfig,
        install_method: DetailedInstallMethod,
    ) -> Result<ConnectionConfig, String> {
        let config = build_search_only_config(search, install_method.clone());
        self.emit(
            "validating_detailed_config",
            "running",
            "Validating retrieval database configuration...",
            Some(5),
        );
        validate_detailed_config(&config)?;
        let search_type = config.search.search_type.clone();
        self.emit(
            "installing_retrieval_database",
            "running",
            &format!("Installing {search_type}..."),
            Some(15),
        );
        match &install_method {
            DetailedInstallMethod::Docker => {
                run_detailed_docker(&config, &detailed_install_services(&config)).await?
            }
            DetailedInstallMethod::Binary => match search_type.as_str() {
                "weaviate" => install_weaviate_binary(&config.search).await?,
                _ => run_detailed_binary(&config, &detailed_install_services(&config)).await?,
            },
        }
        self.emit(
            "installing_retrieval_database",
            "success",
            "Retrieval database is running",
            Some(55),
        );
        self.emit(
            "verifying_connections",
            "running",
            "Verifying retrieval database connection...",
            Some(65),
        );
        verify_detailed_connections(&config).await?;
        let connection = save_search_connection(&config).await?;
        self.emit(
            "verifying_connections",
            "success",
            "Retrieval database connection saved",
            Some(90),
        );
        Ok(connection)
    }

    pub fn emit(&self, step: &str, status: &str, message: &str, progress_percent: Option<u8>) {
        // Mirror the step into LogUtil (the global logger forwards to it) so installs
        // leave a persistent trace next to the streamed UI events.
        if status == "error" {
            log::error!("[{step}] {message}");
        } else {
            log::info!("[{step}] {message}");
        }
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
            connection: None,
        };
        let _ = self.progress_tx.send(event);
    }

    pub fn emit_finished_with_connection(&self, message: &str, connection: ConnectionConfig) {
        let event = SetupProgressEvent {
            step: "finished".to_string(),
            status: "success".to_string(),
            message: message.to_string(),
            progress_percent: Some(100),
            error: None,
            connection: Some(connection),
        };
        let _ = self.progress_tx.send(event);
    }
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
    pub napcat_native_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub napcat_native_reason: Option<String>,
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
    let (binary_install_available, binary_install_reason) =
        detect_binary_installation_support().await;
    let (napcat_native_available, napcat_native_reason) = detect_napcat_native_support().await;
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
        napcat_native_available,
        napcat_native_reason,
        cuda_version,
        compiler_version,
        proxy,
        services,
    }
}

#[cfg(target_os = "windows")]
async fn detect_binary_installation_support() -> (bool, Option<String>) {
    (false, Some("Windows 上的二进制安装需在 WSL 发行版内运行本程序".to_string()))
}

#[cfg(not(target_os = "windows"))]
async fn detect_binary_installation_support() -> (bool, Option<String>) {
    for package_manager in ["brew", "apt-get", "dnf", "pacman"] {
        if check_command(package_manager, &["--version"]).await {
            return (true, None);
        }
    }

    (false, Some("未检测到 Homebrew、apt、dnf 或 pacman 包管理器".to_string()))
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
        let cuda_path =
            std::path::PathBuf::from("C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA");
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

pub(crate) async fn detect_proxy() -> Option<String> {
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

async fn check_service_port(service: &str, host: &str, port: u16) -> ServiceDetectionResult {
    let detected = tokio::net::TcpStream::connect((host, port)).await.is_ok();
    ServiceDetectionResult {
        service: service.to_string(),
        detected,
        connection_test_result: None,
    }
}
