//! Installs WeChatPadPro (personal-WeChat iPad protocol bridge) as a plugin.
//!
//! The bridge ships as a Docker image that expects MySQL and Redis, and the compose stack
//! also carries a hook relay so nothing inside the containers has to reach the host. Both
//! install methods of the standard plugin pipeline are supported:
//!
//! - `docker`: the server runs the compose project itself, probes which published port
//!   answers the REST API, and records the connection once the stack is up.
//! - `command_docker`: nothing runs here. The compose project, env file and hook relay
//!   script are embedded base64 into a shell command the operator copies to the target
//!   Linux machine, like the standard command-mode installers. The recorded connection
//!   points at the target machine address when public access is exposed, otherwise at
//!   `127.0.0.1`.
//!
//! The wxid and the webhook token are left for the connection form: only the account owner
//! knows the wxid, and the relay pulls inbound messages so no inbound route is required.

use std::future::Future;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use serde_json::Value;

use super::super::{
    command_uninstall_command, plugin_from_request, save_plugin_record, InstallPluginRequest,
    PluginRecord,
};
use super::SpecialPluginInstaller;
use crate::setup_orchestrator::base64_encode;
use zihuan_core::ims_bot_adapter::runtime::wechat_pad::WeChatPadConfig;
use zihuan_core::ims_bot_adapter::BotAdapterConnection;
use zihuan_core::storage::{self, ConnectionConfig, ConnectionKind};

const IMAGE: &str = "wechatpadpro/wechatpadpro:latest";

const HOOK_JS: &str = include_str!("wechat_hook.mjs");
const DEFAULT_API_PORT: u16 = 8849;
const PROBE_ATTEMPTS: usize = 45;
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

pub struct WeChatPadProSpecialPluginInstaller;

/// Ports, secrets and the host the recorded connection points at, shared by both install
/// methods.
struct BridgeDeployment {
    api_port: u16,
    web_port: u16,
    hook_port: u16,
    admin_key: String,
    mysql_password: String,
    project: String,
    /// Publishes the ports on all interfaces instead of `127.0.0.1` so a zihuan_service
    /// running elsewhere can reach the bridge.
    expose_public_access: bool,
    connection_host: String,
}

impl SpecialPluginInstaller for WeChatPadProSpecialPluginInstaller {
    fn component_type(&self) -> &'static str {
        "wechat_pad"
    }

    fn install<'a>(
        &'a self,
        request: &'a InstallPluginRequest,
    ) -> Pin<Box<dyn Future<Output = Result<PluginRecord, String>> + Send + 'a>> {
        Box::pin(async move {
            if matches!(request.install_method.as_str(), "binary" | "command_binary") {
                return Err(
                    "WeChatPadPro 通过 Docker 运行，不支持 native 程序安装方式".to_string(),
                );
            }
            // Requests without a method come from clients predating the command mode and
            // always meant the local Docker install.
            if request.install_method == "command_docker" {
                install_command_mode(request).await
            } else {
                install_local_mode(request).await
            }
        })
    }

    fn uninstall<'a>(
        &'a self,
        plugin: &'a PluginRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move {
            let Some(compose_path) =
                plugin.extra_install_metadata.get("compose_path").and_then(Value::as_str)
            else {
                // Command-mode installs run elsewhere: delete_plugin already hands the
                // recorded uninstall command back to the operator.
                return Ok(());
            };
            let project = format!("zihuan-wechatpadpro-{}", short_id(&plugin.id));
            // Best effort: a stopped daemon must not block removing the plugin record.
            if let Err(error) = run_compose(
                Path::new(compose_path),
                &project,
                &["down", "--volumes", "--remove-orphans"],
            )
            .await
            {
                log::warn!("[wechat_pad installer] docker compose down failed: {error}");
            }
            if let Some(dir) = Path::new(compose_path).parent() {
                if let Err(error) = tokio::fs::remove_dir_all(dir).await {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        return Err(format!(
                            "删除 WeChatPadPro 数据目录 {} 失败: {error}",
                            dir.display()
                        ));
                    }
                }
            }
            Ok(())
        })
    }
}

async fn install_local_mode(request: &InstallPluginRequest) -> Result<PluginRecord, String> {
    if !docker_compose_available().await {
        return Err(
            "Docker Compose 不可用：WeChatPadPro 通过 Docker 运行，请先安装并启动 Docker"
                .to_string(),
        );
    }

    let mut plugin = plugin_from_request(request, "installing");
    plugin.installation_method = "docker".to_string();
    strip_unrelated_metadata(&mut plugin);
    let deployment = plan_deployment(request, &plugin.id);

    let install_dir = install_dir(&plugin.id);
    tokio::fs::create_dir_all(&install_dir).await.map_err(|error| {
        format!("创建 WeChatPadPro 数据目录 {} 失败: {error}", install_dir.display())
    })?;

    write_env(&install_dir, &deployment).await?;
    tokio::fs::write(install_dir.join("wechat_hook.mjs"), HOOK_JS)
        .await
        .map_err(|error| format!("写入 webhook 中继脚本失败: {error}"))?;
    let compose_path = install_dir.join("docker-compose.yaml");
    tokio::fs::write(&compose_path, compose_yaml(deployment.expose_public_access))
        .await
        .map_err(|error| format!("写入 WeChatPadPro compose 文件失败: {error}"))?;

    run_compose(&compose_path, &deployment.project, &["up", "-d"]).await?;

    // The image exposes the REST API on one of two container ports depending on the
    // build, so probe both published ports instead of guessing.
    let reachable = wait_for_api(&[deployment.api_port, deployment.web_port]).await;
    let pad_port = reachable.unwrap_or(deployment.api_port);

    let connection = bot_adapter_connection(
        &plugin,
        &deployment.connection_host,
        pad_port,
        deployment.hook_port,
        &deployment.admin_key,
    );
    storage::upsert_connection(connection.clone()).map_err(|error| error.to_string())?;

    plugin.connection_ids = vec![connection.config_id.clone()];
    plugin.status = "installed".to_string();
    plugin.updated_at = chrono::Utc::now().to_rfc3339();
    let metadata = &mut plugin.extra_install_metadata;
    metadata["connection_config"] = serde_json::to_value(&connection).unwrap_or(Value::Null);
    metadata["compose_path"] = Value::String(compose_path.to_string_lossy().to_string());
    metadata["data_dir"] = Value::String(install_dir.to_string_lossy().to_string());
    metadata["api_port"] = Value::from(pad_port);
    metadata["web_port"] = Value::from(deployment.web_port);
    metadata["hook_port"] = Value::from(deployment.hook_port);
    metadata["relay_url"] = Value::String(format!(
        "http://{}:{}",
        deployment.connection_host, deployment.hook_port
    ));
    metadata["pad_url"] = Value::String(format!(
        "http://{}:{}",
        deployment.connection_host, pad_port
    ));
    metadata["api_reachable"] = Value::Bool(reachable.is_some());
    metadata["uninstall_command"] = Value::String(format!(
        "docker compose -f {} -p {} down --volumes --remove-orphans",
        compose_path.display(),
        deployment.project
    ));
    if reachable.is_none() {
        metadata["warning"] = Value::String(
            "容器已启动，但 API 在探测期内未响应；请在连接配置里确认 pad_url，\
             或查看 `docker logs` 后重试"
                .to_string(),
        );
    }
    save_plugin_record(&plugin)?;
    Ok(plugin)
}

/// Builds the copy-command install: the stack is never started here, the operator runs
/// the generated command on the machine that has Docker.
async fn install_command_mode(request: &InstallPluginRequest) -> Result<PluginRecord, String> {
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
         printf '%s' '{}' | base64 -d > wechat_hook.mjs\n\
         docker compose -f docker-compose.yaml up -d",
        base64_encode(compose.as_bytes()),
        base64_encode(env.as_bytes()),
        base64_encode(HOOK_JS.as_bytes()),
    );

    let connection = bot_adapter_connection(
        &plugin,
        &deployment.connection_host,
        deployment.api_port,
        deployment.hook_port,
        &deployment.admin_key,
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
    metadata["api_port"] = Value::from(deployment.api_port);
    metadata["web_port"] = Value::from(deployment.web_port);
    metadata["hook_port"] = Value::from(deployment.hook_port);
    metadata["pad_url"] =
        Value::String(format!("http://{}:{}", deployment.connection_host, deployment.api_port));
    metadata["relay_url"] =
        Value::String(format!("http://{}:{}", deployment.connection_host, deployment.hook_port));
    metadata["project"] = Value::String(deployment.project.clone());
    save_plugin_record(&plugin)?;
    Ok(plugin)
}

/// The standard detailed config only feeds the expose/target flags to this installer; its
/// component sections are unrelated defaults that would only pollute the record.
fn strip_unrelated_metadata(plugin: &mut PluginRecord) {
    if let Some(metadata) = plugin.extra_install_metadata.as_object_mut() {
        metadata.remove("detailed_config");
    }
}

fn plan_deployment(request: &InstallPluginRequest, plugin_id: &str) -> BridgeDeployment {
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
    let api_port = pick_free_port(DEFAULT_API_PORT);
    let web_port = pick_free_port(api_port + 1);
    let hook_port = pick_free_port(web_port + 1);
    let admin_key = uuid::Uuid::new_v4().simple().to_string();
    // MySQL password shares the admin-key alphabet: hex only, safe inside the DSN.
    let mysql_password = uuid::Uuid::new_v4().simple().to_string();
    let project = format!("zihuan-wechatpadpro-{}", short_id(plugin_id));
    BridgeDeployment {
        api_port,
        web_port,
        hook_port,
        admin_key,
        mysql_password,
        project,
        expose_public_access,
        connection_host,
    }
}

fn bot_adapter_connection(
    plugin: &PluginRecord,
    host: &str,
    api_port: u16,
    hook_port: u16,
    admin_key: &str,
) -> ConnectionConfig {
    let wechat = WeChatPadConfig {
        pad_url: format!("http://{host}:{api_port}"),
        pad_token: admin_key.to_string(),
        self_wxid: String::new(),
        webhook_token: String::new(),
        webhook_url: None,
        relay_url: Some(format!("http://{host}:{hook_port}")),
        bot_names: Vec::new(),
        nickname: Some(plugin.name.clone()),
    };
    let kind = BotAdapterConnection {
        bot_server_url: String::new(),
        mock: None,
        adapter_server_url: None,
        bot_server_token: None,
        qq_id: None,
        napcat_install_path: None,
        wechat: Some(wechat),
    };
    ConnectionConfig {
        id: format!("plugin-wechat-pad-{}", plugin.id),
        config_id: format!("plugin-wechat-pad-{}", plugin.id),
        name: format!("{} (WeChatPadPro)", plugin.name),
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
        .join(format!("wechat_pad-{}", short_id(plugin_id)))
}

fn short_id(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect()
}

fn pick_free_port(start: u16) -> u16 {
    (start..start.saturating_add(50))
        .find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
        .unwrap_or(start)
}

async fn docker_compose_available() -> bool {
    matches!(
        tokio::process::Command::new("docker").args(["compose", "version"]).output().await,
        Ok(output) if output.status.success()
    )
}

async fn write_env(dir: &Path, deployment: &BridgeDeployment) -> Result<(), String> {
    tokio::fs::write(dir.join(".env"), env_file_contents(deployment))
        .await
        .map_err(|error| format!("写入 WeChatPadPro 环境变量失败: {error}"))
}

fn env_file_contents(deployment: &BridgeDeployment) -> String {
    let BridgeDeployment {
        api_port,
        web_port,
        hook_port,
        admin_key,
        mysql_password,
        project,
        ..
    } = deployment;
    format!(
        "# Generated by the Zihuan Next WeChatPadPro plugin installer.\n\
CONTAINER_PREFIX={project}\n\
ADMIN_KEY={admin_key}\n\
API_PORT={api_port}\n\
WEB_PORT={web_port}\n\
HOOK_PORT={hook_port}\n\
HOOK_SELF_URL=http://hook:8080/hook\n\
PAD_URLS=http://wechatpadpro:8080,http://wechatpadpro:1238\n\
TZ=Asia/Shanghai\n\
MYSQL_ROOT_PASSWORD={mysql_password}\n\
MYSQL_DATABASE=weixin\n\
MYSQL_USER=weixin\n\
MYSQL_PASSWORD={mysql_password}\n\
MYSQL_CONNECT_STR=weixin:{mysql_password}@tcp(mysql:3306)/weixin?charset=utf8mb4&parseTime=true&loc=Local\n"
    )
}

fn compose_yaml(expose_public_access: bool) -> String {
    let bind = if expose_public_access { "" } else { "127.0.0.1:" };
    format!(
        r#"services:
  wechatpadpro:
    image: {IMAGE}
    container_name: ${{CONTAINER_PREFIX}}
    restart: unless-stopped
    ports:
      - "{bind}${{API_PORT}}:8080"
      - "{bind}${{WEB_PORT}}:1238"
    # Lets the container reach a zihuan_service running on the host for webhook delivery.
    extra_hosts:
      - "host.docker.internal:host-gateway"
    environment:
      - DB_HOST=mysql
      - REDIS_HOST=redis
      - ADMIN_KEY=${{ADMIN_KEY}}
      - MYSQL_CONNECT_STR=${{MYSQL_CONNECT_STR}}
      - TZ=${{TZ}}
    depends_on:
      mysql:
        condition: service_healthy
      redis:
        condition: service_healthy
    networks:
      - wechatpadpro

  hook:
    image: node:22-alpine
    container_name: ${{CONTAINER_PREFIX}}-hook
    restart: unless-stopped
    command: ["node", "/app/wechat_hook.mjs"]
    environment:
      - HOOK_SELF_URL=${{HOOK_SELF_URL}}
      - PAD_URLS=${{PAD_URLS}}
      - PAD_TOKEN=${{ADMIN_KEY}}
    volumes:
      - ./wechat_hook.mjs:/app/wechat_hook.mjs:ro
    ports:
      - "{bind}${{HOOK_PORT}}:8080"
    networks:
      - wechatpadpro

  mysql:
    image: mysql:8.0
    container_name: ${{CONTAINER_PREFIX}}-mysql
    restart: unless-stopped
    environment:
      MYSQL_ROOT_PASSWORD: ${{MYSQL_ROOT_PASSWORD}}
      MYSQL_DATABASE: ${{MYSQL_DATABASE}}
      MYSQL_USER: ${{MYSQL_USER}}
      MYSQL_PASSWORD: ${{MYSQL_PASSWORD}}
    volumes:
      - mysql_data:/var/lib/mysql
    healthcheck:
      test: ["CMD", "mysqladmin", "ping", "-h", "localhost", "-u", "root", "-p${{MYSQL_ROOT_PASSWORD}}"]
      interval: 5s
      timeout: 5s
      retries: 30
    networks:
      - wechatpadpro

  redis:
    image: redis:6
    container_name: ${{CONTAINER_PREFIX}}-redis
    restart: unless-stopped
    command: redis-server --appendonly yes
    volumes:
      - redis_data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 5s
      retries: 30
    networks:
      - wechatpadpro

networks:
  wechatpadpro:

volumes:
  mysql_data:
  redis_data:
"#
    )
}

async fn run_compose(compose_path: &Path, project: &str, args: &[&str]) -> Result<(), String> {
    let output = tokio::process::Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(compose_path)
        .arg("-p")
        .arg(project)
        .args(args)
        .output()
        .await
        .map_err(|error| format!("执行 docker compose 失败: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "docker compose {} 失败: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// Wait until one of the published ports answers the bridge's REST API.
///
/// Any HTTP status counts: some builds 404 on `/api/v1/status` while still being up.
async fn wait_for_api(ports: &[u16]) -> Option<u16> {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(3)).build().ok()?;
    for _ in 0..PROBE_ATTEMPTS {
        for port in ports {
            let url = format!("http://127.0.0.1:{port}/api/v1/status");
            if client.get(url).send().await.is_ok() {
                return Some(*port);
            }
        }
        tokio::time::sleep(PROBE_INTERVAL).await;
    }
    None
}
