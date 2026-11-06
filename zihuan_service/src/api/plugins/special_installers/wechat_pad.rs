//! Installs WeChatPadPro (personal-WeChat iPad protocol bridge) as a plugin.
//!
//! The bridge ships as a Docker image that expects MySQL and Redis, so the installer writes a
//! self-contained compose project into the application data directory, starts it, probes which
//! published port answers the REST API, and records a `bot_adapter` connection pointing at it.
//! The wxid and the webhook URL are left for the connection form: only the account owner knows
//! the wxid, and only the operator knows the address the bridge can reach this server on.

use std::future::Future;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::time::Duration;

use serde_json::Value;

use super::super::{plugin_from_request, save_plugin_record, InstallPluginRequest, PluginRecord};
use super::SpecialPluginInstaller;
use zihuan_core::ims_bot_adapter::runtime::wechat_pad::WeChatPadConfig;
use zihuan_core::ims_bot_adapter::BotAdapterConnection;
use zihuan_core::storage::{self, ConnectionConfig, ConnectionKind};

const IMAGE: &str = "wechatpadpro/wechatpadpro:latest";
const DEFAULT_API_PORT: u16 = 8849;
const PROBE_ATTEMPTS: usize = 45;
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

pub struct WeChatPadProSpecialPluginInstaller;

impl SpecialPluginInstaller for WeChatPadProSpecialPluginInstaller {
    fn component_type(&self) -> &'static str {
        "wechat_pad"
    }

    fn install<'a>(
        &'a self,
        request: &'a InstallPluginRequest,
    ) -> Pin<Box<dyn Future<Output = Result<PluginRecord, String>> + Send + 'a>> {
        Box::pin(async move {
            if !docker_compose_available().await {
                return Err(
                    "Docker Compose 不可用：WeChatPadPro 通过 Docker 运行，请先安装并启动 Docker"
                        .to_string(),
                );
            }

            let mut plugin = plugin_from_request(request, "installing");
            plugin.installation_method = "docker".to_string();

            let install_dir = install_dir(&plugin.id);
            tokio::fs::create_dir_all(&install_dir).await.map_err(|error| {
                format!("创建 WeChatPadPro 数据目录 {} 失败: {error}", install_dir.display())
            })?;

            let api_port = pick_free_port(DEFAULT_API_PORT);
            let web_port = pick_free_port(api_port + 1);
            let admin_key = uuid::Uuid::new_v4().simple().to_string();
            // MySQL password shares the admin-key alphabet: hex only, safe inside the DSN.
            let mysql_password = uuid::Uuid::new_v4().simple().to_string();
            let project = format!("zihuan-wechatpadpro-{}", short_id(&plugin.id));

            write_env(&install_dir, api_port, web_port, &project, &admin_key, &mysql_password)
                .await?;
            let compose_path = install_dir.join("docker-compose.yaml");
            tokio::fs::write(&compose_path, compose_yaml())
                .await
                .map_err(|error| format!("写入 WeChatPadPro compose 文件失败: {error}"))?;

            run_compose(&compose_path, &project, &["up", "-d"]).await?;

            // The image exposes the REST API on one of two container ports depending on the
            // build, so probe both published ports instead of guessing.
            let reachable = wait_for_api(&[api_port, web_port]).await;
            let pad_port = reachable.unwrap_or(api_port);

            let connection = bot_adapter_connection(&plugin, pad_port, &admin_key);
            storage::upsert_connection(connection.clone()).map_err(|error| error.to_string())?;

            plugin.connection_ids = vec![connection.config_id.clone()];
            plugin.status = "installed".to_string();
            plugin.updated_at = chrono::Utc::now().to_rfc3339();
            plugin.extra_install_metadata["connection_config"] =
                serde_json::to_value(&connection).unwrap_or(Value::Null);
            plugin.extra_install_metadata["compose_path"] =
                Value::String(compose_path.to_string_lossy().to_string());
            plugin.extra_install_metadata["data_dir"] =
                Value::String(install_dir.to_string_lossy().to_string());
            plugin.extra_install_metadata["api_port"] = Value::from(pad_port);
            plugin.extra_install_metadata["web_port"] = Value::from(web_port);
            plugin.extra_install_metadata["pad_url"] =
                Value::String(format!("http://127.0.0.1:{pad_port}"));
            plugin.extra_install_metadata["api_reachable"] = Value::Bool(reachable.is_some());
            plugin.extra_install_metadata["uninstall_command"] = Value::String(format!(
                "docker compose -f {} -p {project} down --volumes --remove-orphans",
                compose_path.display()
            ));
            if reachable.is_none() {
                plugin.extra_install_metadata["warning"] = Value::String(
                    "容器已启动，但 API 在探测期内未响应；请在连接配置里确认 pad_url，\
                     或查看 `docker logs` 后重试"
                        .to_string(),
                );
            }
            save_plugin_record(&plugin)?;
            Ok(plugin)
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

fn bot_adapter_connection(plugin: &PluginRecord, port: u16, admin_key: &str) -> ConnectionConfig {
    let wechat = WeChatPadConfig {
        pad_url: format!("http://127.0.0.1:{port}"),
        pad_token: admin_key.to_string(),
        self_wxid: String::new(),
        webhook_token: uuid::Uuid::new_v4().simple().to_string(),
        webhook_url: None,
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

fn short_id(plugin_id: &str) -> String {
    plugin_id.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect()
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

async fn write_env(
    dir: &Path,
    api_port: u16,
    web_port: u16,
    project: &str,
    admin_key: &str,
    mysql_password: &str,
) -> Result<(), String> {
    let env = format!(
        "# Generated by the Zihuan Next WeChatPadPro plugin installer.\n\
CONTAINER_PREFIX={project}\n\
ADMIN_KEY={admin_key}\n\
API_PORT={api_port}\n\
WEB_PORT={web_port}\n\
TZ=Asia/Shanghai\n\
MYSQL_ROOT_PASSWORD={mysql_password}\n\
MYSQL_DATABASE=weixin\n\
MYSQL_USER=weixin\n\
MYSQL_PASSWORD={mysql_password}\n\
MYSQL_CONNECT_STR=weixin:{mysql_password}@tcp(mysql:3306)/weixin?charset=utf8mb4&parseTime=true&loc=Local\n"
    );
    tokio::fs::write(dir.join(".env"), env)
        .await
        .map_err(|error| format!("写入 WeChatPadPro 环境变量失败: {error}"))
}

fn compose_yaml() -> String {
    format!(
        r#"services:
  wechatpadpro:
    image: {IMAGE}
    container_name: ${{CONTAINER_PREFIX}}
    restart: unless-stopped
    ports:
      - "127.0.0.1:${{API_PORT}}:8080"
      - "127.0.0.1:${{WEB_PORT}}:1238"
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
