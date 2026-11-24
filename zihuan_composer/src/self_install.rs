use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::api::components::{
    component_from_request, render_error, save_component_record, ComponentRecord,
    InstallComponentRequest,
};
use crate::api::state::AppState;
use crate::orchestrator::emit_progress;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct NativeRuntime {
    pub executable: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

pub async fn install(request: InstallComponentRequest, depot: &mut Depot, res: &mut Response) {
    if !matches!(request.install_method.as_str(), "docker" | "native") {
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            "紫幻本体支持 docker 或 native 安装".into(),
        );
    }
    let state = depot.obtain::<Arc<AppState>>().unwrap().clone();
    let mut component = component_from_request(&request, "installing");
    let task_id = uuid::Uuid::new_v4().to_string();
    component.extra_install_metadata["install_task_id"] = Value::String(task_id.clone());
    if let Err(error) = save_component_record(&component) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    let (sender, _) = tokio::sync::broadcast::channel(256);
    state.register_task(task_id.clone(), sender.clone());
    let accepted = component.clone();
    let task = task_id.clone();
    tokio::spawn(async move {
        emit_progress(Some(&sender), "prepare", "running", "准备安装紫幻组件".into());
        let result = install_impl(&mut component, &request).await;
        match result {
            Ok(()) => {
                component.status = "installed".into();
                component.updated_at = chrono::Utc::now().to_rfc3339();
                match save_component_record(&component) {
                    Ok(()) => emit_progress(
                        Some(&sender),
                        "finished",
                        "success",
                        "紫幻组件安装完成".into(),
                    ),
                    Err(error) => emit_progress(Some(&sender), "failed", "error", error),
                }
            }
            Err(error) => {
                component.status = "failed".into();
                component.extra_install_metadata["last_error"] = Value::String(error.clone());
                let _ = save_component_record(&component);
                emit_progress(Some(&sender), "failed", "error", error);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        state.setup_tasks.lock().unwrap().remove(&task);
    });
    res.render(Json(json!({"accepted": true, "task_id": task_id, "component": accepted})));
}

async fn install_impl(
    component: &mut ComponentRecord,
    request: &InstallComponentRequest,
) -> Result<(), String> {
    let directory = crate::config::root().join("components").join(&component.id);
    tokio::fs::create_dir_all(&directory).await.map_err(|error| error.to_string())?;
    let metadata = &request.extra_install_metadata;
    let host = metadata.get("host").and_then(Value::as_str).unwrap_or("127.0.0.1");
    let port = metadata.get("port").and_then(Value::as_u64).unwrap_or(9951);
    if port == 0 || port > u16::MAX as u64 {
        return Err("端口必须在 1–65535 之间".into());
    }
    component.extra_install_metadata["webui_url"] = json!(format!("http://127.0.0.1:{port}"));
    if request.install_method == "docker" {
        let image = metadata.get("image").and_then(Value::as_str).filter(|value| !value.trim().is_empty())
            .ok_or("请填写紫幻 Docker 镜像名称（也可先在仓库执行 docker build -f docker/Dockerfile -t zihuan-next .）")?;
        let compose = json!({"services": {"zihuan": {
            "image": image,
            "restart": "unless-stopped",
            "ports": [format!("{host}:{port}:9951")],
            "environment": {"ZIHUAN_HOST": "0.0.0.0", "ZIHUAN_PORT": "9951", "XDG_CONFIG_HOME": "/data/config"},
            "volumes": ["./data:/data", "./plugins:/app/plugins", "./sub_agents:/app/sub_agents", "./scheduled_jobs:/app/scheduled_jobs"]
        }}});
        let path = directory.join("compose.yaml");
        for relative in ["data", "plugins", "sub_agents", "scheduled_jobs"] {
            tokio::fs::create_dir_all(directory.join(relative))
                .await
                .map_err(|error| error.to_string())?;
        }
        let source = serde_yaml::to_string(&compose).map_err(|error| error.to_string())?;
        let compose = crate::docker_config::compose(&source, &component.id)?;
        tokio::fs::write(&path, compose).await.map_err(|error| error.to_string())?;
        component.extra_install_metadata["compose_path"] = json!(path);
        save_component_record(component)?;
        return crate::native::run(
            tokio::process::Command::new("docker")
                .args(["compose", "-f"])
                .arg(&path)
                .args(["up", "-d"]),
        )
        .await;
    }
    let release_dir = directory.join("release");
    if release_dir.exists() {
        return Err(format!(
            "安装目录已存在：{}；请先卸载该组件，数据会保留",
            release_dir.display()
        ));
    }
    tokio::fs::create_dir_all(&release_dir)
        .await
        .map_err(|error| error.to_string())?;
    let source = metadata.get("source").and_then(Value::as_str).unwrap_or("");
    let result = prepare_release(source, &request.version, &release_dir, metadata).await;
    if let Err(error) = result {
        let _ = tokio::fs::remove_dir_all(&release_dir).await;
        return Err(error);
    }
    let executable = find_executable(&release_dir).ok_or("安装包中没有 zihuan_next 可执行文件")?;
    let config_dir = directory.join("data").join("config");
    tokio::fs::create_dir_all(&config_dir)
        .await
        .map_err(|error| error.to_string())?;
    let mut environment = BTreeMap::from([
        ("ZIHUAN_HOST".into(), host.into()),
        ("ZIHUAN_PORT".into(), port.to_string()),
        ("XDG_CONFIG_HOME".into(), config_dir.to_string_lossy().into_owned()),
    ]);
    if cfg!(target_os = "windows") {
        environment.insert("APPDATA".into(), config_dir.to_string_lossy().into_owned());
        environment.insert("LOCALAPPDATA".into(), config_dir.to_string_lossy().into_owned());
    }
    crate::config::write_yaml(
        &directory.join("runtime.yaml"),
        &NativeRuntime {
            executable,
            args: Vec::new(),
            environment,
        },
    )?;
    component.extra_install_metadata["install_path"] = json!(release_dir);
    save_component_record(component)?;
    set_running(component, true).await
}

async fn prepare_release(
    source: &str,
    version: &str,
    destination: &Path,
    metadata: &Value,
) -> Result<(), String> {
    if !source.is_empty() && Path::new(source).is_dir() {
        return copy_release(Path::new(source), destination);
    }
    let mut client = reqwest::Client::builder()
        .user_agent("zihuan-composer")
        .timeout(std::time::Duration::from_secs(900));
    if let Some(proxy) = metadata
        .get("http_proxy")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        client = client.proxy(reqwest::Proxy::all(proxy).map_err(|error| error.to_string())?);
    }
    let client = client.build().map_err(|error| error.to_string())?;
    let url = if source.is_empty() {
        let endpoint = if version == "latest" {
            "https://api.github.com/repos/FredYakumo/zihuan-next/releases/latest".to_string()
        } else {
            format!("https://api.github.com/repos/FredYakumo/zihuan-next/releases/tags/{version}")
        };
        let release: Value = client
            .get(endpoint)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json()
            .await
            .map_err(|error| error.to_string())?;
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => "Windows-x86_64-cpu.zip",
            ("linux", "x86_64") => "Linux-x86_64-cpu.tar.xz",
            ("macos", "aarch64") => "macOS-Apple-cpu.zip",
            _ => {
                return Err(
                    "此平台没有默认发布包，请在 source 中指定发布包 URL 或解压后的目录".into()
                )
            }
        };
        release["assets"]
            .as_array()
            .ok_or("GitHub 发布包列表为空")?
            .iter()
            .find(|asset| asset["name"].as_str().is_some_and(|name| name.ends_with(platform)))
            .and_then(|asset| asset["browser_download_url"].as_str())
            .ok_or("未找到此平台的 CPU 发布包")?
            .to_string()
    } else {
        source.to_string()
    };
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("source 需要是发布包 URL 或解压后的目录".into());
    }
    let bytes = client
        .get(&url)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .bytes()
        .await
        .map_err(|error| error.to_string())?;
    if url.ends_with(".zip") {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|error| error.to_string())?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            let relative = entry.enclosed_name().ok_or("发布包包含非法路径")?.to_path_buf();
            if entry.is_symlink() {
                return Err("发布包不能包含符号链接".into());
            }
            let path = destination.join(relative);
            if entry.is_dir() {
                std::fs::create_dir_all(path).map_err(|error| error.to_string())?;
            } else {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                let mut file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
                std::io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
                #[cfg(unix)]
                if let Some(mode) = entry.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
                        .map_err(|error| error.to_string())?;
                }
            }
        }
    } else if url.ends_with(".tar.xz") || url.ends_with(".tar.gz") {
        let archive = destination.with_extension("archive");
        tokio::fs::write(&archive, bytes).await.map_err(|error| error.to_string())?;
        let result = crate::native::run(
            tokio::process::Command::new("tar")
                .arg("-xf")
                .arg(&archive)
                .arg("-C")
                .arg(destination),
        )
        .await;
        let _ = tokio::fs::remove_file(archive).await;
        result?;
    } else {
        return Err("发布包格式需要为 zip、tar.gz 或 tar.xz".into());
    }
    Ok(())
}

fn copy_release(source: &Path, destination: &Path) -> Result<(), String> {
    let source = source.canonicalize().map_err(|error| error.to_string())?;
    let destination = destination.canonicalize().map_err(|error| error.to_string())?;
    if destination.starts_with(&source) {
        return Err("安装目录不能位于源目录内".into());
    }
    for entry in std::fs::read_dir(&source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        if kind.is_symlink() {
            return Err("源目录不能包含符号链接".into());
        }
        if kind.is_dir() {
            std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
            copy_release(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn find_executable(directory: &Path) -> Option<PathBuf> {
    let filename = if cfg!(target_os = "windows") {
        "zihuan_next.exe"
    } else {
        "zihuan_next"
    };
    let direct = directory.join(filename);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(directory).ok()?.flatten().find_map(|entry| {
        entry
            .file_type()
            .ok()?
            .is_dir()
            .then(|| find_executable(&entry.path()))
            .flatten()
    })
}

pub async fn set_running(component: &ComponentRecord, enabled: bool) -> Result<(), String> {
    let directory = crate::config::root().join("components").join(&component.id);
    let runtime: NativeRuntime = crate::config::read_yaml(&directory.join("runtime.yaml"))?;
    if !runtime.executable.is_file() {
        return Err("紫幻可执行文件不存在".into());
    }
    if enabled {
        if crate::native::running_pid(&directory, &runtime.executable)?.is_some() {
            return Ok(());
        }
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("runtime.log"))
            .map_err(|error| error.to_string())?;
        let child = tokio::process::Command::new(&runtime.executable)
            .args(&runtime.args)
            .envs(&runtime.environment)
            .current_dir(runtime.executable.parent().ok_or("可执行文件路径无效")?)
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone().map_err(|error| error.to_string())?)
            .stderr(log)
            .spawn()
            .map_err(|error| error.to_string())?;
        let pid = child.id().ok_or("无法获取紫幻进程 ID")?;
        tokio::fs::write(directory.join("runtime.pid"), pid.to_string())
            .await
            .map_err(|error| error.to_string())?;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if crate::native::running_pid(&directory, &runtime.executable)?.is_none() {
            return Err(format!(
                "紫幻启动后退出，请查看 {}",
                directory.join("runtime.log").display()
            ));
        }
    } else {
        crate::native::stop_process(&directory, &runtime.executable).await?;
    }
    Ok(())
}

pub async fn uninstall(component: &ComponentRecord) -> Result<(), String> {
    if crate::config::root()
        .join("components")
        .join(&component.id)
        .join("runtime.yaml")
        .exists()
    {
        set_running(component, false).await?;
    }
    let release = crate::config::root().join("components").join(&component.id).join("release");
    if release.exists() {
        tokio::fs::remove_dir_all(release).await.map_err(|error| error.to_string())?;
    }
    Ok(())
}
