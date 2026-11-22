use std::path::PathBuf;

use clap::Subcommand;
use serde_json::{json, Value};

#[derive(Subcommand)]
pub enum Command {
    /// 启动交互式终端界面（默认）
    Tui,
    /// 显示 Composer 和本体仓库版本及构建模式
    BuildInfo,
    /// 输出组件生成的安装命令
    InstallCommand { id: String },
    /// 列出已管理的组件
    List,
    /// 查看组件详情（含部署元数据）
    Show { id: String },
    /// 获取 WeChatPadPro 登录二维码及链接
    LoginQr { id: String },
    /// 查询 WeChatPadPro 登录状态
    LoginStatus {
        id: String,
        #[arg(long, default_value = "")]
        auth_key: String,
    },
    /// 检测本机运行环境
    Environment,
    /// 生成安装请求 YAML 模板（含随机凭证）
    Template {
        #[arg(value_parser = ["zihuan", "mysql", "sqlite", "redis", "weaviate", "elasticsearch", "rustfs", "napcat", "wechat_pad"])]
        kind: String,
    },
    /// 从 YAML 安装组件，并等待任务完成
    Install { file: PathBuf },
    /// 启动组件
    Start { id: String },
    /// 停止组件
    Stop { id: String },
    /// 卸载组件（可能删除安装器管理的数据）
    Uninstall { id: String },
    /// 查看部署 YAML；--file 保存新的配置
    Config {
        id: String,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// 查看连接 YAML；--file 导入连接配置
    Connections {
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// 导出全部或指定组件的连接 JSON
    Export {
        file: PathBuf,
        /// 仅导出指定组件的连接
        #[arg(long)]
        id: Option<String>,
    },
}

pub async fn run(command: Option<Command>, url: &str, token: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|error| error.to_string())?;
    match command {
        Some(command) => execute(command, &client, url, token).await,
        None => crate::tui::run(url, token).await,
    }
}

pub(crate) async fn request(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let mut request = client
        .request(
            method.parse::<reqwest::Method>().map_err(|error| error.to_string())?,
            format!("{url}/api/{path}"),
        )
        .bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    let value: Value = response.json().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(value["error"].as_str().unwrap_or("Composer 请求失败").into());
    }
    Ok(value)
}

async fn execute(
    command: Command,
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<(), String> {
    let (method, path, body) = match command {
        Command::Tui => return Err("界面入口应由主程序启动".into()),
        Command::BuildInfo => ("GET", "build-info".into(), None),
        Command::InstallCommand { id } => {
            let component = crate::api::components::find_component(&id)?;
            let command = component.extra_install_metadata["install_command"]
                .as_str()
                .ok_or("此组件没有生成的安装命令")?;
            println!("{command}");
            return Ok(());
        }
        Command::List => ("GET", "components".into(), None),
        Command::Show { id } => {
            let component = crate::api::components::find_component(&id)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&component).map_err(|error| error.to_string())?
            );
            return Ok(());
        }
        Command::LoginQr { id } => (
            "POST",
            "components/wechat_pad/login_qr".into(),
            Some(json!({"component_id": id})),
        ),
        Command::LoginStatus { id, auth_key } => (
            "POST",
            "components/wechat_pad/login_status".into(),
            Some(json!({"component_id": id, "auth_key": auth_key})),
        ),
        Command::Environment => ("GET", "environment".into(), None),
        Command::Template { kind } => {
            let value =
                request(client, url, token, "GET", &format!("template/{kind}"), None).await?;
            println!("{}", value["yaml"].as_str().unwrap_or_default());
            return Ok(());
        }
        Command::Install { file } => {
            let text = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
            let value: crate::api::components::InstallComponentRequest =
                serde_yaml::from_str(&text).map_err(|error| error.to_string())?;
            (
                "POST",
                "components/install".into(),
                Some(serde_json::to_value(value).map_err(|error| error.to_string())?),
            )
        }
        Command::Start { id } => ("POST", format!("components/{id}/enable"), None),
        Command::Stop { id } => ("POST", format!("components/{id}/disable"), None),
        Command::Uninstall { id } => ("DELETE", format!("components/{id}"), None),
        Command::Config { id, file } => {
            let path = format!("components/{id}/configuration");
            if let Some(file) = file {
                (
                    "PUT",
                    path,
                    Some(
                        json!({"yaml": std::fs::read_to_string(file).map_err(|error| error.to_string())?}),
                    ),
                )
            } else {
                let value = request(client, url, token, "GET", &path, None).await?;
                println!("{}", value["yaml"].as_str().unwrap_or_default());
                return Ok(());
            }
        }
        Command::Connections { file } => {
            if let Some(file) = file {
                (
                    "PUT",
                    "connections/yaml".into(),
                    Some(
                        json!({"yaml": std::fs::read_to_string(file).map_err(|error| error.to_string())?}),
                    ),
                )
            } else {
                let value = request(client, url, token, "GET", "connections/yaml", None).await?;
                println!("{}", value["yaml"].as_str().unwrap_or_default());
                return Ok(());
            }
        }
        Command::Export { file, id } => {
            let mut value = request(client, url, token, "GET", "connections", None).await?;
            if let Some(id) = id {
                let component = crate::api::components::find_component(&id)?;
                filter_connections(&mut value, &component.connection_ids);
            }
            std::fs::write(
                file,
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            println!("连接配置已导出");
            return Ok(());
        }
    };
    let result = request(client, url, token, method, &path, body).await?;
    println!("{}", serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?);
    if let Some(id) = result["task_id"].as_str() {
        let mut displayed = 0;
        loop {
            let value = request(client, url, token, "GET", &format!("tasks/{id}"), None).await?;
            for event in value.as_array().ok_or("进度响应无效")?.iter().skip(displayed) {
                println!(
                    "[{}] {}",
                    event["step"].as_str().unwrap_or_default(),
                    event["message"].as_str().unwrap_or_default()
                );
                displayed += 1;
                if event["status"] == "error" {
                    return Err(event["message"].as_str().unwrap_or("操作失败").into());
                }
                if event["step"] == "finished" {
                    return Ok(());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    }
    Ok(())
}

/// GUI、CLI、TUI 使用同一组件记录中的连接 ID 选择导出内容。
pub(crate) fn filter_connections(value: &mut Value, ids: &[String]) {
    if let Some(connections) = value.as_array_mut() {
        connections.retain(|connection| {
            connection["config_id"]
                .as_str()
                .is_some_and(|id| ids.iter().any(|saved| saved == id))
        });
    }
}
