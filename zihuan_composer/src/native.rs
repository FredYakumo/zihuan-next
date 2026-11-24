use std::path::Path;

use crate::api::components::ComponentRecord;

pub async fn run(command: &mut tokio::process::Command) -> Result<(), String> {
    let output = command.output().await.map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "命令退出 {}：{}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

pub fn running_pid(directory: &Path, executable: &Path) -> Result<Option<sysinfo::Pid>, String> {
    let Ok(text) = std::fs::read_to_string(directory.join("runtime.pid")) else {
        return Ok(None);
    };
    let pid: usize = text.trim().parse().map_err(|_| "进程记录无效".to_string())?;
    let system = sysinfo::System::new_all();
    let pid = sysinfo::Pid::from(pid);
    let Some(process) = system.process(pid) else {
        return Ok(None);
    };
    let expected = executable.canonicalize().map_err(|error| error.to_string())?;
    let actual = process.exe().and_then(|path| path.canonicalize().ok());
    if actual.as_ref() != Some(&expected) {
        return Err("进程 ID 已被其他程序使用，拒绝操作；请核对后清理 runtime.pid".into());
    }
    Ok(Some(pid))
}

pub async fn stop_process(directory: &Path, executable: &Path) -> Result<(), String> {
    if let Some(pid) = running_pid(directory, executable)? {
        #[cfg(target_os = "windows")]
        run(tokio::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]))
            .await?;
        #[cfg(not(target_os = "windows"))]
        run(tokio::process::Command::new("kill").args(["-TERM", &pid.to_string()])).await?;
        for _ in 0..50 {
            if running_pid(directory, executable)?.is_none() {
                let _ = std::fs::remove_file(directory.join("runtime.pid"));
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        return Err("进程尚未退出，请稍后重试".into());
    }
    Ok(())
}

async fn package_manager() -> Result<&'static str, String> {
    for program in ["brew", "apt-get", "dnf", "pacman"] {
        if crate::installation::check_command(program, &["--version"]).await {
            return Ok(program);
        }
    }
    Err("未找到受支持的包管理器".into())
}

fn package(component: &ComponentRecord, manager: &str) -> Result<&'static str, String> {
    match component.component_type.as_str() {
        "mysql" if manager != "brew" => Ok("mysql-server"),
        "mysql" => Ok("mysql"),
        "redis" => Ok("redis"),
        "elasticsearch" => Ok("elasticsearch"),
        _ => Err("此 native 组件由原生安装程序管理，请在它的管理界面操作".into()),
    }
}

pub async fn set_running(component: &ComponentRecord, enabled: bool) -> Result<(), String> {
    if component.component_type == "weaviate" {
        #[cfg(target_os = "windows")]
        {
            let _ = enabled;
            return Err(crate::installation::weaviate::WINDOWS_UNSUPPORTED_MESSAGE.to_string());
        }
        #[cfg(not(target_os = "windows"))]
        {
            let directory = crate::config::root().join("components").join(&component.id);
            let install_root = directory.join("bin").join("weaviate");
            let executable = crate::installation::weaviate::find_weaviate_binary(&install_root)
                .ok_or("Weaviate 可执行文件不存在")?;
            if !enabled {
                return stop_process(&install_root, &executable).await;
            }
            let config = crate::api::components::component_config(component)?;
            return crate::config::COMPONENT_SCOPE
                .scope(
                    (component.id.clone(), directory),
                    crate::installation::weaviate::install_weaviate_binary(&config.search),
                )
                .await;
        }
    }
    let manager = package_manager().await?;
    let name = package(component, manager)?;
    let action = if enabled { "start" } else { "stop" };
    if manager == "brew" {
        run(tokio::process::Command::new("brew").args(["services", action, name])).await
    } else {
        let service = match component.component_type.as_str() {
            "mysql" => "mysql",
            "redis" if manager == "apt-get" => "redis-server",
            other => other,
        };
        run(tokio::process::Command::new("systemctl").args([action, service])).await
    }
}

pub async fn uninstall(component: &ComponentRecord) -> Result<(), String> {
    if component.component_type == "weaviate" {
        #[cfg(not(target_os = "windows"))]
        set_running(component, false).await?;
        let directory = crate::config::root()
            .join("components")
            .join(&component.id)
            .join("bin")
            .join("weaviate");
        return tokio::fs::remove_dir_all(directory).await.map_err(|error| error.to_string());
    }
    let manager = package_manager().await?;
    let name = package(component, manager)?;
    let mut command = tokio::process::Command::new(manager);
    match manager {
        "brew" => {
            command.args(["uninstall", name]);
        }
        "apt-get" => {
            command.args(["remove", "-y", name]);
        }
        "dnf" => {
            command.args(["remove", "-y", name]);
        }
        "pacman" => {
            command.args(["-R", "--noconfirm", name]);
        }
        _ => unreachable!(),
    }
    run(&mut command).await
}
