//! Installs NapCat (the QQ OneBot11 protocol side) as a plugin.
//!
//! NapCat ships both as the official Docker image (`mlikiowa/napcat-docker`, which
//! initializes a OneBot WebSocket server on 3001 and its WebUI on 6099) and as a
//! Windows-only OneKey package, so three install methods are supported:
//!
//! - `docker`: the server runs the compose project itself, probes the WebUI port and
//!   records the bot-adapter connection once the container is up.
//! - `command_docker`: nothing runs here. The compose project and env file are embedded
//!   base64 into a shell command the operator copies to the target Linux machine, like
//!   the standard command-mode installers. The recorded connection points at the target
//!   machine address when public access is exposed, otherwise at `127.0.0.1`.
//! - `native`: Windows only. Reuses the setup wizard's OneKey installer; the recorded
//!   connection points at the default `ws://127.0.0.1:3001` and keeps the install path.
//!
//! The QQ login (QR code in the WebUI) happens after install, so the recorded connection
//! leaves `qq_id` empty for the connection form to fill.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use serde_json::Value;

use super::super::{
    command_uninstall_command, plugin_from_request, save_plugin_record, InstallPluginRequest,
    PluginRecord,
};
use super::{
    docker_compose_available, pick_free_port, run_compose, short_id, strip_unrelated_metadata,
    SpecialPluginInstaller,
};
use crate::setup_orchestrator::{
    base64_encode, detect_proxy, emit_progress, install_napcat_native, SetupProgressTx,
};
use zihuan_core::ims_bot_adapter::BotAdapterConnection;
use zihuan_core::storage::{self, ConnectionConfig, ConnectionKind};

const IMAGE: &str = "mlikiowa/napcat-docker:latest";
const DEFAULT_WEBUI_PORT: u16 = 6099;
const DEFAULT_WS_PORT: u16 = 3001;
const PROBE_ATTEMPTS: usize = 30;
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

pub struct NapCatSpecialPluginInstaller;

/// Ports, secrets and the host the recorded connection points at, shared by the Docker
/// install methods.
struct NapCatDeployment {
    webui_port: u16,
    ws_port: u16,
    /// Fixes the NapCat WebUI token instead of the random one printed in the container
    /// logs, so the operator can log in without digging through `docker logs`.
    webui_token: String,
    project: String,
    /// Publishes the ports on all interfaces instead of `127.0.0.1` so a zihuan_service
    /// running elsewhere can reach the OneBot WebSocket.
    expose_public_access: bool,
    connection_host: String,
}

impl SpecialPluginInstaller for NapCatSpecialPluginInstaller {
    fn component_type(&self) -> &'static str {
        "napcat"
    }

    fn install<'a>(
        &'a self,
        request: &'a InstallPluginRequest,
    ) -> Pin<Box<dyn Future<Output = Result<PluginRecord, String>> + Send + 'a>> {
        Box::pin(async move { Self::install_impl(request, None).await })
    }

    fn install_with_progress<'a>(
        &'a self,
        request: &'a InstallPluginRequest,
        progress: &'a SetupProgressTx,
    ) -> Pin<Box<dyn Future<Output = Result<PluginRecord, String>> + Send + 'a>> {
        Box::pin(async move { Self::install_impl(request, Some(progress)).await })
    }

    fn uninstall<'a>(
        &'a self,
        plugin: &'a PluginRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move { Self::uninstall_impl(plugin, None).await })
    }

    fn uninstall_with_progress<'a>(
        &'a self,
        plugin: &'a PluginRecord,
        progress: &'a SetupProgressTx,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move { Self::uninstall_impl(plugin, Some(progress)).await })
    }
}

impl NapCatSpecialPluginInstaller {
    fn uninstall_impl<'a>(
        plugin: &'a PluginRecord,
        progress: Option<&'a SetupProgressTx>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let Some(compose_path) =
                plugin.extra_install_metadata.get("compose_path").and_then(Value::as_str)
            else {
                // Command-mode installs run elsewhere and native installs keep the
                // OneKey-managed NapCat on disk (it may be shared with the setup
                // wizard); removing the record is all that happens here.
                emit_progress(
                    progress,
                    "uninstall",
                    "running",
                    "本机无运行中的组件，直接移除插件记录...".to_string(),
                );
                return Ok(());
            };
            let project = format!("zihuan-napcat-{}", short_id(&plugin.id));
            emit_progress(
                progress,
                "uninstall",
                "running",
                format!(
                    "执行: docker compose -f {compose_path} -p {project} down --volumes --remove-orphans"
                ),
            );
            // The stack may still be running when compose fails (daemon down, etc.);
            // fail the uninstall so the record and its data survive for a manual
            // cleanup and retry instead of leaking the container.
            run_compose(
                Path::new(compose_path),
                &project,
                &["down", "--volumes", "--remove-orphans"],
            )
            .await?;
            emit_progress(progress, "uninstall", "success", "容器已停止并移除".to_string());
            if let Some(dir) = Path::new(compose_path).parent() {
                emit_progress(
                    progress,
                    "uninstall",
                    "running",
                    format!("删除数据目录: {}", dir.display()),
                );
                if let Err(error) = tokio::fs::remove_dir_all(dir).await {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        return Err(format!(
                            "删除 NapCat 数据目录 {} 失败: {error}",
                            dir.display()
                        ));
                    }
                }
                emit_progress(progress, "uninstall", "success", "数据目录已删除".to_string());
            }
            Ok(())
        })
    }
}

impl NapCatSpecialPluginInstaller {
    fn install_impl<'a>(
        request: &'a InstallPluginRequest,
        progress: Option<&'a SetupProgressTx>,
    ) -> Pin<Box<dyn Future<Output = Result<PluginRecord, String>> + Send + 'a>> {
        Box::pin(async move {
            if matches!(request.install_method.as_str(), "binary" | "command_binary") {
                return Err(
                    "NapCat 不支持包管理器安装方式，请选择 Docker、native 或 Docker 安装命令"
                        .to_string(),
                );
            }
            // Requests without a method come from clients predating the command mode and
            // always meant the local Docker install.
            match request.install_method.as_str() {
                "native" => install_native_mode(request, progress).await,
                "command_docker" => install_command_mode(request, progress).await,
                "command_native" => install_native_command_mode(request, progress).await,
                _ => install_local_mode(request, progress).await,
            }
        })
    }
}

async fn install_local_mode(
    request: &InstallPluginRequest,
    progress: Option<&SetupProgressTx>,
) -> Result<PluginRecord, String> {
    emit_progress(progress, "prepare", "running", "检查 Docker Compose 可用性...".into());
    if !docker_compose_available().await {
        return Err(
            "Docker Compose 不可用：NapCat 通过 Docker 运行，请先安装并启动 Docker，\
             或改用 native / 安装命令方式"
                .to_string(),
        );
    }

    let mut plugin = plugin_from_request(request, "installing");
    plugin.installation_method = "docker".to_string();
    strip_unrelated_metadata(&mut plugin);
    let deployment = plan_deployment(request, &plugin.id);

    let install_dir = install_dir(&plugin.id);
    emit_progress(
        progress,
        "prepare",
        "running",
        format!("创建安装目录: {}", install_dir.display()),
    );
    tokio::fs::create_dir_all(&install_dir).await.map_err(|error| {
        format!("创建 NapCat 数据目录 {} 失败: {error}", install_dir.display())
    })?;

    emit_progress(progress, "compose", "running", "写入 docker-compose.yaml 与 .env...".into());
    write_env(&install_dir, &deployment).await?;
    let compose_path = install_dir.join("docker-compose.yaml");
    tokio::fs::write(&compose_path, compose_yaml(deployment.expose_public_access))
        .await
        .map_err(|error| format!("写入 NapCat compose 文件失败: {error}"))?;

    // Pulling the image can take minutes; the SSE keep-alive covers the silent stretch.
    emit_progress(
        progress,
        "compose",
        "running",
        format!(
            "执行: docker compose -f {} -p {} up -d",
            compose_path.display(),
            deployment.project
        ),
    );
    run_compose(&compose_path, &deployment.project, &["up", "-d"]).await?;
    emit_progress(progress, "compose", "success", "容器已启动".into());

    emit_progress(
        progress,
        "probe",
        "running",
        format!("探测 WebUI: http://127.0.0.1:{} ...", deployment.webui_port),
    );
    let webui_reachable = wait_for_webui(deployment.webui_port).await;
    if webui_reachable {
        emit_progress(progress, "probe", "success", "WebUI 已就绪".into());
    } else {
        emit_progress(
            progress,
            "probe",
            "running",
            "WebUI 暂未响应，容器仍在启动中".into(),
        );
    }

    emit_progress(progress, "connection", "running", "保存 BotAdapter 连接...".into());
    let connection = bot_adapter_connection(
        &plugin,
        &deployment.connection_host,
        deployment.ws_port,
        None,
    );
    storage::upsert_connection(connection.clone()).map_err(|error| error.to_string())?;
    emit_progress(
        progress,
        "connection",
        "success",
        format!("连接 {} 已创建", connection.name),
    );

    plugin.connection_ids = vec![connection.config_id.clone()];
    plugin.status = "installed".to_string();
    plugin.updated_at = chrono::Utc::now().to_rfc3339();
    let metadata = &mut plugin.extra_install_metadata;
    metadata["connection_config"] = serde_json::to_value(&connection).unwrap_or(Value::Null);
    metadata["compose_path"] = Value::String(compose_path.to_string_lossy().to_string());
    metadata["data_dir"] = Value::String(install_dir.to_string_lossy().to_string());
    metadata["webui_port"] = Value::from(deployment.webui_port);
    metadata["ws_port"] = Value::from(deployment.ws_port);
    metadata["webui_token"] = Value::String(deployment.webui_token.clone());
    metadata["webui_url"] = Value::String(format!(
        "http://{}:{}",
        deployment.connection_host, deployment.webui_port
    ));
    metadata["ws_url"] = Value::String(format!(
        "ws://{}:{}",
        deployment.connection_host, deployment.ws_port
    ));
    metadata["uninstall_command"] = Value::String(format!(
        "docker compose -f {} -p {} down --volumes --remove-orphans",
        compose_path.display(),
        deployment.project
    ));
    if !webui_reachable {
        metadata["warning"] = Value::String(
            "容器已启动，但 WebUI 在探测期内未响应；请稍后打开 WebUI 扫码登录 QQ，\
             或查看 `docker logs` 后重试"
                .to_string(),
        );
    }
    emit_progress(
        progress,
        "finished",
        "success",
        format!("安装完成: {}", plugin.name),
    );
    save_plugin_record(&plugin)?;
    Ok(plugin)
}

/// Builds the copy-command install: the stack is never started here, the operator runs
/// the generated command on the machine that has Docker.
async fn install_command_mode(
    request: &InstallPluginRequest,
    progress: Option<&SetupProgressTx>,
) -> Result<PluginRecord, String> {
    emit_progress(progress, "command", "running", "生成 Docker 安装命令...".into());
    let mut plugin = plugin_from_request(request, "command_generated");
    strip_unrelated_metadata(&mut plugin);
    let deployment = plan_deployment(request, &plugin.id);

    let compose = compose_yaml(deployment.expose_public_access);
    let env = env_file_contents(&deployment);
    let install_command = format!(
        "# Run on the target Linux machine\n\
         mkdir -p ~/zihuan-next-install && cd ~/zihuan-next-install\n\
         printf '%s' '{}' | base64 -d > docker-compose.yaml\n\
         printf '%s' '{}' | base64 -d > .env\n\
         docker compose -f docker-compose.yaml up -d",
        base64_encode(compose.as_bytes()),
        base64_encode(env.as_bytes()),
    );

    let connection = bot_adapter_connection(
        &plugin,
        &deployment.connection_host,
        deployment.ws_port,
        None,
    );
    storage::upsert_connection(connection.clone()).map_err(|error| error.to_string())?;

    plugin.connection_ids = vec![connection.config_id.clone()];
    plugin.updated_at = chrono::Utc::now().to_rfc3339();
    let metadata = &mut plugin.extra_install_metadata;
    // Nothing exists locally: drop the placeholder path `plugin_from_request` derived
    // from the detailed config so uninstall never tries to stop a local stack.
    if let Some(object) = metadata.as_object_mut() {
        object.remove("compose_path");
    }
    metadata["install_command"] = Value::String(install_command);
    metadata["uninstall_command"] = Value::String(command_uninstall_command());
    metadata["connection_config"] = serde_json::to_value(&connection).unwrap_or(Value::Null);
    metadata["webui_port"] = Value::from(deployment.webui_port);
    metadata["ws_port"] = Value::from(deployment.ws_port);
    metadata["webui_token"] = Value::String(deployment.webui_token.clone());
    metadata["webui_url"] = Value::String(format!(
        "http://{}:{}",
        deployment.connection_host, deployment.webui_port
    ));
    metadata["ws_url"] = Value::String(format!(
        "ws://{}:{}",
        deployment.connection_host, deployment.ws_port
    ));
    metadata["project"] = Value::String(deployment.project.clone());
    emit_progress(
        progress,
        "finished",
        "success",
        "安装命令已生成，请在目标机器上执行".into(),
    );
    save_plugin_record(&plugin)?;
    Ok(plugin)
}

/// Builds the native copy-command install: nothing runs here, the operator pastes the
/// generated block into the target machine's shell — Windows PowerShell for the OneKey
/// package, Linux bash for the official installer script — and finishes the QQ login
/// there.
async fn install_native_command_mode(
    request: &InstallPluginRequest,
    progress: Option<&SetupProgressTx>,
) -> Result<PluginRecord, String> {
    emit_progress(progress, "command", "running", "生成 native 安装命令...".into());
    let mut plugin = plugin_from_request(request, "command_generated");
    strip_unrelated_metadata(&mut plugin);

    // Same host rule as the Docker command mode: the recorded connection points at the
    // target machine only when public access was exposed and its address requested.
    let connection_host = request
        .detailed_config
        .as_ref()
        .map(|config| {
            if config.expose_public_access
                && config.use_target_machine_address
                && !config.target_machine_address.trim().is_empty()
            {
                config.target_machine_address.trim().to_string()
            } else {
                "127.0.0.1".to_string()
            }
        })
        .unwrap_or_else(|| "127.0.0.1".to_string());

    let connection = bot_adapter_connection(&plugin, &connection_host, DEFAULT_WS_PORT, None);
    storage::upsert_connection(connection.clone()).map_err(|error| error.to_string())?;

    plugin.connection_ids = vec![connection.config_id.clone()];
    plugin.updated_at = chrono::Utc::now().to_rfc3339();
    let metadata = &mut plugin.extra_install_metadata;
    if let Some(object) = metadata.as_object_mut() {
        object.remove("compose_path");
    }
    metadata["install_command"] = Value::String(native_install_command());
    metadata["uninstall_command"] = Value::String(native_uninstall_hint());
    metadata["connection_config"] = serde_json::to_value(&connection).unwrap_or(Value::Null);
    metadata["webui_port"] = Value::from(DEFAULT_WEBUI_PORT);
    metadata["ws_port"] = Value::from(DEFAULT_WS_PORT);
    metadata["webui_url"] =
        Value::String(format!("http://{connection_host}:{DEFAULT_WEBUI_PORT}"));
    metadata["ws_url"] = Value::String(format!("ws://{connection_host}:{DEFAULT_WS_PORT}"));
    emit_progress(
        progress,
        "finished",
        "success",
        "native 安装命令已生成，请在目标机器上执行".into(),
    );
    save_plugin_record(&plugin)?;
    Ok(plugin)
}

/// Copy-paste block for installing NapCat natively on a target machine. Both official
/// paths are included; the operator runs the section matching the target OS. The QQ
/// login and the OneBot WebSocket port stay with the operator on that machine.
fn native_install_command() -> String {
    r#"# ===== Windows（在 PowerShell 中执行；需已安装 QQ 桌面版）=====
$dir = "$env:USERPROFILE\NapCat"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
Invoke-WebRequest -Uri "https://github.com/NapNeko/NapCatQQ/releases/latest/download/NapCat.Shell.Windows.OneKey.zip" -OutFile "$dir\NapCat.OneKey.zip"
Expand-Archive -Path "$dir\NapCat.OneKey.zip" -DestinationPath "$dir\NapCat" -Force
$installer = Get-ChildItem -Path "$dir\NapCat" -Recurse -Filter "NapCatInstaller.exe" | Select-Object -First 1
Start-Process $installer.FullName -Wait
# 安装完成后启动 QQ 并登录；NapCat WebUI 默认 http://127.0.0.1:6099，OneBot WebSocket 默认 ws://127.0.0.1:3001

# ===== Linux（在终端中执行；官方 rootless 安装脚本，安装到 ~/Napcat）=====
curl -fL --connect-timeout 20 --max-time 1800 -o napcat.sh https://raw.githubusercontent.com/NapNeko/NapCat-Installer/main/script/install.sh && bash napcat.sh --docker n --cli y --github-proxy 0
# 用 napcat 命令启动服务；NapCat WebUI 默认 http://127.0.0.1:6099，OneBot WebSocket 默认 ws://127.0.0.1:3001
"#
    .to_string()
}

fn native_uninstall_hint() -> String {
    "# 卸载插件不会删除目标机器上的 NapCat 本体；如需彻底卸载，请手动删除其安装目录（Windows: %USERPROFILE%\\NapCat，Linux: ~/Napcat）"
        .to_string()
}

/// Windows OneKey install: reuses the setup wizard's downloader and keeps the install
/// path on the recorded connection. The WebUI opens in the browser as part of the
/// shared installer; the QR login itself is left to the operator.
async fn install_native_mode(
    request: &InstallPluginRequest,
    progress: Option<&SetupProgressTx>,
) -> Result<PluginRecord, String> {
    emit_progress(
        progress,
        "native",
        "running",
        "开始 native 安装（按平台使用官方 OneKey 安装包 / 安装脚本 / Mac 安装器）...".into(),
    );
    let mut plugin = plugin_from_request(request, "installing");
    plugin.installation_method = "native".to_string();
    strip_unrelated_metadata(&mut plugin);

    let proxy = detect_proxy().await;
    let install_path = install_napcat_native(proxy.as_deref(), None, progress).await?;

    let connection = bot_adapter_connection(
        &plugin,
        "127.0.0.1",
        DEFAULT_WS_PORT,
        Some(install_path.clone()),
    );
    storage::upsert_connection(connection.clone()).map_err(|error| error.to_string())?;

    plugin.connection_ids = vec![connection.config_id.clone()];
    plugin.status = "installed".to_string();
    plugin.updated_at = chrono::Utc::now().to_rfc3339();
    let metadata = &mut plugin.extra_install_metadata;
    // Nothing runs from a compose file here: drop the placeholder path
    // `plugin_from_request` derived from the detailed config so uninstall never tries
    // to stop a local stack.
    if let Some(object) = metadata.as_object_mut() {
        object.remove("compose_path");
    }
    metadata["connection_config"] = serde_json::to_value(&connection).unwrap_or(Value::Null);
    metadata["install_path"] = Value::String(install_path.clone());
    metadata["webui_port"] = Value::from(DEFAULT_WEBUI_PORT);
    metadata["ws_port"] = Value::from(DEFAULT_WS_PORT);
    metadata["webui_url"] = Value::String("http://127.0.0.1:6099/webui/".to_string());
    metadata["ws_url"] = Value::String(format!("ws://127.0.0.1:{DEFAULT_WS_PORT}"));
    metadata["uninstall_command"] = Value::String(format!(
        "# 卸载插件不会删除 NapCat 本体；如需彻底卸载请手动删除 {}",
        install_path
    ));
    // What "native" leaves behind differs per platform: Windows opens the WebUI right
    // away, Linux needs a `napcat` start, and macOS opens the GUI installer instead.
    let note = if cfg!(target_os = "macos") {
        "NapCat Mac 安装器已打开，请在安装器窗口完成注入；完成后启动 QQ，在 NapCat WebUI（默认 \
         6099 端口）扫码登录，并在网络配置里让 OneBot WebSocket 服务监听 3001 端口"
    } else if cfg!(target_os = "linux") {
        "NapCat 已通过官方脚本安装到 ~/Napcat，使用 `napcat` 命令启动后，在 WebUI（默认 \
         6099 端口）扫码登录，并在网络配置里让 OneBot WebSocket 服务监听 3001 端口"
    } else {
        "NapCat 已安装到本机，请在打开的 WebUI 里扫码登录 QQ；OneBot WebSocket 服务\
         需要在 WebUI 网络配置里监听 3001 端口"
    };
    metadata["warning"] = Value::String(note.to_string());
    emit_progress(
        progress,
        "finished",
        "success",
        format!("NapCat native 安装完成: {install_path}"),
    );
    save_plugin_record(&plugin)?;
    Ok(plugin)
}

fn plan_deployment(request: &InstallPluginRequest, plugin_id: &str) -> NapCatDeployment {
    let (expose_public_access, connection_host) = request
        .detailed_config
        .as_ref()
        .map(|config| {
            // Same host rule as the standard command-mode installers: the recorded
            // connection only points at the target machine when the operator exposed
            // public access and asked to use that address.
            let host = if config.expose_public_access
                && config.use_target_machine_address
                && !config.target_machine_address.trim().is_empty()
            {
                config.target_machine_address.trim().to_string()
            } else {
                "127.0.0.1".to_string()
            };
            (config.expose_public_access, host)
        })
        .unwrap_or((false, "127.0.0.1".to_string()));
    let webui_port = pick_free_port(DEFAULT_WEBUI_PORT);
    // The scan ranges (6099+, 3001+) never overlap, so the two picks cannot collide.
    let ws_port = pick_free_port(DEFAULT_WS_PORT);
    let webui_token = uuid::Uuid::new_v4().simple().to_string();
    let project = format!("zihuan-napcat-{}", short_id(plugin_id));
    NapCatDeployment {
        webui_port,
        ws_port,
        webui_token,
        project,
        expose_public_access,
        connection_host,
    }
}

fn bot_adapter_connection(
    plugin: &PluginRecord,
    host: &str,
    ws_port: u16,
    napcat_install_path: Option<String>,
) -> ConnectionConfig {
    let kind = BotAdapterConnection {
        bot_server_url: format!("ws://{host}:{ws_port}"),
        mock: None,
        adapter_server_url: None,
        bot_server_token: None,
        qq_id: None,
        napcat_install_path,
        wechat: None,
    };
    ConnectionConfig {
        id: format!("plugin-napcat-{}", plugin.id),
        config_id: format!("plugin-napcat-{}", plugin.id),
        name: format!("{} (NapCat)", plugin.name),
        enabled: true,
        kind: ConnectionKind::BotAdapter(
            serde_json::to_value(kind).unwrap_or(serde_json::Value::Null),
        ),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn install_dir(plugin_id: &str) -> PathBuf {
    zihuan_core::system_config::application_data_dir()
        .join("plugins")
        .join(format!("napcat-{}", short_id(plugin_id)))
}

async fn write_env(dir: &Path, deployment: &NapCatDeployment) -> Result<(), String> {
    tokio::fs::write(dir.join(".env"), env_file_contents(deployment))
        .await
        .map_err(|error| format!("写入 NapCat 环境变量失败: {error}"))
}

fn env_file_contents(deployment: &NapCatDeployment) -> String {
    format!(
        "# Generated by the Zihuan Next NapCat plugin installer.\n\
CONTAINER_PREFIX={project}\n\
WEBUI_TOKEN={webui_token}\n\
WEBUI_PORT={webui_port}\n\
WS_PORT={ws_port}\n\
TZ=Asia/Shanghai\n",
        project = deployment.project,
        webui_token = deployment.webui_token,
        webui_port = deployment.webui_port,
        ws_port = deployment.ws_port,
    )
}

/// Compose project mirroring the official `NapCat-Docker/compose/ws.yml`: `MODE=ws`
/// initializes a OneBot WebSocket server on 3001, and the fixed MAC address keeps QQ's
/// device fingerprint stable across restarts.
fn compose_yaml(expose_public_access: bool) -> String {
    let bind = if expose_public_access { "" } else { "127.0.0.1:" };
    format!(
        r#"services:
  napcat:
    image: {IMAGE}
    container_name: ${{CONTAINER_PREFIX}}
    restart: unless-stopped
    network_mode: bridge
    mac_address: 02:42:ac:11:00:02
    environment:
      - NAPCAT_UID=0
      - NAPCAT_GID=0
      - MODE=ws
      - WEBUI_TOKEN=${{WEBUI_TOKEN}}
      - TZ=${{TZ}}
    ports:
      - "{bind}${{WS_PORT}}:3001"
      - "{bind}${{WEBUI_PORT}}:6099"
    volumes:
      - ./napcat/config:/app/napcat/config
      - ./ntqq:/app/.config/QQ
"#
    )
}

/// Wait until the published WebUI port answers. Any HTTP status counts: the WebUI may
/// redirect or 404 on `/` while still being up.
async fn wait_for_webui(port: u16) -> bool {
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(3)).build() else {
        return false;
    };
    for _ in 0..PROBE_ATTEMPTS {
        let url = format!("http://127.0.0.1:{port}/webui");
        if client.get(url).send().await.is_ok() {
            return true;
        }
        tokio::time::sleep(PROBE_INTERVAL).await;
    }
    false
}
