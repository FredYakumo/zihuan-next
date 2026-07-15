use std::path::{Path, PathBuf};
use std::process::Command;

use zihuan_core::error::{Error, Result};
use zihuan_core::python_runtime::{PythonRuntimeConfig, PythonRuntimeKind};

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

pub fn check_python_runtime(
    workspace_root: &Path,
    config: &PythonRuntimeConfig,
) -> Result<(PythonRuntimeCommand, String, String)> {
    let command_spec = resolve_python_runtime(workspace_root, config)?;
    let output = command_spec
        .to_command()
        .arg("--version")
        .current_dir(workspace_root)
        .output()
        .map_err(|error| Error::ValidationError(format!("无法启动 Python 运行时: {error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(Error::ValidationError(format!(
            "Python 运行时检测失败: {}",
            if stderr.is_empty() { "unknown error" } else { &stderr }
        )));
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let version = if version.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        version
    };
    let executable_output = command_spec
        .to_command()
        .arg("-c")
        .arg("import sys; print(sys.executable)")
        .current_dir(workspace_root)
        .output()
        .map_err(|error| Error::ValidationError(format!("无法读取 Python 可执行文件路径: {error}")))?;
    if !executable_output.status.success() {
        let stderr = String::from_utf8_lossy(&executable_output.stderr).trim().to_string();
        return Err(Error::ValidationError(format!(
            "无法读取 Python 可执行文件路径: {}",
            if stderr.is_empty() { "unknown error" } else { &stderr }
        )));
    }
    let executable_path = String::from_utf8_lossy(&executable_output.stdout).trim().to_string();
    if executable_path.is_empty() {
        return Err(Error::ValidationError("Python 运行时未返回可执行文件路径".to_string()));
    }
    Ok((command_spec, version, executable_path))
}

fn resolve_workspace_path(workspace_root: &Path, raw_path: &str) -> PathBuf {
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        path
    } else {
        workspace_root.join(path)
    }
}
