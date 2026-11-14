//! NapCat native install support: per-platform official installers (Windows
//! OneKey, Linux installer script, macOS GUI installer), support detection,
//! and small helpers shared by the installers.

use crate::setup_orchestrator::{emit_progress, SetupProgressTx};

#[cfg(target_os = "windows")]
const NAPCAT_WIN_ONEKEY_URL: &str =
    "https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.Windows.OneKey.zip";
#[cfg(target_os = "linux")]
const NAPCAT_LINUX_INSTALLER_URL: &str =
    "https://raw.githubusercontent.com/NapNeko/NapCat-Installer/main/script/install.sh";
#[cfg(target_os = "macos")]
const NAPCAT_MAC_INSTALLER_RELEASE_API: &str =
    "https://api.github.com/repos/NapNeko/NapCat-Mac-Installer/releases/latest";

/// HTTP client with optional proxy, shared by the NapCat native installers' downloads.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn build_napcat_http_client(http_proxy: Option<&str>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(std::time::Duration::from_secs(600));
    if let Some(proxy) = http_proxy {
        if let Ok(p) = reqwest::Proxy::all(proxy) {
            builder = builder.proxy(p);
        }
    }
    builder.build().map_err(|e| format!("Failed to build HTTP client: {e}"))
}

/// Whether the local system can run the NapCat native installer: Windows ships the
/// OneKey package, Linux relies on the official installer script's supported package
/// managers (apt/dnf), and macOS needs an existing QQ.app for the Mac installer to patch.
#[cfg(target_os = "windows")]
pub(crate) async fn detect_napcat_native_support() -> (bool, Option<String>) {
    (true, None)
}

#[cfg(target_os = "linux")]
pub(crate) async fn detect_napcat_native_support() -> (bool, Option<String>) {
    for package_manager in ["apt-get", "dnf"] {
        if crate::setup_wizard::check_command(package_manager, &["--version"]).await {
            return (true, None);
        }
    }
    (
        false,
        Some("Linux 上的 NapCat native 安装使用官方安装脚本，需要 apt 或 dnf".to_string()),
    )
}

#[cfg(target_os = "macos")]
pub(crate) async fn detect_napcat_native_support() -> (bool, Option<String>) {
    if std::path::PathBuf::from("/Applications/QQ.app").exists() {
        return (true, None);
    }
    (
        false,
        Some(
            "macOS 上的 NapCat native 安装使用官方 Mac 安装器，需要先安装 QQ 桌面版（/Applications/QQ.app）"
                .to_string(),
        ),
    )
}

/// Installs NapCat natively using each platform's official package.
/// Windows: the OneKey installer. Linux: the official rootless installer script.
/// macOS: the official GUI installer, which is downloaded and opened for the operator.
/// Returns the install directory path on success.
pub(crate) async fn install_napcat_native(
    http_proxy: Option<&str>,
    qq_id: Option<&str>,
    progress: Option<&SetupProgressTx>,
) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        let install_root =
            zihuan_core::system_config::application_data_dir().join("napcat_install");
        let zip_path = install_root.join("NapCat.OneKey.zip");
        let extract_dir = install_root.join("NapCat");

        tokio::fs::create_dir_all(&install_root)
            .await
            .map_err(|e| format!("Failed to create install directory: {e}"))?;

        // Download
        emit_progress(progress, "download", "running", "下载 NapCat OneKey 安装包...".to_string());
        let mut client_builder =
            reqwest::Client::builder().timeout(std::time::Duration::from_secs(600));
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
        let file =
            std::fs::File::open(&zip_path).map_err(|e| format!("Failed to open zip: {e}"))?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {e}"))?;
        archive
            .extract(&extract_dir)
            .map_err(|e| format!("Failed to extract NapCat: {e}"))?;

        // Find and run NapCatInstaller.exe
        let installer = find_napcat_installer(&extract_dir)?;
        emit_progress(
            progress,
            "install",
            "running",
            format!("运行安装器: {}", installer.display()),
        );
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

        emit_progress(
            progress,
            "install",
            "success",
            format!("NapCat 安装完成: {}", shell_dir.display()),
        );
        Ok(shell_dir.to_string_lossy().to_string())
    }

    #[cfg(target_os = "linux")]
    {
        // The Linux script has no quick-login hook, so the QQ id is not usable here.
        let _ = qq_id;
        install_napcat_native_linux(http_proxy, progress).await
    }

    #[cfg(target_os = "macos")]
    {
        // The GUI installer drives the injection; no quick login is possible.
        let _ = qq_id;
        install_napcat_native_macos(http_proxy, progress).await
    }
}

/// Linux native install: runs the official NapCat-Installer script, which is rootless
/// (installs QQ and NapCat under `~/Napcat` plus the `napcat` management CLI) and
/// downloads everything itself. Failure output is surfaced so the operator can fall
/// back to running the script manually in a terminal.
#[cfg(target_os = "linux")]
async fn install_napcat_native_linux(
    http_proxy: Option<&str>,
    progress: Option<&SetupProgressTx>,
) -> Result<String, String> {
    let install_root = zihuan_core::system_config::application_data_dir().join("napcat_install");
    tokio::fs::create_dir_all(&install_root)
        .await
        .map_err(|e| format!("Failed to create install directory: {e}"))?;
    let script_path = install_root.join("napcat-install.sh");

    emit_progress(progress, "download", "running", "下载官方 NapCat-Installer 脚本...".to_string());
    let client = build_napcat_http_client(http_proxy)?;
    let response = client
        .get(NAPCAT_LINUX_INSTALLER_URL)
        .send()
        .await
        .map_err(|e| format!("Failed to download NapCat installer script: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download NapCat installer script ({}). Please download it from \
             https://github.com/NapNeko/NapCat-Installer and run manually: \
             bash napcat.sh --docker n --cli y",
            response.status().as_u16()
        ));
    }
    let script = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read NapCat installer script: {e}"))?;
    tokio::fs::write(&script_path, &script)
        .await
        .map_err(|e| format!("Failed to save NapCat installer script: {e}"))?;

    let mut cmd = tokio::process::Command::new("bash");
    cmd.arg(&script_path)
        .args(["--docker", "n", "--cli", "y", "--github-proxy", "0"]);
    if let Some(proxy) = http_proxy {
        cmd.env("HTTP_PROXY", proxy).env("HTTPS_PROXY", proxy);
    }
    emit_progress(
        progress,
        "install",
        "running",
        format!("执行: bash {} --docker n --cli y --github-proxy 0", script_path.display()),
    );
    // Pulling QQ and NapCat inside the script can take minutes on slow links.
    emit_progress(
        progress,
        "install",
        "running",
        "安装脚本运行中（下载 QQ 与 NapCat，可能需要较长时间）...".to_string(),
    );
    // The script downloads QQ itself, which can take a while on slow links.
    let output = tokio::time::timeout(std::time::Duration::from_secs(1800), cmd.output())
        .await
        .map_err(|_| {
            "NapCat installer script timed out (30 min). Please check the network, or run the \
             script manually in a terminal"
                .to_string()
        })
        .and_then(|result| {
            result.map_err(|e| format!("Failed to run NapCat installer script: {e}"))
        })?;

    if !output.status.success() {
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let tail: Vec<&str> = combined.lines().rev().take(10).collect();
        return Err(format!(
            "NapCat installer script failed (exit {:?}):\n{}",
            output.status.code(),
            tail.into_iter().rev().collect::<Vec<_>>().join("\n")
        ));
    }

    // Rootless layout per the installer documentation.
    let home = std::env::var("HOME").unwrap_or_default();
    emit_progress(progress, "install", "success", format!("NapCat 安装完成: {home}/Napcat"));
    log::info!("NapCat installed via installer script at {home}/Napcat");
    Ok(format!("{home}/Napcat"))
}

/// macOS native install: NapCat officially ships a GUI installer (NapCat-Mac-Installer)
/// that patches the existing QQ.app, so this downloads the latest universal package,
/// strips the quarantine flag and opens it; the operator finishes in the installer
/// window. There is no scripted macOS install path to automate further.
#[cfg(target_os = "macos")]
async fn install_napcat_native_macos(
    http_proxy: Option<&str>,
    progress: Option<&SetupProgressTx>,
) -> Result<String, String> {
    if !std::path::PathBuf::from("/Applications/QQ.app").exists() {
        return Err("macOS 上的 NapCat native 安装需要先安装 QQ 桌面版（/Applications/QQ.app），\
             请从 https://im.qq.com/macqq/ 下载安装后重试"
            .to_string());
    }

    let install_root = zihuan_core::system_config::application_data_dir().join("napcat_install");
    tokio::fs::create_dir_all(&install_root)
        .await
        .map_err(|e| format!("Failed to create install directory: {e}"))?;

    let client = build_napcat_http_client(http_proxy)?;
    emit_progress(progress, "download", "running", "解析最新 NapCat Mac 安装器版本...".to_string());
    // Resolve the latest universal asset via the release API: the asset name carries
    // the version, so the `latest/download/<name>` shortcut breaks on every release.
    let release_response = client
        .get(NAPCAT_MAC_INSTALLER_RELEASE_API)
        .header("User-Agent", "zihuan-next")
        .send()
        .await
        .map_err(|e| format!("Failed to query NapCat Mac installer release: {e}"))?;
    if !release_response.status().is_success() {
        return Err(format!(
            "Failed to query NapCat Mac installer release ({}). Please download it manually from \
             https://github.com/NapNeko/NapCat-Mac-Installer/releases",
            release_response.status().as_u16()
        ));
    }
    let release: serde_json::Value = release_response
        .json()
        .await
        .map_err(|e| format!("Failed to parse NapCat Mac installer release: {e}"))?;
    let asset_url = release["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|asset| {
                    asset["name"].as_str().is_some_and(|name| name.ends_with("_universal.zip"))
                })
                .and_then(|asset| asset["browser_download_url"].as_str())
        })
        .ok_or("未找到 NapCat Mac 安装器的 universal 下载包")?
        .to_string();

    let zip_path = install_root.join("NapCat.Mac.Installer.zip");
    emit_progress(progress, "download", "running", format!("下载 NapCat Mac 安装器: {asset_url}"));
    let response = client
        .get(&asset_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download NapCat Mac installer: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download NapCat Mac installer ({}). Please download it manually from \
             https://github.com/NapNeko/NapCat-Mac-Installer/releases",
            response.status().as_u16()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read NapCat Mac installer download: {e}"))?;
    tokio::fs::write(&zip_path, &bytes)
        .await
        .map_err(|e| format!("Failed to save NapCat Mac installer: {e}"))?;

    let extract_dir = install_root.join("MacInstaller");
    let file = std::fs::File::open(&zip_path).map_err(|e| format!("Failed to open zip: {e}"))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read zip archive: {e}"))?;
    archive
        .extract(&extract_dir)
        .map_err(|e| format!("Failed to extract NapCat Mac installer: {e}"))?;

    let app_path = find_mac_installer_app(&extract_dir)?;
    emit_progress(
        progress,
        "install",
        "running",
        format!("准备打开安装器: {}", app_path.display()),
    );
    // Strip the quarantine flag so Gatekeeper does not block the installer we
    // downloaded ourselves; it is the official NapNeko release.
    let _ = tokio::process::Command::new("xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(&app_path)
        .output()
        .await;
    emit_progress(progress, "install", "running", format!("执行: open {}", app_path.display()));
    let opened = tokio::process::Command::new("open")
        .arg(&app_path)
        .output()
        .await
        .map_err(|e| format!("Failed to open NapCat Mac installer: {e}"))?;
    if !opened.status.success() {
        return Err(format!(
            "Failed to open NapCat Mac installer: {}",
            String::from_utf8_lossy(&opened.stderr).trim()
        ));
    }

    emit_progress(
        progress,
        "install",
        "success",
        "已打开 NapCat Mac 安装器，请在安装器窗口中完成安装".to_string(),
    );
    log::info!("NapCat Mac installer opened at {}", app_path.display());
    Ok(app_path.to_string_lossy().to_string())
}

#[cfg(target_os = "macos")]
fn find_mac_installer_app(dir: &std::path::Path) -> Result<std::path::PathBuf, String> {
    for entry in std::fs::read_dir(dir).map_err(|e| format!("Cannot read extract dir: {e}"))? {
        let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            if path.extension().is_some_and(|ext| ext == "app") {
                return Ok(path);
            }
            if let Ok(found) = find_mac_installer_app(&path) {
                return Ok(found);
            }
        }
    }
    Err("未在 NapCat Mac 安装包中找到安装器 App".to_string())
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
    for entry in
        std::fs::read_dir(extract_dir).map_err(|e| format!("Cannot read extract dir: {e}"))?
    {
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
