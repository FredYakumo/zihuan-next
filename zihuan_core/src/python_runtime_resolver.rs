use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::time::timeout;

use crate::error::{Error, Result};
use crate::python_runtime::{PythonRuntimeConfig, PythonRuntimeKind};

const RUNTIME_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct PythonRuntimeCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl PythonRuntimeCommand {
    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command
    }

    pub fn display(&self) -> String {
        std::iter::once(self.program.display().to_string())
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn resolve_python_runtime(workspace_root: &Path, config: &PythonRuntimeConfig) -> Result<PythonRuntimeCommand> {
    match config.kind {
        PythonRuntimeKind::UvProject => Ok(PythonRuntimeCommand {
            program: PathBuf::from("uv"),
            args: vec!["run".to_string(), "python".to_string()],
        }),
        PythonRuntimeKind::ProjectVenv => {
            let executable = project_venv_python_path(workspace_root);
            if !executable.is_file() {
                return Err(Error::ValidationError(format!(
                    "项目 Python 虚拟环境不存在: {}",
                    executable.display()
                )));
            }
            Ok(PythonRuntimeCommand {
                program: executable,
                args: Vec::new(),
            })
        }
        PythonRuntimeKind::CustomExecutable => {
            let raw_path = config
                .executable_path
                .as_deref()
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .ok_or_else(|| Error::ValidationError("自定义 Python 解释器路径不能为空".to_string()))?;
            let executable = resolve_workspace_path(workspace_root, raw_path);
            if !executable.is_file() {
                return Err(Error::ValidationError(format!(
                    "自定义 Python 解释器不存在: {}",
                    executable.display()
                )));
            }
            Ok(PythonRuntimeCommand {
                program: executable,
                args: Vec::new(),
            })
        }
    }
}

pub fn project_venv_python_path(workspace_root: &Path) -> PathBuf {
    let venv_dir = workspace_root.join(".venv");
    if cfg!(windows) {
        venv_dir.join("Scripts").join("python.exe")
    } else {
        venv_dir.join("bin").join("python")
    }
}

pub async fn check_python_runtime(
    workspace_root: &Path,
    config: &PythonRuntimeConfig,
) -> Result<(PythonRuntimeCommand, String, String)> {
    let command_spec = resolve_python_runtime(workspace_root, config)?;
    if config.kind == PythonRuntimeKind::UvProject {
        let pyproject_path = workspace_root.join("pyproject.toml");
        if !pyproject_path.is_file() {
            return Err(Error::ValidationError(format!(
                "未检测到 pyproject.toml: {}",
                pyproject_path.display()
            )));
        }

        let mut command = Command::new("uv");
        command.arg("--version");
        let output = run_command_with_timeout(command, "uv").await?;
        if !output.status.success() {
            return Err(command_failure("uv 运行时检测失败", &output.stderr));
        }
        return Ok((
            command_spec,
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
            pyproject_path.display().to_string(),
        ));
    }

    let mut command = command_spec.to_command();
    command.arg("--version").current_dir(workspace_root);
    let output = run_command_with_timeout(command, "Python 运行时").await?;
    if !output.status.success() {
        return Err(command_failure("Python 运行时检测失败", &output.stderr));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = if version.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        version
    };
    Ok((command_spec.clone(), version, command_spec.program.display().to_string()))
}

async fn run_command_with_timeout(command: Command, name: &str) -> Result<std::process::Output> {
    let program = command.get_program().to_owned();
    let args: Vec<_> = command.get_args().map(|arg| arg.to_owned()).collect();
    let current_dir = command.get_current_dir().map(PathBuf::from);
    let mut async_command = tokio::process::Command::new(program);
    async_command
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(current_dir) = current_dir {
        async_command.current_dir(current_dir);
    }
    let mut child = async_command
        .spawn()
        .map_err(|error| Error::ValidationError(format!("无法启动{name}: {error}")))?;
    let status = match timeout(RUNTIME_CHECK_TIMEOUT, child.wait()).await {
        Ok(result) => result.map_err(|error| Error::ValidationError(format!("无法等待{name}: {error}")))?,
        Err(_) => {
            let _ = child.kill().await;
            return Err(Error::ValidationError(format!(
                "{name}检测超时（{} 秒），暂未检测到",
                RUNTIME_CHECK_TIMEOUT.as_secs()
            )));
        }
    };

    let mut stdout = Vec::new();
    if let Some(mut stream) = child.stdout.take() {
        stream
            .read_to_end(&mut stdout)
            .await
            .map_err(|error| Error::ValidationError(format!("无法读取{name}输出: {error}")))?;
    }
    let mut stderr = Vec::new();
    if let Some(mut stream) = child.stderr.take() {
        stream
            .read_to_end(&mut stderr)
            .await
            .map_err(|error| Error::ValidationError(format!("无法读取{name}错误输出: {error}")))?;
    }
    Ok(std::process::Output { status, stdout, stderr })
}

fn command_failure(prefix: &str, stderr: &[u8]) -> Error {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    Error::ValidationError(format!(
        "{prefix}: {}",
        if stderr.is_empty() { "暂未检测到" } else { &stderr }
    ))
}

fn resolve_workspace_path(workspace_root: &Path, raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        path
    } else {
        workspace_root.join(path)
    }
}
